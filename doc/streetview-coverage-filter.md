# ストリートビュー非カバレッジノードの除外（Google 圏）

## 背景・課題

海外（中国以外）のY字路データが増えるにつれ、**Google ストリートビューの画像が存在しないノード**が地図に表示されるようになった。マーカーを開いても空/壊れたビューにしか繋がらず、体験を損なう。これらを除外したい。

### なぜ現状のデータでは除外できないか

- Google 用 `streetview_url` は座標＋方位から**実行時に生成される**だけで、Google 側にパノラマが実在するかの情報を保持していない（`backend/src/domain/junction.rs:104` の `Junction::streetview_url()`）。座標がある限り常に非空 URL が作られるため、DB 上で有無を判定できない。
- したがって「カバレッジ有無」を外部（Google metadata API）に問い合わせて DB に保存し、それを見て除外する仕組みが要る。

## 設計の出発点：要件から最小構成を組む

中国は Baidu で似た問題（無カバレッジ除外）を解決済みで、本設計は**その構造（専用テーブル・`osm_node_id` 主キー・3状態・ローカル照会 bin・Rust 側 China フィルタ・`/deploy-data` 経由の反映）を踏襲する**。ただし Baidu 固有の要件（URL 生成に `panoid`＋パノラマ座標という**実データの消費**が必要。`streetview_enricher.rs:57` の `baidu_panorama_url`）に由来する部分は落とす。

**Google は URL を座標から生成できるため、必要なのは「カバレッジの真偽」1 ビットだけ**。したがって Baidu との差分は実質「**パノラマ座標2列と（未稼働の）pipeline/cron を持たない**」だけになる。要件を満たすのに必要なのは以下の 3 つ：

1. カバレッジ有無を知る手段 → Google metadata API（`radius=10&source=outdoor`）。代替なし・必須。
2. その真偽を serving DB に置く → バックエンドが API 時に読めること。
3. 除外する → enricher で無しノードを落とす。

## 決定事項

