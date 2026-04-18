---
syntax: add-region [region] [display-name]
description: 新規地域のY字路データをローカルDBに追加する
allowed-tools: Bash
---

新規地域のY字路データを追加する。以下の手順を順番に実行すること。

- region: ${1} （例: taiwan）
- display-name: ${2:-${1}} （例: 台湾全土）

作業開始前に、対象地域のbboxをユーザーに確認すること（例: `119.9,21.9,122.1,25.4`）。
regionが指定されていない場合も確認すること。

## Step 1: インポートバイナリのビルド

```bash
set -euo pipefail
cd backend && cargo build --release --bin import --bin import_two_way --bin import-baidu-panoid
```

## Step 2: 本番DBの全データをローカルに取り込む

`/sync-from-prod` スキルを実行する。

## Step 3: OSMデータのダウンロード

Geofabrik (https://download.geofabrik.de/) から `${1}-latest.osm.pbf` をダウンロードして `~/y-junctions-data/osm/` に配置する。

```bash
set -euo pipefail
curl -fLo ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  https://download.geofabrik.de/.../${1}-latest.osm.pbf
```

URLはGeofabrikのサイトで確認して正しいパスを使うこと。

## Step 4: ローカルCockroachDBに新規地域をインポート

```bash
set -euo pipefail
cd backend
./target/release/import \
  --input ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  --bbox <bbox>

./target/release/import_two_way \
  --input ~/y-junctions-data/osm/${1}-latest.osm.pbf \
  --bbox <bbox>
```

件数が増えていることを確認する:

```bash
set -euo pipefail
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "SELECT COUNT(*) FROM y_junctions;"
```

## Step 5: Baidu panoid 取得

無条件に実行する。バイナリ内部で `is_in_china_mainland()` により対象行をフィルタするので、
中国本土外の地域では `Found 0 mainland-China Y-junctions` で即終了する（副作用なし）。

```bash
set -euo pipefail
cd backend && ./target/release/import-baidu-panoid
```

中国本土を含む地域の場合は `Found N mainland-China Y-junctions to query Baidu` → `ok=<件数>`
のログで成功件数を確認する。大量の `none=` が続く場合は `is_in_china_mainland()` の bbox
除外ロジックが想定外の地域を中国扱いしている可能性あり（日本・韓国の沿岸部など境界付近）。

## Step 6: 本番DBに反映

対象地域のbboxを引数として `/deploy-data` スキルを実行する。

```text
/deploy-data <bbox>
```

## Step 7: doc/data-updates.md を更新

以下の形式で履歴の先頭に追記する。3-wayと2-wayの件数はStep 4のインポートログから確認する。

```
- YYYY-MM-DD: **${2:-${1}}データ追加**
  - 総件数: X件（前回Y件から+Z件）
  - 追加地域: ${2:-${1}}（bbox: <bbox>）
  - 内訳:
    - 3-way Y字路: X件
    - 2-way Y字路: Y件
```

## Step 8: PRを作成

```bash
set -euo pipefail
# worktree 運用で既に data/${1} ブランチに居る場合もあるので、無ければ作成・あれば切り替え
git checkout -b data/${1} 2>/dev/null || git checkout data/${1}
git add doc/data-updates.md
git commit -m "data: Add ${2:-${1}} Y-junction data"
gh pr create --title "data: ${2:-${1}}のY字路データを追加"
```
