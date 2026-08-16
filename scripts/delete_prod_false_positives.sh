#!/usr/bin/env bash
# issue #319 の誤検出ノード（way 本数 3 / 実分岐 4 以上）を本番 DB から削除する。
#
# パイプラインの load-to-cockroach は ON CONFLICT DO NOTHING の追記専用なので、
# 再実行しても既存の誤検出行は消えない。削除はこの経路で明示的に行う。
#
# 実行前に scripts/backup_prod_junctions.sh でバックアップを取っておくこと。
# y_junctions に FK は無いため、enrich 済みの baidu_panoramas /
# google_streetview_coverage の行は明示的に消さないと孤児として残る。
#
# usage: scripts/delete_prod_false_positives.sh <delete-ids.csv> [--apply]
# --apply を付けない限り件数の確認だけで終わる（dry-run）。
set -euo pipefail

DELETE_IDS="${1:?delete-ids.csv required}"
APPLY="${2:-}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROD_CRDB_URI=$(cd "$REPO_ROOT/terraform" && terraform output -raw cockroachdb_connection_uri)
[ -n "$PROD_CRDB_URI" ] || { echo "prod URI empty"; exit 1; }

COUNT=$(wc -l < "$DELETE_IDS" | tr -d ' ')
echo "delete-ids: $COUNT"

# マージ失敗や取り違えで巨大な削除集合を流し込む事故を防ぐ。
# #319 の全件集計は 64,283 件。桁が違えば止める。
[ "$COUNT" -gt 0 ] || { echo "empty delete set, aborting"; exit 1; }
[ "$COUNT" -lt 100000 ] || { echo "delete set suspiciously large ($COUNT), aborting"; exit 1; }

STAGING=nodes_to_delete_323

echo "== before =="
cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "
  SELECT 'y_junctions' AS t, COUNT(*) FROM y_junctions
  UNION ALL SELECT 'baidu_panoramas', COUNT(*) FROM baidu_panoramas
  UNION ALL SELECT 'google_streetview_coverage', COUNT(*) FROM google_streetview_coverage;"

if [ "$APPLY" != "--apply" ]; then
  echo "dry-run: --apply を付けると実際に削除する"
  exit 0
fi

echo "== staging テーブルに削除対象 ID を投入 =="
cockroach sql --url "$PROD_CRDB_URI" -e "DROP TABLE IF EXISTS $STAGING;"
cockroach sql --url "$PROD_CRDB_URI" -e "CREATE TABLE $STAGING (osm_node_id INT8 PRIMARY KEY);"
cockroach userfile upload "$DELETE_IDS" delete_ids_323.csv --url "$PROD_CRDB_URI"
cockroach sql --url "$PROD_CRDB_URI" -e \
  "IMPORT INTO $STAGING (osm_node_id) CSV DATA ('userfile:///delete_ids_323.csv');"

STAGED=$(cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "SELECT COUNT(*) FROM $STAGING;" | tail -1)
echo "staged: $STAGED (expected $COUNT)"
[ "$STAGED" = "$COUNT" ] || { echo "staged count mismatch, aborting before delete"; exit 1; }

# 1 トランザクションで 64k 行消すと CockroachDB の txn サイズ上限に触れうるので
# LIMIT 付きで削り切るまで回す。
delete_batched() {
  local table="$1" total=0 n
  while :; do
    n=$(cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "
      DELETE FROM $table
      WHERE osm_node_id IN (SELECT osm_node_id FROM $STAGING)
      LIMIT 5000
      RETURNING osm_node_id;" | tail -n +2 | wc -l | tr -d ' ')
    total=$((total + n))
    echo "  $table: deleted $total"
    [ "$n" -gt 0 ] || break
  done
}

echo "== 削除 =="
delete_batched google_streetview_coverage
delete_batched baidu_panoramas
delete_batched y_junctions

echo "== after =="
cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "
  SELECT 'y_junctions' AS t, COUNT(*) FROM y_junctions
  UNION ALL SELECT 'baidu_panoramas', COUNT(*) FROM baidu_panoramas
  UNION ALL SELECT 'google_streetview_coverage', COUNT(*) FROM google_streetview_coverage;"

echo "== 残存確認（0 であること）=="
cockroach sql --url "$PROD_CRDB_URI" --format=csv -e "
  SELECT COUNT(*) AS remaining FROM y_junctions
  WHERE osm_node_id IN (SELECT osm_node_id FROM $STAGING);"

echo "== 後片付け =="
cockroach sql --url "$PROD_CRDB_URI" -e "DROP TABLE $STAGING;"
cockroach userfile delete delete_ids_323.csv --url "$PROD_CRDB_URI"

echo "done"
