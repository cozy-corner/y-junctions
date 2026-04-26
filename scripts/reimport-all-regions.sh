#!/usr/bin/env bash
# Re-import all regions (Japan, Taiwan, Korea, Shanghai) into local y_junction DB
# using freshly-built import binaries. Used for verifying the merge-back filter
# on full-scale data.
#
# Prerequisites:
#   - release binaries built: (cd backend && cargo build --release --bin import --bin import_two_way)
#   - ~/y-junctions-data/osm/ contains the regional PBF files
#   - Docker CockroachDB (y-junctions-cockroachdb) running
#
# Run from the repo root.

set -euo pipefail

DB_URL="${DATABASE_URL:-postgresql://root@localhost:26257/y_junction?sslmode=disable}"
export DATABASE_URL="$DB_URL"

PBF_DIR="${HOME}/y-junctions-data/osm"
IMPORT_3WAY="./backend/target/release/import"
IMPORT_2WAY="./backend/target/release/import_two_way"

if [[ ! -x "$IMPORT_3WAY" || ! -x "$IMPORT_2WAY" ]]; then
  echo "ERROR: release binaries not found. Run: (cd backend && cargo build --release --bin import --bin import_two_way)" >&2
  exit 1
fi

# (region_name, pbf_filename, bbox)
# Bboxes intentionally generous; ON CONFLICT (osm_node_id) DO NOTHING de-dupes
# across overlapping Japan region PBFs.
REGIONS=(
  "hokkaido|hokkaido-latest.osm.pbf|139,40,146,46"
  "tohoku|tohoku-latest.osm.pbf|139,36,143,42"
  "kanto|kanto-latest.osm.pbf|138,34,141,38"
  "chubu|chubu-latest.osm.pbf|135,33,140,38"
  "kansai|kansai-latest.osm.pbf|134,33,137,36"
  "chugoku|chugoku-latest.osm.pbf|130,33,135,36"
  "shikoku|shikoku-latest.osm.pbf|132,32,135,35"
  "kyushu|kyushu-latest.osm.pbf|127,24,132,34"
  "taiwan|taiwan-latest.osm.pbf|119.9,21.9,122.1,25.4"
  "korea|south-korea-latest.osm.pbf|124.6,33.1,130.9,38.6"
  "shanghai|china-latest.osm.pbf|121.30,31.10,121.65,31.40"
)

echo "=== TRUNCATE y_junctions ==="
docker exec y-junctions-cockroachdb ./cockroach sql --insecure \
  --database=y_junction --execute "TRUNCATE TABLE y_junctions;"

for entry in "${REGIONS[@]}"; do
  IFS='|' read -r name pbf bbox <<<"$entry"
  pbf_path="$PBF_DIR/$pbf"
  if [[ ! -f "$pbf_path" ]]; then
    echo "SKIP: $name (missing PBF: $pbf_path)"
    continue
  fi
  echo
  echo "=== $name: 3-way ==="
  "$IMPORT_3WAY" --input "$pbf_path" --bbox "$bbox" 2>&1 \
    | grep -E "Found|imported|Importing" | tail -5
  echo "=== $name: 2-way ==="
  "$IMPORT_2WAY" --input "$pbf_path" --bbox "$bbox" 2>&1 \
    | grep -E "Found|imported|Importing" | tail -5
done

echo
echo "=== Final totals ==="
docker exec y-junctions-cockroachdb ./cockroach sql --insecure \
  --database=y_junction --execute "
SELECT
  CASE
    WHEN lon BETWEEN 121.30 AND 121.65 AND lat BETWEEN 31.10 AND 31.40 THEN 'shanghai'
    WHEN lon BETWEEN 119.9 AND 122.1 AND lat BETWEEN 21.9 AND 25.4 THEN 'taiwan'
    WHEN lon BETWEEN 124.6 AND 130.9 AND lat BETWEEN 33.1 AND 38.6 THEN 'korea'
    ELSE 'japan'
  END AS region,
  COUNT(*) AS count
FROM y_junctions
GROUP BY region
ORDER BY region;
SELECT COUNT(*) AS grand_total FROM y_junctions;"
