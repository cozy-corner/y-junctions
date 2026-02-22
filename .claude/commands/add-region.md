---
syntax: add-region [region] [display-name]
description: 新規地域のY字路データをローカルDBに追加し、本番DBに反映する
allowed-tools: Bash
---

新規地域のY字路データを追加する。以下の手順を順番に実行すること。

- region: ${1} （例: taiwan）
- display-name: ${2:-${1}} （例: 台湾全土）

作業開始前に、対象地域のbboxをユーザーに確認すること（例: `119.9,21.9,122.1,25.4`）。
regionが指定されていない場合も確認すること。

## Step 1: インポートバイナリのビルド

```bash
cd backend && cargo build --release --bin import --bin import_two_way
```

## Step 2: 本番DBの全データをローカルに取り込む

```bash
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "DELETE FROM y_junctions;"

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

docker cp ~/y-junctions-data/prod_export.csv y-junctions-cockroachdb:/tmp/prod_export.csv

docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "\copy y_junctions (osm_node_id, location, angle_1, angle_2, angle_3, bearings, elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, elevation_diff_1, elevation_diff_2, elevation_diff_3, min_angle_index, min_elevation_diff, max_elevation_diff, way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel, way_3_bridge, way_3_tunnel, way_1_highway_type, way_2_highway_type, way_3_highway_type, created_at) FROM '/tmp/prod_export.csv' CSV"
```

件数を確認する:

```bash
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "SELECT COUNT(*) FROM y_junctions;"
```

## Step 3: OSMデータのダウンロード

Geofabrik (https://download.geofabrik.de/) から `${1}-latest.osm.pbf` をダウンロードして `~/y-junctions-data/osm/` に配置する。

```bash
curl -o ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  https://download.geofabrik.de/.../${1}-latest.osm.pbf
```

URLはGeofabrikのサイトで確認して正しいパスを使うこと。

## Step 4: ローカルCockroachDBに新規地域をインポート

```bash
cd backend && ./target/release/import \
  --input ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  --bbox <bbox>

cd backend && ./target/release/import_two_way \
  --input ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  --bbox <bbox>
```

件数が増えていることを確認する:

```bash
docker exec y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "SELECT COUNT(*) FROM y_junctions;"
```

## Step 5: ローカルDBの全データをエクスポート

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

tail -n +2 ~/y-junctions-data/local_export.csv > ~/y-junctions-data/local_export_noheader.csv
```

## Step 6: GCSに一時バケットを作成してアップロード

```bash
gsutil mb -l asia-southeast1 gs://y-junctions-import-tmp/
gsutil iam ch allUsers:objectViewer gs://y-junctions-import-tmp/
gsutil cp ~/y-junctions-data/local_export_noheader.csv gs://y-junctions-import-tmp/
```

## Step 7: 本番DBを空にして全件インポート

```bash
export PATH="/opt/homebrew/opt/libpq/bin:$PATH"
psql "$PROD_CRDB_URI" -c "DELETE FROM y_junctions;"

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

`status: succeeded` と件数が表示されることを確認する。

## Step 8: 件数確認と一時バケットの削除

```bash
cockroach sql --url "$PROD_CRDB_URI" -e "SELECT COUNT(*) FROM y_junctions;"
gsutil rm -r gs://y-junctions-import-tmp/
```

## Step 9: doc/data-updates.md を更新

以下の形式で履歴の先頭に追記する。3-wayと2-wayの件数はStep 4のインポートログから確認する。

```
- YYYY-MM-DD: **${2:-${1}}データ追加**
  - 総件数: X件（前回Y件から+Z件）
  - 追加地域: ${2:-${1}}（bbox: <bbox>）
  - 内訳:
    - 3-way Y字路: X件
    - 2-way Y字路: Y件
```

## Step 10: PRを作成

```bash
git checkout -b data/${1}
git add doc/data-updates.md
git commit -m "data: Add ${2:-${1}} Y-junction data"
gh pr create --title "data: ${2:-${1}}のY字路データを追加"
```
