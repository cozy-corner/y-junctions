---
syntax: sync-from-prod
description: 本番DBの全データをローカルCockroachDBに取り込む
allowed-tools: Bash
---

本番DBの全データをローカルCockroachDBに取り込む。

前提: mainブランチのworktreeのdocker-compose.ymlで起動した `y-junctions-cockroachdb` コンテナが動いていること。
ボリューム `y-junctions_cockroachdata` を使用しているか確認: `docker inspect y-junctions-cockroachdb`

```bash
PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

# ローカルCockroachDB（port 26257）の既存データを削除
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "DELETE FROM y_junctions;"

# 本番DBからCSVエクスポート（~/y-junctions-data/prod_export.csv に出力）
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

# CSVをy-junctions-cockroachdbコンテナのexternディレクトリに配置
docker cp ~/y-junctions-data/prod_export.csv y-junctions-cockroachdb:/cockroach/cockroach-data/extern/prod_export.csv

# IMPORT INTO でローカルCockroachDBにインポート（nodelocal://1/ はコンテナのexternディレクトリを参照）
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "IMPORT INTO y_junctions (osm_node_id, location, angle_1, angle_2, angle_3, bearings, elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, elevation_diff_1, elevation_diff_2, elevation_diff_3, min_angle_index, min_elevation_diff, max_elevation_diff, way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel, way_3_bridge, way_3_tunnel, way_1_highway_type, way_2_highway_type, way_3_highway_type, created_at) CSV DATA ('nodelocal://1/prod_export.csv') WITH skip = '1';"
```

件数を確認する:

```bash
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "SELECT COUNT(*) FROM y_junctions;"
```
