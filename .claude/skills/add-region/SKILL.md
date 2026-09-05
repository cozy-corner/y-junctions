---
name: add-region
description: 新規地域のY字路データをローカルDBに追加する
allowed-tools: Bash
---

新規地域のY字路データを追加する。以下の手順を順番に実行すること。

## 引数

このスキルの起動時に渡された引数から以下を読み取る。

- **REGION**: Geofabrik のリージョン名（例: `taiwan`）。第1引数。
- **DISPLAY_NAME**: 日本語の表示名（例: `台湾全土`）。第2引数。省略時は REGION をそのまま使う。

以降の bash ブロックに出てくる `${REGION}` / `${DISPLAY_NAME}` は
**シェル変数ではなく、実行時にこの値へ置き換えるプレースホルダ**。
各 bash ブロックは独立サブシェルなので変数は引き継がれない。
ブロック先頭の代入行に実際の値を埋めてから実行すること。

作業開始前に、対象地域のbboxをユーザーに確認すること（例: `119.9,21.9,122.1,25.4`）。
REGION が指定されていない場合も確認すること。

## Step 1: インポートバイナリのビルド

```bash
set -euo pipefail
cd backend && cargo build --release --bin import --bin import_two_way --bin import-baidu-panoid
```

## Step 2: 本番DBの全データをローカルに取り込む

`/sync-from-prod` スキルを実行する。

## Step 3: OSMデータのダウンロード

Geofabrik (https://download.geofabrik.de/) から `${REGION}-latest.osm.pbf` をダウンロードして `~/y-junctions-data/osm/` に配置する。

```bash
set -euo pipefail
REGION=<region>   # 引数の値を埋める
curl -fLo ~/y-junctions-data/osm/${REGION}-latest.osm.pbf \
  https://download.geofabrik.de/.../${REGION}-latest.osm.pbf
```

URLはGeofabrikのサイトで確認して正しいパスを使うこと。

## Step 4: ローカルCockroachDBに新規地域をインポート

```bash
set -euo pipefail
REGION=<region>   # 引数の値を埋める
cd backend
./target/release/import \
  --input ~/y-junctions-data/osm/${REGION}-latest.osm.pbf \
  --bbox <bbox>

./target/release/import_two_way \
  --input ~/y-junctions-data/osm/${REGION}-latest.osm.pbf \
  --bbox <bbox>
```

件数が増えていることを確認する:

```bash
set -euo pipefail
cockroach sql --url "postgresql://root@localhost:26257/y_junction?sslmode=disable" \
  --execute "SELECT COUNT(*) FROM y_junctions;"
```

## Step 5: Baidu panoid 取得（**中国本土を追加する場合のみ**）

**このステップは追加する地域が中国本土に重なるときだけ実行する。それ以外の地域
（シンガポール・日本・台湾・香港・マカオ・韓国など）では実行しないこと。**

理由: `import-baidu-panoid` は百度の非公式エンドポイントへ、ブラウザ偽装 User-Agent と
ボット検知回避のペーシング付きでアクセスするスクレイピング処理。しかも Step 2 で
ローカルDBは本番の全複製になっているため、このバイナリは**追加地域だけでなく
ローカルDB全体の「panoid 未取得の中国本土ノード」**を対象に外部リクエストを発火し、
結果を `baidu_panoramas` に書き込む。中国本土と無関係な地域の追加でこれを走らせると、
無関係タスクでの無断外部アクセス・ローカル状態変更・百度側レート制限リスクを招くため不可。
（「中国本土外なら 0 件で即終了・副作用なし」は誤り。本番複製に中国データがある限り発火する。）

判定: 追加する bbox が中国本土の範囲（おおよそ lng 73–135, lat 18–54。ただし香港・マカオ・
台湾・日本の島嶼・韓国・北朝鮮東岸・ロシア沿海州は除外。厳密な定義は
`backend/src/domain/china.rs` の `is_in_china_mainland`）と重なるか:

- **重ならない → このステップをスキップして Step 6 へ。**
- 重なる → 外部リクエストが発生する旨をユーザーに伝え、了承を得てから実行する。

```bash
set -euo pipefail
cd backend && ./target/release/import-baidu-panoid
```

`Found N mainland-China Y-junctions to query Baidu` → `ok=<件数>` のログで成功件数を確認する。
N は**ローカルDBに残る panoid 未取得の中国本土ノード総数**であり、今回追加した地域だけの
件数ではない点に注意。大量の `none=` が続く場合は `is_in_china_mainland()` の bbox 除外
ロジックが境界付近（日本・韓国の沿岸部など）を中国扱いしている可能性あり。

## Step 6: 本番DBに反映

対象地域のbboxを引数として `/deploy-data` スキルを実行する。

```text
/deploy-data <bbox>
```

## Step 7: doc/data-updates.md を更新

以下の形式で履歴の先頭に追記する。3-wayと2-wayの件数はStep 4のインポートログから確認する。

```
- YYYY-MM-DD: **${DISPLAY_NAME}データ追加**
  - 総件数: X件（前回Y件から+Z件）
  - 追加地域: ${DISPLAY_NAME}（bbox: <bbox>）
  - 内訳:
    - 3-way Y字路: X件
    - 2-way Y字路: Y件
```

## Step 8: 主要都市ジャンプに追加

追加地域の代表都市を、サイドバーの「主要都市へジャンプ」ドロップダウンに加える。

`frontend/src/constants/cities.ts` の `CITIES` に 1 行追加する（座標は都市中心の WGS84。
追加した bbox 内に収まること）:

```ts
{ name: '<都市名>', country: '<国・地域名>', lat: <lat>, lon: <lon> },
```

- `country` は optgroup ラベル。新しい国・地域なら新グループが自動生成される。
- 新しい国・地域を足した場合、`frontend/src/components/CityJumpSelect.test.tsx` の
  optgroup 数アサートを +1 する（既存の国・地域に足すだけなら不要）。

検証:

```bash
set -euo pipefail
cd frontend
npm run typecheck && npm run lint
npx vitest run src/components/CityJumpSelect.test.tsx
```

## Step 9: PRを作成

```bash
set -euo pipefail
REGION=<region>              # 引数の値を埋める
DISPLAY_NAME=<display-name>  # 引数の値を埋める（省略時は region）
# worktree 運用で既に data/${REGION} ブランチに居る場合もあるので、無ければ作成・あれば切り替え
git checkout -b data/${REGION} 2>/dev/null || git checkout data/${REGION}
# データ追加・都市ジャンプは項目ごとに別コミットにする
git add doc/data-updates.md
git commit -m "data: Add ${DISPLAY_NAME} Y-junction data"
git add frontend/src/constants/cities.ts frontend/src/components/CityJumpSelect.test.tsx
git commit -m "feat: 主要都市ジャンプに${DISPLAY_NAME}を追加"
gh pr create --title "data: ${DISPLAY_NAME}のY字路データを追加"
```
