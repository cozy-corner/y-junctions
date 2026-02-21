# 新規地域データの追加手順

新しい地域のY字路データをローカルDBにインポートし、本番DBに反映するまでの手順。

---

## 前提条件

- `docker` が起動済みであること（ローカル CockroachDB コンテナ含む）
- `gcloud` / `gsutil` がインストール済みで認証済みであること（`gcloud auth list` で確認）
- `cockroach` CLI がインストール済みであること
- インポートバイナリがビルド済みであること

```bash
cd backend && cargo build --release --bin import --bin import_two_way
```

---

## Step 1: OSM データのダウンロード

[Geofabrik](https://download.geofabrik.de/) から対象地域の `.osm.pbf` ファイルをダウンロードし、`~/y-junctions-data/osm/` に配置する。

```bash
# 例: 台湾
curl -o ~/y-junctions-data/osm/taiwan-latest.osm.pbf \
  https://download.geofabrik.de/asia/taiwan-latest.osm.pbf
```

---

## Step 2: ローカル CockroachDB にインポート

bbox は対象地域に合わせて変更すること。

```bash
# 3-way Y字路
cd backend && ./target/release/import \
  --input ~/y-junctions-data/osm/<region>-latest.osm.pbf \
  --bbox <min_lon>,<min_lat>,<max_lon>,<max_lat>

# 2-way Y字路
cd backend && ./target/release/import_two_way \
  --input ~/y-junctions-data/osm/<region>-latest.osm.pbf \
  --bbox <min_lon>,<min_lat>,<max_lon>,<max_lat>
```

インポート後、ローカル DB の件数を確認する。

```bash
cockroach sql --insecure --database=y_junction \
  -e "SELECT COUNT(*) FROM y_junctions;"
```

---

## Step 3: ローカル DB から CSV をエクスポート

```bash
docker run --rm -v ~/y-junctions-data:/data postgres:15-alpine \
  psql "postgresql://root@host.docker.internal:26257/y_junction?sslmode=disable" -c "\copy (
    SELECT
      osm_node_id,
      ST_AsEWKT(location::geometry) AS location,
      angle_1, angle_2, angle_3,
      bearings, elevation,
      neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3,
      elevation_diff_1, elevation_diff_2, elevation_diff_3,
      min_angle_index, min_elevation_diff, max_elevation_diff,
      way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel,
      way_3_bridge, way_3_tunnel,
      way_1_highway_type, way_2_highway_type, way_3_highway_type,
      created_at
    FROM y_junctions
  ) TO '/data/local_export.csv' WITH CSV HEADER"

# ヘッダーなし CSV を作成
tail -n +2 ~/y-junctions-data/local_export.csv > ~/y-junctions-data/local_export_noheader.csv
```

---

## Step 4: 本番 DB の接続 URI を取得

```bash
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)
```

---

## Step 5: GCS に一時バケットを作成してアップロード

```bash
# バケット作成
gsutil mb -l asia-southeast1 gs://y-junctions-import-tmp/

# 公開設定（IMPORT INTO が認証なしでアクセスできるようにする）
gsutil iam ch allUsers:objectViewer gs://y-junctions-import-tmp/

# CSV アップロード
gsutil cp ~/y-junctions-data/local_export_noheader.csv gs://y-junctions-import-tmp/
```

---

## Step 6: 本番 DB にインポート

```bash
cockroach sql --url "$PROD_CRDB_URI" -e "IMPORT INTO y_junctions (
  osm_node_id, location, angle_1, angle_2, angle_3,
  bearings, elevation,
  neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3,
  elevation_diff_1, elevation_diff_2, elevation_diff_3,
  min_angle_index, min_elevation_diff, max_elevation_diff,
  way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel,
  way_3_bridge, way_3_tunnel,
  way_1_highway_type, way_2_highway_type, way_3_highway_type,
  created_at
) CSV DATA ('https://storage.googleapis.com/y-junctions-import-tmp/local_export_noheader.csv');"
```

`status: succeeded` と `rows: 期待件数` が表示されれば成功。

---

## Step 7: 件数確認と一時バケットの削除

```bash
# 件数確認
cockroach sql --url "$PROD_CRDB_URI" -e "SELECT COUNT(*) FROM y_junctions;"

# 一時バケット削除
gsutil rm -r gs://y-junctions-import-tmp/
```

---

## Step 8: doc/data-updates.md を更新

以下の形式で追記する。

```
- YYYY-MM-DD: **地域名データ追加**
  - 総件数: X件（前回Y件から+Z件）
  - 追加地域: 地域名（bbox: min_lon,min_lat,max_lon,max_lat）
  - 内訳:
    - 3-way Y字路: X件
    - 2-way Y字路: Y件
```

---

## Step 9: PR を作成

ブランチ名を `data/<region>` にして PR を作成する。
ブランチ名が `data/*` にマッチすると `data` ラベルが自動付与され、リリースノートに含まれる。

```bash
git checkout -b data/<region>
git add doc/data-updates.md
git commit -m "data: Add <region> Y-junction data"
gh pr create --title "data: <地域名>のY字路データを追加"
```

---

## 注意事項

- `COPY` 系（psql の `\copy` / `COPY FROM STDIN`）は CockroachDB Cloud と互換性がなく使用不可。必ず `IMPORT INTO` を使うこと。
- `gs://` スキームは CockroachDB 側に認証情報が必要なため使用不可。`https://storage.googleapis.com/` を使うこと。
- `IMPORT INTO` は**追記（additive）**であり、既存データを置き換えない。ただし `osm_node_id`（主キー）が重複するとキー衝突エラーで失敗する。

### IMPORT INTO を使う際の2つのパターン

**パターン A: 新規地域のみをエクスポートして追記する（推奨）**

本番 DB に既存データがある場合はこちら。ローカル DB から新規地域のデータのみを抽出してエクスポートする。

```bash
# Step 3 のエクスポートを新規地域に絞る
docker run --rm -v ~/y-junctions-data:/data postgres:15-alpine \
  psql "postgresql://root@host.docker.internal:26257/y_junction?sslmode=disable" -c "\copy (
    SELECT ... FROM y_junctions
    WHERE <新規地域の条件（例: bboxによるフィルタ）>
  ) TO '/data/local_export.csv' WITH CSV HEADER"
```

**パターン B: 本番 DB を全件置き換える**

本番 DB を一度空にしてから全データをインポートする。ローカル DB に全地域のデータが揃っていることを確認してから実施すること。

```bash
# 本番 DB を空にする
psql "$PROD_CRDB_URI" -c "DELETE FROM y_junctions;"

# Step 3 以降を通常通り実行（全データをエクスポート → IMPORT INTO）
```
