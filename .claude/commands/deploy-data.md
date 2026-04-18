---
syntax: deploy-data [bbox]
description: ローカルDBの指定bbox範囲のY字路データを本番DBに差分追加する
allowed-tools: Bash
---

ローカルCockroachDBから指定 bbox 範囲の Y字路データを本番CockroachDBに追加する。
`add-region` と組み合わせて、新規地域を本番に反映するための差分追加用。

引数: bbox（`min_lon,min_lat,max_lon,max_lat`）。例: `121.30,31.10,121.65,31.40`

**注意:** bbox が既存の本番データと重なっていると `osm_node_id` の UNIQUE 制約で
IMPORT INTO が失敗する。新規地域は非重複 bbox を選ぶこと。
全件フル同期が必要な場合（スキーマ変更後の再構築など）は、このskillではなく
手動で TRUNCATE + 全件 IMPORT INTO を実施する。

Step 4 以降で失敗した場合は userfile が残るので、手動で
`cockroach userfile delete junctions.csv --url "$PROD_CRDB_URI"` で掃除する。

## Step 1: bbox の検証と本番DB接続情報の取得

各 bash ブロックは独立サブシェル。後続ブロックでも bbox (`${1}`) と prod URI を
取得し直す必要がある。

```bash
set -euo pipefail

BBOX="${1:?bbox argument required}"
echo "$BBOX" | grep -Eq '^-?[0-9]+\.?[0-9]*,-?[0-9]+\.?[0-9]*,-?[0-9]+\.?[0-9]*,-?[0-9]+\.?[0-9]*$' \
  || { echo "invalid bbox format: $BBOX"; exit 1; }

PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)
[ -n "$PROD_CRDB_URI" ] || { echo "prod URI empty"; exit 1; }

echo "bbox: $BBOX"
echo "prod uri obtained"
```

## Step 2: ローカルDBの対象bbox範囲をCSVに書き出す

```bash
set -euo pipefail

BBOX="${1}"
MIN_LON=$(echo "$BBOX" | cut -d, -f1)
MIN_LAT=$(echo "$BBOX" | cut -d, -f2)
MAX_LON=$(echo "$BBOX" | cut -d, -f3)
MAX_LAT=$(echo "$BBOX" | cut -d, -f4)

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
      baidu_panoid, baidu_pano_mc_x, baidu_pano_mc_y,
      created_at
    FROM y_junctions
    WHERE lon BETWEEN $MIN_LON AND $MAX_LON
      AND lat BETWEEN $MIN_LAT AND $MAX_LAT
  ) TO '/data/junctions.csv' WITH CSV"

LINE_COUNT=$(wc -l < ~/y-junctions-data/junctions.csv)
[ "$LINE_COUNT" -gt 0 ] || { echo "export produced 0 rows, aborting"; exit 1; }
echo "exported $LINE_COUNT rows"
```

## Step 3: userfile に CSV をアップロード

CockroachDB 組み込みの userfile ストレージ（公式推奨、約15MB以下向け、外部バケット不要）を使う。
Cloud Serverless では nodelocal が使えないため userfile を選択。

```bash
set -euo pipefail

PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

cockroach userfile upload ~/y-junctions-data/junctions.csv \
  junctions.csv --url "$PROD_CRDB_URI"
```

## Step 4: 本番DBに差分追加（IMPORT INTO、TRUNCATE なし）

```bash
set -euo pipefail

PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

cockroach sql --url "$PROD_CRDB_URI" -e "IMPORT INTO y_junctions (
  osm_node_id, location, angle_1, angle_2, angle_3,
  bearings, elevation,
  neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3,
  elevation_diff_1, elevation_diff_2, elevation_diff_3,
  min_angle_index, min_elevation_diff, max_elevation_diff,
  way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel,
  way_3_bridge, way_3_tunnel,
  way_1_highway_type, way_2_highway_type, way_3_highway_type,
  baidu_panoid, baidu_pano_mc_x, baidu_pano_mc_y,
  created_at
) CSV DATA ('userfile:///junctions.csv');"
```

出力に `status: succeeded` と投入件数が出ていることを確認する。

## Step 5: 件数確認と userfile の削除

```bash
set -euo pipefail

BBOX="${1}"
MIN_LON=$(echo "$BBOX" | cut -d, -f1)
MIN_LAT=$(echo "$BBOX" | cut -d, -f2)
MAX_LON=$(echo "$BBOX" | cut -d, -f3)
MAX_LAT=$(echo "$BBOX" | cut -d, -f4)

PROD_CRDB_URI=$(cd terraform && terraform output -raw cockroachdb_connection_uri)

# bbox 内の件数が Step 2 の wc -l と一致することを確認
cockroach sql --url "$PROD_CRDB_URI" -e "
  SELECT COUNT(*) FROM y_junctions
  WHERE lon BETWEEN $MIN_LON AND $MAX_LON
    AND lat BETWEEN $MIN_LAT AND $MAX_LAT;"

cockroach sql --url "$PROD_CRDB_URI" -e "SELECT COUNT(*) FROM y_junctions;"
cockroach userfile delete junctions.csv --url "$PROD_CRDB_URI"
```