| 論点 | 決定 |
| --- | --- |
| 照会対象 | 中国以外の全ノード（**日本を含む**）。地域で例外を作らず一律照会し、無ければ除外。 |
| カバレッジ取得手段 | 公式 **Street View Static API の metadata エンドポイント**。IAM/OAuth 非対応で API キー必須（[公式](https://developers.google.com/maps/documentation/streetview/metadata)で確認済み）。metadata リクエストは無料。 |
| 検索半径 | **`radius=10`（m）＋ `source=outdoor`** でリクエスト。既定 50m は緩すぎるため 10m に絞り、`OK` = 10m 以内にパノラマあり と直接判定。 |
| 保存先 | **専用テーブル `google_streetview_coverage`（`osm_node_id` 主キー）**。理由は後述（Baidu 模倣ではなく `/deploy-data` の追記モデル適合＋id churn 耐性）。 |
| 照会の実行場所 | **ローカル DB に対して実行**（Baidu の `import_baidu_panoid` と同じ）。本番反映は `/deploy-data`。**Cloud Run / cron / Secret Manager は使わない**。 |
| API キーの保管 | **API 有効化・キー発行は Terraform**（`google_project_service` ＋ `google_apikeys_key`、`y-junctions-prod`）。キー値は `terraform output` からローカル `backend/.env` の `GOOGLE_MAPS_API_KEY` へ貼るだけ。サーバーに載せないので Secret Manager/Cloud Run への配線は不要。 |
| 未照会ノードの扱い | 除外せず**表示**。カバレッジ有無を確定できたノードのうち「無し」だけを消す。 |
| tombstone（無し）の再照会 | Google のカバレッジは増えるため固定しない。enrich コマンドの `--refresh` で `has_coverage=false` を再照会。**ただし本番への伝播は差分 `IMPORT INTO` 不可**（既存 `osm_node_id` が UNIQUE 制約で衝突。`deploy-data.md:12-15`）。`false→true` の訂正を本番へ反映するには、Baidu と同じく**手動 TRUNCATE＋全件 IMPORT INTO** による全件同期が必要。`--refresh` はまずローカル DB の更新まで。 |
| フロントエンド | 変更不要。backend が無しノードを除外するため、届く feature は全て有効。 |

## データモデル

```sql
CREATE TABLE google_streetview_coverage (
    osm_node_id  BIGINT PRIMARY KEY,
    has_coverage BOOLEAN NOT NULL,      -- 照会結果（true=あり / false=無し）
    queried_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- **3 状態**：行あり＋`has_coverage=true`＝あり／行あり＋`false`＝無し（除外対象）／**行なし＝未照会**。
- パノラマ座標も `pano_id` も持たない。`build_url` が Google 側の実データを消費しないため、保存しても誰も読まない死んだ列になる。将来 `&pano=<id>` で精密リンク化したくなったら列追加すればよい（今は YAGNI）。
- `osm_node_id` を主キーにするのは、id 再採番でキャッシュが孤立する問題を避けるため（Baidu が migration `009` で `id` 列方式から別テーブルへ移行した理由と同じ）。
- migration（`backend/migrations/`）は `deploy.yml` により serving DB と pipeline DB の**両方**に適用される（`backend-migrate` / `pipeline-staging-migrate`）。本テーブルは pipeline DB では使わないが `CREATE TABLE` は無害な no-op。

### なぜ y_junctions への列追加ではなく別テーブルか

| | 案A: `y_junctions.has_streetview` 列 | 案B: 別テーブル（採用） |
| --- | --- | --- |
| 読み取り | Junction 行に同乗、追加クエリ不要 | enricher で lookup 1 回 |
| **本番反映（`/deploy-data`）** | ✗ 既存本番行を **UPDATE** 要。`/deploy-data` は `IMPORT INTO`（追記）モデルで UPDATE 非対応 | ✓ 別テーブルへ `IMPORT INTO` するだけ。`baidu_panoramas` で実証済みの経路にそのまま乗る |
| id churn 耐性 | y_junctions の id 次第 | `osm_node_id` 主キーで安全 |

決め手は **`/deploy-data` の追記モデルに適合すること**。既にデプロイ済みの海外ノードのカバレッジを後から本番へ入れるのは、案A だと既存行の UPDATE が必要で `/deploy-data` の仕組みに乗らない。案B なら対象 `osm_node_id` を別テーブルへ IMPORT するだけで済む。

## コンポーネント

### 1. カバレッジ照会モジュール — `backend/src/importer/google.rs`（新規）

- Street View Static **metadata** エンドポイントを叩く（画像は取らない＝無料）。リクエストは `location`・`key`・**`radius=10`**・**`source=outdoor`**。
  - `radius=10`：既定 50m では「50m 先にあるだけ」で `OK` になり使えない。10m に絞れば `OK` = 10m 以内、と直接判定。
  - `source=outdoor`：屋内コレクションを除外。
- `status` で分岐：`"OK"` → カバレッジあり／`"ZERO_RESULTS"`・`"NOT_FOUND"` → 無し／`"OVER_QUERY_LIMIT"`・5xx → リトライ・バックオフ／**`"REQUEST_DENIED"`・`"INVALID_REQUEST"` → hard stop でバッチ全体を中断**（キー不正・権限不足を「無し」と誤記録しないため。`has_coverage=false` を大量に書く事故を防ぐ）。
- レート制御・リトライ・ペーシングは `backend/src/importer/baidu.rs` の機構を踏襲。API キーは env `GOOGLE_MAPS_API_KEY` から読む。未設定なら明示エラーで停止（サイレントに全ノード「無し」判定しない）。

### 2. 照会処理 — `backend/src/importer/mod.rs`（変更）

`import_baidu_panoid_data`（`mod.rs:212`）に倣う `import_google_coverage_data(pool, refresh)` を追加。`refresh=false` は未照会ノードのみ、`true` は `has_coverage=false` も対象。

**非中国の判定は Rust 側で行う**（SQL では不可）。地域判定は `china::is_in_china_mainland(lng, lat)`（`backend/src/domain/china.rs:18`、手書き bbox 群の Rust 関数）だけが根拠で、`y_junctions` に地域列は無い。よって Baidu と同じ構造にする（`mod.rs:224-227`）：リポジトリは uncovered 候補を**全件**返し、この関数内で `!china::is_in_china_mainland(j.lon, j.lat)` でフィルタしてから照会する。

照会 → `google_streetview_coverage` に upsert。照会失敗はそのノードを未照会のまま残す（`mod.rs:249` の Baidu と同じ abort-on-error）。

### 3. リポジトリ — `backend/src/db/google_repository.rs`（新規）

- `find_coverage_by_osm_node_ids(pool, ids) -> HashMap<i64, bool>`：`osm_node_id → has_coverage`。キー無し＝未照会。enricher が使う。
- `find_uncovered_nodes(pool, refresh) -> Vec<...>`：未照会（`refresh` 時は `has_coverage=false` も含む）の junction を**全件**返す（`find_without_baidu_panoid` に対応）。**非中国フィルタはここでは行わない**——地域判定は Rust の `china::is_in_china_mainland` のみで、SQL に持ち込めないため。呼び出し側（Component 2）が Rust で絞る。
- `upsert_coverage(pool, rows)`：`osm_node_id` で upsert（`ON CONFLICT ... DO UPDATE`。再照会で false→true に更新され得るため上書き）。

### 4. ローカル enrich コマンド — `backend/src/bin/import_google_streetview.rs`（新規）

`backend/src/bin/import_baidu_panoid.rs`（`--refresh` を取り `import_baidu_panoid_data(&pool, args.refresh)` を呼ぶ薄い CLI）に倣う。`import_google_coverage_data` を呼ぶだけ。ローカル DB（`DATABASE_URL`）に対して実行する。
- 初回 backfill：`cargo run --bin import_google_streetview`
- 再照会：`cargo run --bin import_google_streetview -- --refresh`

### 5. 除外ロジック — `backend/src/api/streetview_enricher.rs`（変更）

現状は China のみ分岐（`enrich_collection` line 22、`build_url` line 55）。「地域ポリシーの唯一の集約点」という設計を維持したまま拡張。

- `enrich_collection`：非中国ノードについて `find_coverage_by_osm_node_ids` を引く。除外条件は「**非中国 かつ `has_coverage=false`**」のみ。covered と未照会（キー無し）は残す。
- `enrich_feature`（単一ノード直リンク）：feature は返すが、**`has_coverage=false` のノードは URL を空にする**。China tombstone は `build_url` が `""` を返し frontend がボタンを抑止できる（`JunctionPopup.tsx:49` の `{streetview_url && ...}`）のに対し、Google で素通しすると壊れた非空 URL が出るため。
- `build_url`：非中国で covered/未照会なら従来どおり `junction.streetview_url()`、`has_coverage=false` なら `""`。そのため `build_url` に非中国のカバレッジ真偽を渡せるよう引数を拡張する。

### 6. 本番反映 — `/deploy-data` skill（変更）

`baidu_panoramas` と同じ扱いを `google_streetview_coverage` に追加する（`.claude/commands/deploy-data.md`）。
- export：bbox 内 junction に紐付く行を `google_streetview_coverage g JOIN y_junctions y ON y.osm_node_id = g.osm_node_id` で CSV 出力（`baidu_panoramas` の export ブロック `deploy-data.md:74-85` に倣う）。
- upload → `IMPORT INTO google_streetview_coverage (...) CSV DATA (...)`（`deploy-data.md:124` に倣う）。
- cleanup：userfile 削除。

**制約（重要）**：この差分 `IMPORT INTO` は**新規 `osm_node_id` の追記専用**。既存本番行の `has_coverage` を更新（`--refresh` の false→true 訂正など）することはできず、重複 bbox は UNIQUE 制約で失敗する（`deploy-data.md:12-15`）。初回 backfill は本番テーブルが空なので問題ないが、既デプロイ地域のカバレッジ訂正を本番へ反映するには手動 TRUNCATE＋全件 IMPORT が要る。この非対称は Baidu の `baidu_panoramas` と同じ。

## API キー発行（一度きり・Terraform）

GCP リソースはプロジェクトの IaC 規律に従い **Terraform で払い出す**（CLI では叩かない。state ドリフトを避ける）。キーを**ローカルで消費する**ことと、リソースを**どう払い出すか**は別軸——消費がローカルでも、API 有効化とキー発行は Terraform に置く。不要なのは Secret Manager / Cloud Run / cron の**配線**だけ。

```hcl
resource "google_project_service" "streetview" {
  service            = "street-view-image-backend.googleapis.com"
  disable_on_destroy = false
}

# キーを Terraform で払い出すには API Keys API 自体の有効化が前提
resource "google_project_service" "apikeys" {
  service            = "apikeys.googleapis.com"
  disable_on_destroy = false
}

resource "google_apikeys_key" "streetview_metadata" {
  name         = "streetview-metadata-local"
  display_name = "Street View Metadata (local enrich)"
  project      = var.project_id
  restrictions {
    api_targets { service = "street-view-image-backend.googleapis.com" }  # Street View のみに制限
  }
  depends_on = [google_project_service.streetview, google_project_service.apikeys]
}

output "streetview_api_key" {   # ローカル .env に貼るためだけの sensitive output
  value     = google_apikeys_key.streetview_metadata.key_string
  sensitive = true
}
```

取得〜配置：`terraform apply` → `terraform output -raw streetview_api_key` の値を `backend/.env` の `GOOGLE_MAPS_API_KEY` へ。`google_apikeys_key` は hashicorp/google の **GA リソース**（`google-beta` 不要）。

### キー漏洩時の課金リスクと対策

API キーの制限は**サービス単位**（`api_targets.service`）で、Street View Static API には無料の metadata と**課金対象の画像取得**の両エンドポイントが含まれる。よって「Street View に制限」しても、漏洩時は画像取得で課金され得る。対策を検証した結果：

- **IP 制限（`server_key_restrictions.allowed_ips`）**：enrich はローカルの動的 IP で走るため固定できず不可。
- **メソッド制限（`api_targets.methods`）で metadata 限定**：Terraform は `methods` を持つが、Street View の metadata は RPC メソッドではなく URL パスの違いで、絞れるメソッド識別子が公式に存在せず不可（検証済み）。
- **課金メトリックの日次上限を 0（採用・Terraform）**：Service Usage API で実メトリックを確認したところ、当プロジェクトの Street View には課金2メトリック（`street-view-image-backend.googleapis.com/billable_default`＝署名付き画像、`.../billable_unsignedbucket`＝未署名画像）と、**別建ての無料メトリック `.../street_view_metadata`** が存在する。enrich は metadata しか叩かないため、`google_service_usage_consumer_quota_override`（google-beta）で**課金2メトリックの日次上限（`/d/project`）を `0` に**する。metadata は無影響のまま、漏洩時の画像課金は構造的に発生し得ない（＝緩和ではなく無害化）。
- **ローテーション**：キー値は `google_apikeys_key.key_string` として TFC state と output に保存される（workspace 閲覧権限者が読める）。workspace の state アクセスを絞ったうえで、漏洩・退職時等は `google_apikeys_key` を作り直して `.env` を差し替える運用とする。

上記はすべて Terraform（`terraform/streetview.tf`）で管理し、Console 手操作はしない。google-beta provider を `versions.tf` に追加する。

## PR 分割

| # | 内容 | 種別 |
| --- | --- | --- |
| PR-1 | `google_streetview_coverage` テーブル追加（DDL のみ） | schema |
| PR-2 | `google.rs`＋リポジトリ＋`import_google_coverage_data`＋ローカル enrich bin | app |
| PR-3 | `/deploy-data` に google テーブルの export/import を追加 | ops/skill |
| ops | ローカルで enrich 実行 → `/deploy-data` で本番反映 | 手動 |
| PR-4 | enricher の除外ロジック有効化（本番にデータが揃ってから） | app |

**適用順**：PR-1 → PR-2 → PR-3 → ops → PR-4。除外を最後に有効化し、データが揃う前に海外ノードが消える事故を防ぐ。API 有効化・キー発行は **Terraform の小 PR（`google_project_service`＋`google_apikeys_key`＋output）＋ `terraform apply`** として、PR-2 の動作確認前に一度実施。

## エラーハンドリング・非機能

- **クォータ／レート**：metadata は無料だが QPS 制限あり。`baidu.rs` のペーシング／リトライを踏襲。`OVER_QUERY_LIMIT` はバックオフ。
- **API キー欠如**：env 未設定なら enrich を明示エラーで停止。
- **API 障害時**：照会失敗は「無し」にせず未照会のまま（次回再試行で回復）。`has_coverage=false` を書くのは「照会に成功し無しと確定」した場合のみ。

## テスト

- `google.rs`：`status` 別（`OK`/`ZERO_RESULTS`/`OVER_QUERY_LIMIT`）のパース、リトライ・バックオフ。
- `google_repository.rs`：DB 統合テスト（upsert、`find_coverage` の 3 状態判別、`find_uncovered_nodes` の対象抽出、refresh での false→true 上書き）。ローカル CockroachDB 必須（スキップ禁止）。
- `streetview_enricher.rs`：collection = 非中国 `has_coverage=false` は除外・未照会/covered は残す、feature = `false` は空 URL・covered は Google URL、の分岐テストを既存テスト（line 93-118）に追加。

## 変更しないもの

- フロントエンド（`MapView.tsx`, `JunctionPopup.tsx`, 型定義）。
- Baidu 関連の既存実装。
- Terraform の Secret Manager / Cloud Run / cron 配線（ローカル照会のため不要）。※ API 有効化・キー発行のみ Terraform に最小追加する（上記「API キー発行」節）。
