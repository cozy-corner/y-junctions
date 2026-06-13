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

# 初回のみ: テスト DB を作成（データは named volume cockroachdata に永続化。
# down -v でボリュームを消した場合のみ再実行が必要）
docker exec y-junctions-cockroachdb ./cockroach sql --insecure \
  --execute "CREATE DATABASE IF NOT EXISTS y_junction_test;"
```

## 環境変数 (backend/.env)

```
DATABASE_URL=postgresql://root@localhost:26257/y_junction?sslmode=disable
TEST_DATABASE_URL=postgresql://root@localhost:26257/y_junction_test?sslmode=disable
```

worktree 運用では `scripts/setup-worktree.sh` が自動生成する。

## 詳細は README.md

データインポート手順・API 仕様・本番デプロイ手順等は README.md を参照。

## ブランチ命名規則

**prefix 選択の判断基準: 「そのリリースで、エンドユーザーがリリースノートで見るべき変更か」を Yes/No で答える。**
PR マージ時に release-drafter がブランチ名からラベルを自動付与する。基本の prefix→ラベル対応は README.md「ブランチ命名規則」を参照。

判断に迷う典型ケース（README に無い補足）:
- **大規模な新規内部実装**（新規バッチ・Cloud Run Job 等で規模は大きいがユーザーに見えない）→ `feature/*` で切って **`skip-changelog` ラベルを手動付与**。`chore/*`（軽量保守専用）には入れない
- **内部改善として始めたが結果的にユーザー価値を持った**ケース → `internal` ラベルを外し `feature` ラベルを手で貼る（release notes に出る・minor bump）

命名形式: `<prefix>/<issue#>-<kebab-desc>`（例: `chore/234-staging-db-migrations`）。

## Worktree 作成

```bash
git gtr new <branch>
```

`postCreate` hook で `npm install` / `cd frontend && npm install` / `mise trust` / `./scripts/setup-worktree.sh` が自動実行される。
`git worktree add` を直接叩かないこと（hook が走らず .env 等が未整備になる）。

## 本番データ操作は skill 経由

直接 SQL を本番 DB に流さないこと。以下の skill を使う：

- 本番 DB → ローカルに取り込む: `/sync-from-prod`
- ローカル DB → 本番に反映: `/deploy-data`
- 新規地域追加: `/add-region`

---

# テストスキップ禁止

統合テストは「ローカル DB が起動していない」を理由にスキップしてはならない。
DB が落ちているなら「ローカル環境起動」の手順 (`docker-compose up -d` 等) を自分で実行してから再度走らせる。手順自体が失敗した場合に限り質問すること。

---

# コマンド実行ルール

サーバーやフロントエンドを起動する際は、バックグラウンド（`&`）ではなくフォアグラウンドで実行すること。
Claude Code はツールごとに独立してコマンドを実行するため、フォアグラウンド実行でもブロックされない。
バックグラウンド起動はプロセスが残存し、ポート競合の原因になる。
