# GitHub Actions 自動デプロイ - 実装タスクリスト

## 概要

mainブランチへのpush時に、バックエンド・フロントエンドを自動的にプロダクション環境へデプロイする仕組みを構築します。

**戦略**: 段階的PR（安全・推奨）
- Phase 1 (PR1): インフラ準備（Terraform）
- Phase 2 (PR2): デプロイ自動化（GitHub Actions）

---

## Phase 1 (PR1): インフラ準備（Terraform管理）

**ゴール**: Workload Identity FederationとIAMリソースをTerraformで管理

**1 Phase = 1 PR**: このPhaseの全タスクを完了させてからPR作成

**背景**:
- 手動のgcloudコマンドではなく、Infrastructure as Codeで管理
- 再現性が高く、他の環境（staging等）にも展開可能
- バージョン管理でIAM設定の変更履歴を追跡

**成果物**:
- `terraform/github-actions.tf` - Workload Identity、Service Account、IAM管理
- `terraform/variables.tf` - `github_repository` 変数追加
- `terraform/outputs.tf` - 認証情報の出力追加
- `terraform/terraform.tfvars.example` - 例を更新
- `doc/github-actions-deployment.md` - セットアップガイド（✅ 更新済み）

### タスク

#### Terraformファイル作成

- [x] `terraform/github-actions.tf` 作成
  - [x] Workload Identity Pool リソース定義
    ```hcl
    resource "google_iam_workload_identity_pool" "github_actions"
    ```
  - [x] OIDC Provider（GitHub）リソース定義
    ```hcl
    resource "google_iam_workload_identity_pool_provider" "github"
    ```
  - [x] Service Account リソース定義
    ```hcl
    resource "google_service_account" "github_actions_deployer"
    ```
  - [x] IAM Roles付与（for_each使用）
    ```hcl
    resource "google_project_iam_member" "github_actions_roles"
    ```
    - `roles/run.admin`
    - `roles/cloudbuild.builds.editor`
    - `roles/storage.objectAdmin`
    - `roles/iam.serviceAccountUser`
  - [x] Workload Identity Binding定義
    ```hcl
    resource "google_service_account_iam_member" "workload_identity_binding"
    ```

- [x] `terraform/variables.tf` に変数追加
  ```hcl
  variable "github_repository" {
    description = "GitHub repository in the format 'owner/repo'"
    type        = string
  }
  ```

- [x] `terraform/outputs.tf` に出力追加
  ```hcl
  output "workload_identity_provider" {
    description = "Workload Identity Provider name for GitHub Actions"
    value       = google_iam_workload_identity_pool_provider.github.name
  }

  output "service_account_email" {
    description = "Service account email for GitHub Actions"
    value       = google_service_account.github_actions_deployer.email
  }
  ```

- [x] `terraform/terraform.tfvars.example` 更新
  ```hcl
  github_repository = "your-username/y-junctions"
  ```

#### 動作確認

- [x] ローカルで `terraform plan` 実行
  ```bash
  cd terraform
  mise exec -- terraform plan
  ```
- [x] エラーがないことを確認（✅ 20リソースが計画される）

#### PR作成

