# 設計: Cloud Run Jobs walking skeleton（issue #229）

親 issue: #220 / sub-issue: #229

## 課題の要約

issue #220 の 7 ジョブ構成（GCS 4 バケット + Cloud Run Jobs + Workflows + Eventarc）に進む前に、**最小縦串 1 本**（`download-osm` → `extract-3way` → `load-to-cockroach`）を Cloud Run Jobs で end-to-end に通し、以下 4 点を実測で潰す:

- (a) `osmpbf` クレートの GCS 対応（一時 DL で十分か / ストリーム化が必要か）
- (b) Cloud Run Jobs のメモリ消費（パイロット bbox での実測）
- (c) CockroachDB Cloud に staging cluster / database を作る方法
- (d) Artifact Registry 構成（既存 `y-junctions` リポジトリ共用 vs 別作り）

issue #229 の宣言通り **1 PR にまとめ、分割しない**。

## 現状

- 3-way 取込本体は parse / insert が既に分離済 — `backend/src/importer/mod.rs:15-35`
  - `parser::parse_pbf_three_way(input_path, bbox) -> Vec<JunctionForInsert>`（DB 非依存）
  - `inserter::insert_junctions(pool, junctions)`（PBF 非依存）
  - 新バイナリは両関数を**そのまま呼ぶだけ**で、既存ロジックは無変更で再利用可能
- `parser::parse_pbf_three_way` は `File::open(input_path)` でローカルファイル前提 — `backend/src/importer/parser.rs:48-50`
- 既存 `import` バイナリは `--input` / `--bbox` で動作 — `backend/src/bin/import.rs:5-16`
- Artifact Registry `y-junctions`（asia-northeast1） / Cloud Run・Cloud Build API 有効化済 — `terraform/backend.tf:1-25`
- CockroachDB cluster は **asia-southeast1**（GCP は asia-northeast1） — `terraform/cockroachdb.tf:5-15` → ジョブは cross-region で書く形になる
- `object_store` / `parquet` クレート未導入 — `backend/Cargo.toml:22-44`
- 本 worktree には `doc/todo.md` 不在（`doc/` 直下に Phase 単位 doc が並ぶ運用）

## 提案する変更

### A. 新規 Rust バイナリ 3 個（既存 `import.rs` 等は無変更）

| バイナリ | 入力 → 出力 | 内部呼出 |
|---|---|---|
| `backend/src/bin/pipeline_download_osm.rs` | Geofabrik URL → `gs://...-yj-raw/osm/<dataset>.osm.pbf` | reqwest streaming |
| `backend/src/bin/pipeline_extract_three_way.rs` | `gs://yj-raw/...` を tmpfs に DL → `gs://yj-extracted/three-way/<run_id>.parquet` + `gs://yj-serving/three-way/<run_id>.parquet`（同一ファイルをコピー） | `parser::parse_pbf_three_way` |
| `backend/src/bin/pipeline_load_to_cockroach.rs` | `gs://yj-serving/three-way/<run_id>.parquet` → CockroachDB | `inserter::insert_junctions` |

**Cargo 依存追加（`backend/Cargo.toml`）**:
- `object_store = { version = "0.x", features = ["gcp"] }`
- `parquet = "..."`, `parquet-derive = "..."`
- `bytes`, `tokio-util`

**Parquet I/O 用 DTO**:
- `JunctionForInsert` 自体は無変更
- I/O 用に別途 DTO を定義し、`bearings: [u32;3]` は `bearing_1/2/3` にフラット展開、`Option<f32>` は nullable column
- `#[derive(ParquetRecordWriter, ParquetRecordReader)]` を試し、動かない場合は手書き schema にフォールバック

**PBF の `Read` ブリッジ**: `osmpbf::ElementReader::new` が `Read` impl を要求するので、`object_store::get` で取得したオブジェクトを Cloud Run の writable FS（tmpfs）に書き出してから `File::open` で渡す。`file://` 入力時は `LocalFileSystem` 経由でパスを直接取得しコピーを避ける。

### B. Docker / Build

- `pipeline/Dockerfile` — マルチステージ、3 バイナリを単一イメージに同梱、entrypoint を引数で切替
- `pipeline/cloudbuild.yaml` — `asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions/pipeline:latest` に push（既存 Artifact Registry リポジトリ共用、`pipeline:` プレフィックス）

### C. Terraform `terraform/pipeline.tf`（既存ファイル無変更）

- **GCS バケット 3 個**: `y-junctions-prod-yj-raw` / `y-junctions-prod-yj-extracted` / `y-junctions-prod-yj-serving`
  - location `asia-northeast1`、uniform_bucket_level_access、90 日 lifecycle 削除
