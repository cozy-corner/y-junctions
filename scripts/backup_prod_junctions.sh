#!/usr/bin/env bash
# 本番 DB の Y字路関連 3 テーブルを CSV にダンプする。
#
# issue #323 の削除作業のように本番データを破壊的に変更する前に実行する。
# 列順は deploy-data skill の IMPORT INTO と揃えてあるので、
# そのまま userfile 経由で復旧できる。
#
# usage: scripts/backup_prod_junctions.sh [out_dir]
set -euo pipefail

OUT_DIR="${1:-$HOME/y-junctions-data/backup-issue-323}"
mkdir -p "$OUT_DIR"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROD_CRDB_URI=$(cd "$REPO_ROOT/terraform" && terraform output -raw cockroachdb_connection_uri)
[ -n "$PROD_CRDB_URI" ] || { echo "prod URI empty"; exit 1; }

# --format=csv は先頭にヘッダ行を出す。IMPORT INTO はヘッダを行として読むので落とす。
dump() {
  local name="$1" query="$2"
  echo "dumping $name ..."
  cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "$query" | tail -n +2 > "$OUT_DIR/$name.csv"
  echo "  $(wc -l < "$OUT_DIR/$name.csv") rows -> $OUT_DIR/$name.csv"
}

dump y_junctions "
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
  FROM y_junctions ORDER BY osm_node_id"

dump baidu_panoramas "
  SELECT osm_node_id, panoid, pano_mc_x, pano_mc_y, queried_at
  FROM baidu_panoramas ORDER BY osm_node_id"

dump google_streetview_coverage "
  SELECT osm_node_id, has_coverage, queried_at
  FROM google_streetview_coverage ORDER BY osm_node_id"

echo "backup complete: $OUT_DIR"
