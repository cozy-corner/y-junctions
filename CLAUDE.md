# 【最優先】プロジェクトの基盤情報

## 技術スタック

- **Backend**: Rust + Axum + SQLx + **CockroachDB** (PostgreSQL ではない)
- **Frontend**: TypeScript + React + Leaflet

CockroachDB の含意:
- `pg_advisory_lock` 非対応 → sqlx migrate は `.set_locking(false)` で実行
- `RESTART IDENTITY` 非対応 → TRUNCATE 時に指定しない
- sqlx は `postgresql://` ドライバで動作するが、一部の PG 機能は使えない

## ローカル環境起動

```bash
# DB (コンテナ名: y-junctions-cockroachdb, port 26257)
docker-compose up -d

# 初回のみ: テスト DB を作成
# データは名前付きボリューム cockroachdata に永続化されるため、
# `docker-compose down` (ボリュームは残る) や docker restart 後の再作成は不要。
# `docker-compose down -v` 等でボリュームを削除した場合のみ再実行が必要。
docker exec y-junctions-cockroachdb ./cockroach sql --insecure \
  --execute "CREATE DATABASE IF NOT EXISTS y_junction_test;"
```

## 環境変数 (backend/.env)

```
DATABASE_URL=postgresql://root@localhost:26257/y_junction?sslmode=disable
TEST_DATABASE_URL=postgresql://root@localhost:26257/y_junction_test?sslmode=disable
```

worktree 運用では `scripts/setup-worktree.sh` が自動生成する。

## README.md

**作業開始前に必ず README.md を Read ツールで読むこと。**
上記の基盤情報は要約であり、README.md にはデータインポート手順、
API 仕様、本番デプロイ手順等の詳細が記載されている。

## ブランチ命名規則

PR マージ時に release-drafter がブランチ名から自動でラベル付けする：

- `data/*` — データ追加・更新 → `data` ラベル
- `feature/*` — 新機能 → `feature` ラベル
- `fix/*` / `bugfix/*` — バグ修正 → `bug` ラベル
- `refactor/*` / `chore/*` / `perf/*` / `style/*` / `docs/*` — 内部改善 → `internal` ラベル（リリースノート対象外）
- `dependabot/*` — 依存関係更新 → `internal` ラベル

issue 番号付きで `<prefix>/<issue#>-<kebab-desc>` 形式にする（例: `chore/234-staging-db-migrations`）。

## Worktree 作成

```bash
git gtr new <branch>
```

`postCreate` hook で `npm install` / `cd frontend && npm install` / `mise trust` / `./scripts/setup-worktree.sh` が自動実行される。
`git worktree add` を直接叩かないこと（hook が走らず .env 等が未整備になる）。

## Terraform 操作（新規 worktree）

```bash
# main worktree から terraform.tfvars をコピー
cp <main-worktree-path>/terraform/terraform.tfvars terraform/
cd terraform && terraform init
```

`terraform.tfvars` は機密情報を含み gitignore 対象のため、各 worktree で個別に準備が必要。

## 本番データ操作は skill 経由

直接 SQL を本番 DB に流さないこと。以下の skill を使う：

- 本番 DB → ローカルに取り込む: `/sync-from-prod`
- ローカル DB → 本番に反映: `/deploy-data`
- 新規地域追加: `/add-region`

---

# 【必須】コミット前のチェック

**コミット前に必ず以下のチェックを実行すること。チェックなしでのコミットは禁止。**

## Backend変更時

```bash
# 1. テスト実行（必須）
cargo test --manifest-path backend/Cargo.toml

# 2. フォーマットチェック（必須）
cargo fmt --manifest-path backend/Cargo.toml --check

# 3. Clippyチェック（必須）
cargo clippy --manifest-path backend/Cargo.toml -- -D warnings
```

## Frontend変更時

```bash
# 1. テスト実行（必須）
cd frontend && npm test

# 2. 型チェック（必須）
npm run typecheck

# 3. フォーマットチェック（必須）
npm run format:check

# 4. Lintチェック（必須）
npm run lint
```

## コミット前の確認チェックリスト

以下の全てにチェックが入っていることを確認してからコミットすること：

- [ ] 該当するテストを全て実行し、全て通過した
- [ ] フォーマットチェックを実行し、通過した
- [ ] Clippyチェックを実行し、通過した（Backend）
- [ ] 型チェックを実行し、通過した（Frontend）
- [ ] Lintチェックを実行し、通過した（Frontend）

## テストスキップ禁止

統合テストは「ローカル DB が起動していない」を理由にスキップしてはならない。
DB が落ちているなら、上記「ローカル環境起動」セクションの手順 (`docker-compose up -d` 等)
を自分で実行してから再度テストを走らせる。手順自体が失敗した場合に限り質問すること。

## CI失敗時の対応プロトコル

CI失敗を検出した場合、以下の手順を実行する：

1. ローカルで上記の全チェックを実行する
2. 失敗したチェックを修正する
3. 再度全チェックを実行し、全て通過することを確認する
4. 修正をコミットしてプッシュする

---

# コマンド実行ルール

サーバーやフロントエンドを起動する際は、バックグラウンド（`&`）ではなくフォアグラウンドで実行すること。
Claude Codeはツールごとに独立してコマンドを実行するため、フォアグラウンド実行でもブロックされない。
バックグラウンド起動はプロセスが残存し、ポート競合の原因になる。

---

# Phase開発ルール

`doc/todo.md`の各Phaseチェックリストに書かれていることだけを実装する。
それ以外は実装しない。疑問があれば質問する。

## PR作成前の必須作業

**`doc/todo.md`の更新を必ず行う：**
- 完了したPhaseのタスクチェックボックスをすべて `[x]` にする
- Phaseタイトルに ✅ を追加する
- 完了条件に実際の結果を記載する（✅ や ⚠️ を使用）
- 必要に応じて実装メモを追加する（重要な実装詳細や発見した課題）

