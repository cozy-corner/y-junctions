#!/usr/bin/env bash
# 全リージョンの PBF に対して audit_passing_way_junctions.py を回す。
#
# 入力の node_ids.csv は監査対象ノード ID の一覧（1 行 1 ID）。
# 出力は $OUT_DIR/audit-<region>.json（audit スクリプトの生出力）。
#
# usage: scripts/audit_all_regions.sh <node_ids.csv> [out_dir]
set -euo pipefail

NODE_IDS="${1:?node_ids.csv required}"
OUT_DIR="${2:-$HOME/y-junctions-data/audit-issue-323}"
OSM_DIR="$HOME/y-junctions-data/osm"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

mkdir -p "$OUT_DIR"

REGIONS=(
  south-korea kanto chubu kyushu taiwan kansai
  tohoku chugoku shikoku hokkaido china malaysia-singapore-brunei
)

# osmium も python もシングルスレッドなので並列に流す。
# PBF の読み出しは I/O 律速で、4 並列くらいが手元では頭打ち。
MAX_PARALLEL="${MAX_PARALLEL:-4}"

for region in "${REGIONS[@]}"; do
  pbf="$OSM_DIR/${region}-latest.osm.pbf"
  [ -f "$pbf" ] || { echo "missing PBF: $pbf"; exit 1; }

  out="$OUT_DIR/audit-${region}.json"
  if [ -s "$out" ]; then
    echo "skip ${region} (already done)"
    continue
  fi

  while [ "$(jobs -rp | wc -l)" -ge "$MAX_PARALLEL" ]; do wait -n; done

  echo "start ${region}"
  python3 "$REPO_ROOT/scripts/audit_passing_way_junctions.py" \
    "$NODE_IDS" "$pbf" "$out" 2> "$OUT_DIR/audit-${region}.log" &
done

wait
echo "all regions done -> $OUT_DIR"