- [x] ブランチ作成: `feat/github-actions-infrastructure`
- [x] PR作成: "feat: Add GitHub Actions infrastructure with Terraform" (PR #22)
- [ ] レビュー依頼
- [ ] レビュー対応
- [ ] PRマージ

#### デプロイ

- [ ] PRマージ後、ローカルで `terraform apply` 実行
  ```bash
  cd terraform
  mise exec -- terraform apply
  ```
- [ ] リソース作成を確認
  ```bash
  # Workload Identity Provider確認
  terraform output -raw workload_identity_provider

  # Service Account確認
  terraform output -raw service_account_email
  ```

### 完了条件

- [x] `terraform plan` がエラーなく実行できる（✅ 20リソース計画）
- [ ] PRがマージされる
- [ ] `terraform apply` が成功する
- [ ] `terraform output -raw workload_identity_provider` で値が取得できる
- [ ] `terraform output -raw service_account_email` で値が取得できる
- [ ] GCPコンソールでリソースが確認できる
  - Workload Identity Pool
  - OIDC Provider
  - Service Account
  - IAM Bindings

### 注意事項

- このPRは**コード変更のみ**（terraform apply は手動実行）
- マージ後にローカルで `terraform apply` を実行してリソースを作成
- GitHub Secretsの設定は次のPhaseで実施

---

## Phase 2 (PR2): デプロイ自動化（GitHub Actions）

**ゴール**: mainブランチへのpush時に自動デプロイを実行

**1 Phase = 1 PR**: このPhaseの全タスクを完了させてからPR作成

**背景**:
- Phase 1でIAMリソースが作成済み
- GitHub Secretsが設定済み（Phase 1完了後に手動設定）
- マイグレーションをsqlx-cliに統一

**成果物**:
- `.mise.toml` - `backend:migrate` タスク修正
- `.github/workflows/deploy.yml` - デプロイワークフロー（改善版）

### タスク

#### .mise.toml修正

- [ ] `backend:migrate` タスクをsqlx-cliに変更
  ```toml
  [tasks."backend:migrate"]
  description = "Run database migrations"
  run = """
  export DATABASE_URL=$(mise exec -- terraform output -raw neon_connection_uri)
  sqlx migrate run
  """
  dir = "backend"
  ```

#### GitHub Secretsの設定（手動作業）

- [ ] GitHubリポジトリの Settings > Secrets and variables > Actions を開く
- [ ] 以下のシークレットを追加:
  - [ ] `WORKLOAD_IDENTITY_PROVIDER`
    ```bash
    cd terraform
    terraform output -raw workload_identity_provider
    ```
  - [ ] `SERVICE_ACCOUNT`
    ```bash
    terraform output -raw service_account_email
    ```
  - [ ] `DATABASE_URL`
    ```bash
    terraform output -raw neon_connection_uri
    ```
  - [ ] `GCP_REGION` = `asia-northeast1`
  - [ ] `GCP_PROJECT_ID` = `y-junctions-prod`
  - [ ] `GCP_BUCKET_NAME` = `y-junctions-prod-frontend`
  - [ ] `BACKEND_URL` (Cloud Runデプロイ後に取得)

#### ワークフローファイル作成

- [ ] `.github/workflows/deploy.yml` 作成（改善版を使用）
  - [ ] backend-test ジョブ
    - Setup Rust
    - Cache Cargo
    - Run tests
    - Check formatting
    - Run Clippy
    - Security audit
  - [ ] frontend-test ジョブ
    - Setup Node.js
    - Install dependencies
    - Run tests
    - Type check
    - Lint
    - Format check
  - [ ] backend-deploy ジョブ
    - Checkout code
    - Authenticate to Google Cloud
    - Setup Cloud SDK
    - Setup Docker Buildx
    - Build and push Docker image
    - Setup Rust for migrations
    - Install SQLx CLI
    - Run database migrations
    - Deploy to Cloud Run
    - Verify deployment (health check)
    - Rollback on failure
    - Cleanup old revisions
  - [ ] frontend-deploy ジョブ
    - Checkout code
    - Setup Node.js
    - Install dependencies
    - Build
    - Verify no secrets in build
    - Authenticate to Google Cloud
    - Setup Cloud SDK
    - Deploy to Cloud Storage
    - Verify deployment
  - [ ] post-deploy ジョブ
    - Generate deployment summary
    - Check overall deployment status

#### PR作成

- [ ] ブランチ作成: `feat/github-actions-deployment`
- [ ] PR作成: "feat: Add GitHub Actions deployment automation"
- [ ] レビュー依頼
- [ ] レビュー対応
- [ ] PRマージ

#### デプロイ確認

- [ ] PRマージ後、GitHub Actionsが自動実行されることを確認
- [ ] GitHub Actions タブでワークフローの進行状況を確認
- [ ] 各ジョブが成功することを確認
  - [ ] backend-test
  - [ ] frontend-test
  - [ ] backend-deploy
  - [ ] frontend-deploy
  - [ ] post-deploy

### 完了条件

- [ ] `.mise.toml` がsqlx-cli版に更新されている
- [ ] `.github/workflows/deploy.yml` が作成されている
- [ ] GitHub Secretsがすべて設定されている
- [ ] PRがマージされる
- [ ] mainにマージ後、自動デプロイが成功する
- [ ] Cloud Runサービスが更新される
- [ ] フロントエンドがCloud Storageに配信される
- [ ] ヘルスチェックがパスする
- [ ] デプロイサマリーが生成される

### デプロイフロー

```
main へ push
    ↓
deploy.yml トリガー
    ↓
[並列実行]
├─ backend-test  (fmt, clippy, test)
└─ frontend-test (typecheck, lint, test)
    ↓ 両方成功
[並列実行]
├─ backend-deploy
│   1. マイグレーション実行 (失敗時は中断)
│   2. Dockerビルド
│   3. Cloud Runデプロイ
│   4. ヘルスチェック (失敗時は自動ロールバック)
│   5. 古いリビジョン削除
└─ frontend-deploy
    1. ビルド
    2. シークレットスキャン
    3. Cloud Storageアップロード
    ↓
post-deploy (サマリ生成)
```

### 重要な注意事項

- **GitHub Secrets設定は必須**: デプロイ前に必ず設定すること
- **初回デプロイの確認**: マージ後はGitHub Actionsのログを監視
- **失敗時の対応**: 自動ロールバックが動作するが、ログを確認して原因を特定
- **BACKEND_URL**: 初回デプロイ後に取得してGitHub Secretsに追加

### トラブルシューティング

| 問題 | 原因 | 対処法 |
|------|------|--------|
| Workload Identity認証失敗 | `github_repository` が間違っている | `terraform.tfvars` を確認 |
| マイグレーション失敗 | SQLエラー | ローカルで `sqlx migrate run` を実行して確認 |
| Dockerビルド失敗 | Dockerfileのエラー | ローカルで `docker build` を実行 |
| Cloud Runデプロイ失敗 | 権限不足 | Service Accountの権限を確認 |
| ヘルスチェック失敗 | `/health` エンドポイントの問題 | バックエンドのログを確認 |

---

## 成功指標

自動デプロイが正常に機能していることを示す指標：

- ✅ デプロイ時間: 手動15分 → 自動3-5分以内
- ✅ デプロイ成功率: 95%以上
- ✅ 人的エラー: ゼロ（手順ミスなし）
- ✅ ロールバック時間: 自動（数秒）または手動5分以内
- ✅ ヘルスチェック成功率: 100%

---

## 参考ドキュメント

- `doc/github-actions-deployment.md` - 詳細なセットアップガイド
- `.github/workflows/deploy.yml` - デプロイワークフロー（実装後）
- `DEPLOY.md` - 既存のデプロイ手順（手動版）

---

## 進捗管理

### Phase 1 (PR1): インフラ準備
- [x] 開始
- [x] 実装中（Terraformファイル作成完了、terraform plan 成功）
- [x] PR作成完了 (PR #22: https://github.com/cozy-corner/y-junctions/pull/22)
- [ ] レビュー中
- [ ] 完了

### Phase 2 (PR2): デプロイ自動化
- [ ] 開始
- [ ] 実装中
- [ ] レビュー中
- [ ] 完了
