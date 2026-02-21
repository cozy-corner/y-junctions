# 新規地域データの追加手順

新しい地域のY字路データをローカルDBに追加し、本番DBに反映するまでの手順。

**基本方針**: 作業開始前に本番DBの全データをローカルに取り込み、新規地域を追加してから全件を本番に反映する。

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

## Step 1: 本番 DB の全データをローカルに取り込む

```bash
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

# ローカル DB を空にする
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "DELETE FROM y_junctions;"

# 本番 DB からエクスポート
docker run --rm -v ~/y-junctions-data:/data postgres:15-alpine \
  psql "$PROD_CRDB_URI" -c "\copy (
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
  ) TO '/data/prod_export.csv' WITH CSV HEADER"

# ローカル DB にインポート
docker cp ~/y-junctions-data/prod_export.csv y-junctions-cockroachdb:/tmp/prod_export.csv
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "\copy y_junctions (osm_node_id, location, angle_1, angle_2, angle_3, bearings, elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, elevation_diff_1, elevation_diff_2, elevation_diff_3, min_angle_index, min_elevation_diff, max_elevation_diff, way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel, way_3_bridge, way_3_tunnel, way_1_highway_type, way_2_highway_type, way_3_highway_type, created_at) FROM '/tmp/prod_export.csv' CSV"
```

件数が本番と一致していることを確認する。

```bash
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "SELECT COUNT(*) FROM y_junctions;"
```

---

## Step 2: OSM データのダウンロード

[Geofabrik](https://download.geofabrik.de/) から対象地域の `.osm.pbf` ファイルをダウンロードし、`~/y-junctions-data/osm/` に配置する。

```bash
# 例: 台湾
curl -o ~/y-junctions-data/osm/taiwan-latest.osm.pbf \
  https://download.geofabrik.de/asia/taiwan-latest.osm.pbf
```

---

## Step 3: ローカル CockroachDB に新規地域をインポート

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

インポート後、件数が増えていることを確認する。

```bash
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "SELECT COUNT(*) FROM y_junctions;"
```

---

## Step 4: ローカル DB の全データをエクスポート

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

## Step 5: 本番 DB の接続 URI を取得

```bash
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)
```

---

## Step 6: GCS に一時バケットを作成してアップロード

```bash
# バケット作成
gsutil mb -l asia-southeast1 gs://y-junctions-import-tmp/

# 公開設定（IMPORT INTO が認証なしでアクセスできるようにする）
gsutil iam ch allUsers:objectViewer gs://y-junctions-import-tmp/

# CSV アップロード
gsutil cp ~/y-junctions-data/local_export_noheader.csv gs://y-junctions-import-tmp/
```

---

## Step 7: 本番 DB を空にして全件インポート

```bash
# 本番 DB を空にする
export PATH="/opt/homebrew/opt/libpq/bin:$PATH"
psql "$PROD_CRDB_URI" -c "DELETE FROM y_junctions;"

# 全件インポート
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

## Step 8: 件数確認と一時バケットの削除

```bash
# 件数確認
cockroach sql --url "$PROD_CRDB_URI" -e "SELECT COUNT(*) FROM y_junctions;"

# 一時バケット削除
gsutil rm -r gs://y-junctions-import-tmp/
```

---

## Step 9: doc/data-updates.md を更新

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

## Step 10: PR を作成

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
- `IMPORT INTO` は追記（additive）であり、主キー（`osm_node_id`）が重複するとキー衝突エラーになる。そのため Step 7 で先に `DELETE` してから実行する。
