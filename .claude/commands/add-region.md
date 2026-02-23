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
cd backend && cargo build --release --bin import --bin import_two_way
```

## Step 2: 本番DBの全データをローカルに取り込む

`/sync-from-prod` スキルを実行する。

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
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "SELECT COUNT(*) FROM y_junctions;"
```

## Step 5: 本番DBに反映

`/deploy-data` スキルを実行する。

## Step 6: doc/data-updates.md を更新

以下の形式で履歴の先頭に追記する。3-wayと2-wayの件数はStep 4のインポートログから確認する。

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
