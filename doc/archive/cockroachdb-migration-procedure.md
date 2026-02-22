# Neon → CockroachDB ローカル移行手順

## 前提

- CockroachDB コンテナが起動済みでスキーマ適用済みであること
- Terraform の出力から Neon の接続 URI が取得できること

## 1. Neon からエクスポート

```bash
NEON_URI=$(cd terraform && terraform output -raw neon_connection_uri)

docker run --rm -v ~/y-junctions-data:/data postgres:15-alpine \
  psql "$NEON_URI" -c "\copy (
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
  ) TO '/data/neon_export.csv' WITH CSV HEADER"
```

出力: `~/y-junctions-data/neon_export.csv`

## 2. CockroachDB へインポート

```bash
# ヘッダー行を除いた CSV を用意
tail -n +2 ~/y-junctions-data/neon_export.csv > ~/y-junctions-data/neon_export_noheader.csv

# コンテナにコピー
docker cp ~/y-junctions-data/neon_export_noheader.csv y-junctions-cockroachdb:/tmp/neon_export.csv

# インポート（id は BIGSERIAL のため列リストから除外する）
docker exec -it y-junctions-cockroachdb ./cockroach sql \
  --insecure --database=y_junction \
  -e "\copy y_junctions (osm_node_id, location, angle_1, angle_2, angle_3, bearings, elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, elevation_diff_1, elevation_diff_2, elevation_diff_3, min_angle_index, min_elevation_diff, max_elevation_diff, way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel, way_3_bridge, way_3_tunnel, way_1_highway_type, way_2_highway_type, way_3_highway_type, created_at) FROM '/tmp/neon_export.csv' CSV"
```

## 3. ローカル起動

```bash
# CockroachDB 起動
docker start y-junctions-cockroachdb

# バックエンド（backend/.env の DATABASE_URL が CockroachDB を向いていること）
cd backend && cargo run

# フロントエンド
cd frontend && npm run dev
```

`backend/.env`:
```
DATABASE_URL=postgresql://root@localhost:26257/y_junction?sslmode=disable
```