- **ジョブ専用 SA 3 個**: `sa-pipeline-download-osm` / `sa-pipeline-extract-3way` / `sa-pipeline-load-to-cockroach`
  - 各 SA は対応するバケットの read/write のみ、`load-to-cockroach` のみ DB シークレット参照可
- **`google_cloud_run_v2_job` 3 個**:
  - `download-osm`: cpu=1, memory=2Gi
  - `extract-3way`: cpu=2, memory=8Gi
  - `load-to-cockroach`: cpu=1, memory=2Gi
- **`google_workflows_workflow` 1 個**: YAML inline、3 ジョブ順次実行
- **`google_cloud_scheduler_job` 1 個**: monthly cron で定義するが初期は disable で開始（手動キックで end-to-end 確認）
- **`cockroach_database`**: `y_junctions_pipeline_smoke` を既存 cluster 内に追加。`load-to-cockroach` 起動時に既存 migrations を sqlx migrate で流す

### D. パイロット bbox

**shikoku-latest（~120MB）+ 香川県相当の小 bbox** で開始。日本全国・上海は本 PR 対象外。

### E. 実装順序（1 PR 内のコミット粒度）

1. Cargo 依存追加 + Parquet DTO
2. `pipeline_download_osm` 実装 + ローカル `file://` smoke
3. `pipeline_extract_three_way` 実装 + ローカル smoke
4. `pipeline_load_to_cockroach` 実装 + ローカル smoke（staging DB は事前手動作成）
5. Dockerfile + cloudbuild + イメージ push
6. `terraform/pipeline.tf` 追加 + apply
7. Scheduler 手動キックで end-to-end → 完了条件 3 項目を issue #229 にコメント

## 完了条件

- [ ] Cloud Scheduler 手動キック → Workflows → 3 ジョブ順次成功 → `y_junctions_pipeline_smoke` database に Y 字路レコードが入る
- [ ] パイロット bbox での所要時間・メモリ実測値を取得し issue #229 にコメント
- [ ] osmpbf GCS 対応の判断材料（一時 DL で十分か / ストリーム化が必要か）を記録

## 却下した選択肢

- **PR 分割** — issue #229 既に却下（Workflows YAML が宙ぶらりんになる工程が出るため、3 ジョブは 1 PR にまとめる）
- **`object_store` を薄いヘルパで代用** — issue #229 既に却下（後続ジョブで結局必要になるリファクタが発生）
- **PBF の GCS ストリーム化** — issue #229 既に却下（クレート改造 or 自前 `Read` 実装が要り、walking skeleton の趣旨から外れる。tmpfs 一時 DL で十分）
- **prep PR で抽出/ロード分離リファクタを先行** — issue #229 既に却下（`parser` / `inserter` は既に分離済み）
- **中間フォーマットに JSON Lines / Arrow IPC を採用** — issue #229 既に却下（Parquet が事実上の標準、DuckDB / BigQuery 直クエリも将来効く）
- **3 バイナリを別イメージ化** — Cloud Build 時間倍増・Terraform も冗長。1 イメージ + entrypoint 切替で十分
- **`yj-enriched` バケット先行作成** — 後続 sub-issue で要件が固まってから

## リスク / 未確認事項

- **cross-region**: Cluster は asia-southeast1、ジョブは asia-northeast1。レイテンシ・egress 課金は実測待ち。実測次第で staging cluster を asia-northeast1 に作る案を別 sub-issue 化
- **`object_store::gcp` の認証**: Cloud Run Jobs SA からの ADC 取得経路は未検証（ローカル smoke 時は `gcloud auth application-default login` が必要かも）
- **`parquet-derive`**: `i32` / `f32` / `bool` / `String` / `Option` の組合せで動くか未確認。失敗時は手書き schema へフォールバック
- **同一 cluster で staging DB を共存** → `request_unit_limit = 50000000`（`terraform/cockroachdb.tf:18`）を消費。月次ロード程度なら軽微見込みだが要監視
- **Cloud Run の writable filesystem は in-memory tmpfs**、書き込みはメモリ上限に算入される（[公式 docs](https://cloud.google.com/run/docs/container-contract)）。パイロット bbox の小さい PBF なら問題なし、日本全国（〜2GB）でも 32GB 上限内に収まる見込みだが要実測
- **CockroachDB Cloud Free tier に追加 database 作成のクォータ制限**があるか未確認
- **Workflows 月次 cron の課金**が無料枠内か未確認（issue #220 の見積もり通りか）
