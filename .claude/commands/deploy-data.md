---
syntax: deploy-data
description: ローカルCockroachDBの全データを本番DBに反映する
allowed-tools: Bash
---

ローカルCockroachDBの全データを本番CockroachDBに反映する。

## Step 1: 本番DBの接続情報を取得

```bash
export PATH="/opt/homebrew/opt/libpq/bin:$PATH"
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)
```

## Step 2: ローカルDBの全データをエクスポート

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

件数を確認する（空でないことを確認してから次へ進む）:

```bash
wc -l ~/y-junctions-data/local_export_noheader.csv
```

## Step 3: GCSに一時バケットを作成してアップロード

```bash
gsutil mb -l asia-southeast1 gs://y-junctions-import-tmp/
gsutil iam ch allUsers:objectViewer gs://y-junctions-import-tmp/
gsutil cp ~/y-junctions-data/local_export_noheader.csv gs://y-junctions-import-tmp/
```

## Step 4: 本番DBを空にして全件インポート

```bash
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

## Step 5: 件数確認と一時バケットの削除

```bash
cockroach sql --url "$PROD_CRDB_URI" -e "SELECT COUNT(*) FROM y_junctions;"
gsutil rm -r gs://y-junctions-import-tmp/
```
