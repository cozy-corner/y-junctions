# GitHub Actions 自動デプロイ設定ガイド

## 概要

mainブランチへのpush時に、バックエンド・フロントエンドを自動的にプロダクション環境（GCP）へデプロイします。

## 自動化の目的

- **手動作業の削減**: `mise run` コマンドの実行が不要に
- **人的エラーの防止**: デプロイ手順の実行ミスをゼロに
- **デプロイ時間の短縮**: 15分 → 3-5分以内（並列実行により高速化）
- **信頼性の向上**: 自動ロールバック、ヘルスチェック、リソース管理

## 主な機能

✅ **並列デプロイ**: Backend/Frontendを同時実行してデプロイ時間を短縮
✅ **自動ロールバック**: マイグレーションやデプロイ失敗時に自動的に前のバージョンに戻す
✅ **ヘルスチェック**: デプロイ後に自動的にサービスの正常性を確認
✅ **リソース管理**: 古いCloud Runリビジョンを自動削除してコストを削減
✅ **セキュリティ**: ビルド成果物内のシークレット検出、脆弱性監査

## デプロイフロー

```
┌─────────────────────────┐
│  main へ push           │
└──────────┬──────────────┘
           │
           ▼
     ┌─────────────┐
     │  deploy.yml │
     └──────┬──────┘
           │
    ┌──────┴──────┐
    │             │
    ▼             ▼
┌─────────┐  ┌──────────┐
│ Backend │  │ Frontend │
│  Test   │  │  Test    │
└────┬────┘  └────┬─────┘
     │            │
     │ 並列実行   │
     │            │
     ▼            ▼
┌─────────┐  ┌──────────┐
│ Backend │  │ Frontend │
│ Deploy  │  │ Deploy   │
│         │  │          │
│ 1. 認証 │  │ 1. 認証  │
│ 2. 移行 │  │ 2. ビルド│
│ 3. ビルド│  │ 3. 同期 │
│ 4. 配置 │  │ 4. 検証 │
│ 5. 検証 │  │          │
└────┬────┘  └────┬─────┘
     │            │
     └──────┬─────┘
            ▼
    ┌───────────────┐
    │ Post-Deploy   │
    │ - サマリ生成  │
    │ - 状態確認    │
    └───────────────┘
```

## セットアップ手順

### 前提条件

- GCPプロジェクト: `y-junctions-prod`
- GitHubリポジトリへの管理者権限
- ローカルに `gcloud` CLI インストール済み

---

### Step 1: Workload Identity Federation 設定（Terraform管理）

Workload Identity Federationを**Terraformで管理**することで、再現性が高く、バージョン管理された状態でインフラを構築できます。

#### 1.1 Terraformファイルの作成

`terraform/github-actions.tf` を作成：

```hcl
# Workload Identity Pool
resource "google_iam_workload_identity_pool" "github_actions" {
  project                   = var.project_id
  workload_identity_pool_id = "github-actions"
  display_name              = "GitHub Actions Pool"
  description               = "Workload Identity Pool for GitHub Actions"
}

# OIDC Provider for GitHub
resource "google_iam_workload_identity_pool_provider" "github" {
  project                            = var.project_id
  workload_identity_pool_id          = google_iam_workload_identity_pool.github_actions.workload_identity_pool_id
  workload_identity_pool_provider_id = "github"
  display_name                       = "GitHub Provider"
  description                        = "OIDC provider for GitHub Actions"

  attribute_mapping = {
    "google.subject"       = "assertion.sub"
    "attribute.repository" = "assertion.repository"
    "attribute.actor"      = "assertion.actor"
  }

  oidc {
    issuer_uri = "https://token.actions.githubusercontent.com"
  }
}

# Service Account for GitHub Actions
resource "google_service_account" "github_actions_deployer" {
  project      = var.project_id
  account_id   = "github-actions-deployer"
  display_name = "GitHub Actions Deployer"
  description  = "Service account for GitHub Actions deployments"
}

# IAM Roles for Service Account
resource "google_project_iam_member" "github_actions_roles" {
  for_each = toset([
    "roles/run.admin",
    "roles/cloudbuild.builds.editor",
    "roles/storage.objectAdmin",
    "roles/iam.serviceAccountUser",
  ])

  project = var.project_id
  role    = each.key
  member  = "serviceAccount:${google_service_account.github_actions_deployer.email}"
}

# Workload Identity Binding
resource "google_service_account_iam_member" "workload_identity_binding" {
  service_account_id = google_service_account.github_actions_deployer.name
  role               = "roles/iam.workloadIdentityUser"
  member             = "principalSet://iam.googleapis.com/${google_iam_workload_identity_pool.github_actions.name}/attribute.repository/${var.github_repository}"
}
```

#### 1.2 変数の追加

`terraform/variables.tf` に以下を追加：

```hcl
variable "github_repository" {
  description = "GitHub repository in the format 'owner/repo'"
  type        = string
  # 例: "your-username/y-junctions"
}
```

#### 1.3 outputs.tf への追加

`terraform/outputs.tf` に以下を追加：

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

#### 1.4 terraform.tfvars への設定

`terraform/terraform.tfvars` に追加：

```hcl
github_repository = "your-username/y-junctions"  # 実際の値に置き換える
```

#### 1.5 Terraform apply

```bash
cd terraform
mise exec -- terraform init
mise exec -- terraform plan
mise exec -- terraform apply
```

**確認**:

```bash
# Workload Identity Provider名を取得
terraform output -raw workload_identity_provider

# Service Account emailを取得
terraform output -raw service_account_email
```

---

### Step 2: GitHub Secrets 設定

GitHubリポジトリの `Settings` > `Secrets and variables` > `Actions` で以下のシークレットを追加：

| Secret名 | 値の取得方法 | 説明 |
|---------|------------|------|
| `WORKLOAD_IDENTITY_PROVIDER` | `terraform output -raw workload_identity_provider` | Workload Identity認証 |
| `SERVICE_ACCOUNT` | `terraform output -raw service_account_email` | デプロイ用サービスアカウント |
| `DATABASE_URL` | `terraform output -raw neon_connection_uri` | Neon接続URI |
| `GCP_REGION` | `asia-northeast1` | GCPリージョン |
| `GCP_PROJECT_ID` | `y-junctions-prod` | GCPプロジェクトID |
| `GCP_BUCKET_NAME` | `y-junctions-prod-frontend` | フロントエンド用バケット名 |
| `BACKEND_URL` | Cloud Runデプロイ後に取得 | バックエンドAPIのURL（ビルド時に使用） |

#### シークレット値の一括取得

```bash
cd terraform

# Workload Identity Provider
echo "WORKLOAD_IDENTITY_PROVIDER:"
terraform output -raw workload_identity_provider

# Service Account
echo "SERVICE_ACCOUNT:"
terraform output -raw service_account_email

# Database URL
echo "DATABASE_URL:"
terraform output -raw neon_connection_uri

# Backend URL (初回デプロイ後)
echo "BACKEND_URL:"
terraform output -raw backend_url
```

---

### Step 3: 実装ファイルの作成・修正

#### 3.1 `.mise.toml` の修正

`backend:migrate` タスクをsqlx-cliに統一：

**変更前**:
```toml
[tasks."backend:migrate"]
description = "Run database migrations"
run = """
DB_URL=$(mise exec -- terraform output -raw neon_connection_uri)
psql "$DB_URL" -f ../backend/migrations/001_create_y_junctions.sql
"""
dir = "terraform"
```

**変更後**:
```toml
[tasks."backend:migrate"]
description = "Run database migrations"
run = """
export DATABASE_URL=$(mise exec -- terraform output -raw neon_connection_uri)
sqlx migrate run
"""
dir = "backend"
```

#### 3.2 `.github/workflows/deploy.yml` の作成

**改善版ワークフローファイルは既に作成済みです**: `.github/workflows/deploy.yml`

主な特徴：
- ✅ テストとデプロイの分離（並列実行）
- ✅ マイグレーション失敗時の自動ロールバック
- ✅ デプロイ後のヘルスチェック
- ✅ 古いリビジョンの自動クリーンアップ
- ✅ シークレットスキャン
- ✅ デプロイサマリーの自動生成

詳細は `.github/workflows/deploy.yml` を参照してください。

---

### Step 4: 動作確認

#### 4.1 テストデプロイ

1. 小さな変更（例: READMEの修正）をコミット
2. mainブランチにマージ
3. GitHub Actionsタブでデプロイの進行状況を確認

#### 4.2 確認項目

- [ ] GitHub Actionsがトリガーされたか
- [ ] Backend/Frontend テストが並列実行されたか
- [ ] Backendデプロイが成功したか（マイグレーション含む）
- [ ] Frontendデプロイが成功したか
- [ ] ヘルスチェックがパスしたか
- [ ] Cloud Runサービスが更新されたか
- [ ] フロントエンドが正しく表示されるか

#### 4.3 デプロイ後の確認

```bash
# Cloud Runのログ確認
gcloud logging read "resource.type=cloud_run_revision AND resource.labels.service_name=y-junctions-backend" \
  --limit 50 \
  --format json

# Cloud Runサービスの状態確認
gcloud run services describe y-junctions-backend --region asia-northeast1

# フロントエンドのURL確認
cd terraform
terraform output -raw frontend_bucket_url

# デプロイされたリビジョン一覧
gcloud run revisions list --service=y-junctions-backend --region=asia-northeast1
```

---

## 機能詳細

### 自動ロールバック

#### マイグレーション失敗時
マイグレーション実行前に現在のリビジョンを記録し、失敗時には自動的にロールバックします。

```yaml
# 現在のリビジョンを保存
CURRENT_REVISION=$(sqlx migrate info --database-url "$DATABASE_URL" | grep "applied" | tail -1)

# マイグレーション失敗時
sqlx migrate revert --target-version $CURRENT_REVISION
```

#### Cloud Runデプロイ失敗時
デプロイが失敗した場合、前のリビジョンに自動的にトラフィックを戻します。

```bash
# 前のリビジョンを取得して切り替え
gcloud run services update-traffic y-junctions-backend \
  --to-revisions=$PREVIOUS_REVISION=100
```

### ヘルスチェック

デプロイ後、`/health` エンドポイントに5回リトライしてサービスの正常性を確認します。

```bash
# 5回リトライ（10秒間隔）
for i in {1..5}; do
  curl -f "${SERVICE_URL}/health" && exit 0
  sleep 10
done
```

### リソースクリーンアップ

デプロイ成功後、古いCloud Runリビジョンを自動削除（最新5つを保持）してコストを削減します。

```bash
# 最新5つ以外を削除
gcloud run revisions list --service=y-junctions-backend \
  | tail -n +6 | xargs -I {} gcloud run revisions delete {}
```

### 並列デプロイ

Backend/Frontendデプロイはそれぞれのテスト完了後に独立して実行されるため、デプロイ時間が大幅に短縮されます。

```yaml
backend-deploy:
  needs: [backend-test]  # Backendテストのみに依存

frontend-deploy:
  needs: [frontend-test]  # Frontendテストのみに依存
```

---

## エラーハンドリング

### マイグレーション失敗時

**症状**: `sqlx migrate run` が失敗してデプロイが中断

**自動対応**:
1. 前のリビジョンへの自動ロールバックを試行
2. GitHub Actionsのステップサマリーにエラー詳細を出力

**手動対応**:
1. GitHub Actionsのログで詳細なエラーメッセージを確認
2. ローカルで同じマイグレーションを実行して問題を特定
   ```bash
   cd backend
   export DATABASE_URL="<Neon接続URL>"
   sqlx migrate run
   ```
3. マイグレーションファイルを修正して再度push

### Dockerビルド失敗時

**症状**: Docker イメージのビルドが失敗

**対処法**:
1. GitHub ActionsのBuild and push Docker imageステップのログを確認
2. ローカルでDockerビルドを試す
   ```bash
   cd backend
   docker build -t test-backend .
   ```
3. Dockerfileのエラーを修正
4. 再度mainにpush

### Cloud Run デプロイ失敗時

**症状**: `gcloud run deploy` が失敗

**自動対応**:
前のリビジョンへの自動ロールバックを実行

**手動確認**:
1. Cloud Runの権限を確認
2. イメージが正しくArtifact Registryにプッシュされているか確認
   ```bash
   gcloud artifacts docker images list asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions
   ```
3. Cloud Runコンソールでエラー詳細を確認

### ヘルスチェック失敗時

**症状**: デプロイは成功したがヘルスチェックが失敗

**対処法**:
1. デプロイされたサービスのログを確認
   ```bash
   gcloud logging read "resource.type=cloud_run_revision" --limit 50
   ```
2. `/health` エンドポイントが正しく実装されているか確認
3. 必要に応じて手動でロールバック

---

## ロールバック手順

### 自動ロールバック（推奨）

ワークフローに組み込み済みのため、デプロイ失敗時は自動的にロールバックされます。

### 手動ロールバック

#### Backendのロールバック

Cloud Runコンソールまたはコマンドラインから前のリビジョンに切り替え：

```bash
# 利用可能なリビジョンを確認
gcloud run revisions list --service=y-junctions-backend --region=asia-northeast1

# 特定のリビジョンにロールバック
gcloud run services update-traffic y-junctions-backend \
  --region=asia-northeast1 \
  --to-revisions=REVISION_NAME=100
```

または、コミットハッシュ付きイメージを再デプロイ：

```bash
# 前のコミットのイメージを使用
gcloud run deploy y-junctions-backend \
  --image asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions/backend:COMMIT_SHA \
  --region asia-northeast1
```

#### Frontendのロールバック

前のコミットをチェックアウトしてビルド・デプロイ：

```bash
git checkout PREVIOUS_COMMIT
cd frontend
npm ci
npm run build
gsutil -m rsync -r -d dist/ gs://y-junctions-prod-frontend
git checkout main
```

---

## 重要な注意事項

### マイグレーションの冪等性

sqlx-cliは `_sqlx_migrations` テーブルで実行済みマイグレーションを管理します。同じマイグレーションを複数回実行しても安全です。

### Dockerイメージのタグ戦略

2つのタグを付与：
- `latest`: 常に最新版
- `<commit-sha>`: コミットハッシュ（ロールバック用）

### 並列実行の仕組み

Backend/Frontendデプロイは互いに独立して実行されます。ただし、どちらも各自のテストが完了するまで開始されません。

### 既存CIワークフローとの関係

- `backend-ci.yml`: PRとmainへのpush時にテスト・チェック実行（維持）
- `frontend-ci.yml`: PRとmainへのpush時にテスト・チェック実行（維持）
- `deploy.yml`: mainへのpush時にテスト・デプロイ実行（新規）

デプロイワークフローは既存CIと並行して動作しますが、独自のテストステップを持つため、既存CIに依存しません。

---

## トラブルシューティング

### Workload Identity認証エラー

**エラー例**:
```
Error: google-github-actions/auth failed with: retry function failed after 3 attempts
```

**原因**:
- Workload Identity Bindingの設定ミス
- `github_repository` 変数の値が間違っている
- 権限不足

**対処法**:

1. **terraform.tfvarsの確認**:
```bash
cd terraform
cat terraform.tfvars | grep github_repository
# 出力: github_repository = "your-username/y-junctions"
```

GitHubのリポジトリURLと一致しているか確認。

2. **Workload Identity設定の確認**:
```bash
# Terraform出力を確認
terraform output workload_identity_provider
terraform output service_account_email

# GitHub Secretsと一致しているか確認
```

3. **再デプロイ**:
```bash
# terraform.tfvarsを修正後
terraform plan
terraform apply

# GitHub Secretsを更新
# WORKLOAD_IDENTITY_PROVIDER と SERVICE_ACCOUNT を新しい値に
```

### DATABASE_URLが見つからない

**エラー例**:
```
Error: DATABASE_URL environment variable is not set
```

**対処法**:
1. GitHub Secretsに `DATABASE_URL` が正しく設定されているか確認
2. Terraformから接続URLを再取得
   ```bash
   cd terraform
   terraform output -raw neon_connection_uri
   ```
3. GitHub Secretsを更新

### Docker Buildxキャッシュエラー

**エラー例**:
```
Error: failed to solve with frontend dockerfile.v0
```

**対処法**:
1. GitHub Actionsのキャッシュをクリア（Settings > Actions > Caches）
2. ワークフローを再実行

---

## パフォーマンス最適化

### キャッシュ戦略

- **Rustビルド**: `Swatinem/rust-cache@v2` でCargoキャッシュを自動管理
- **Node.js依存関係**: `actions/setup-node` のビルトインキャッシュを使用
- **SQLx CLI**: `~/.cargo/bin/sqlx` をキャッシュして再インストールを回避
- **Dockerレイヤー**: GitHub Actionsキャッシュを使用（`cache-from: type=gha`）

### 並列実行

Backend/Frontendのテスト・デプロイを並列実行することで、デプロイ時間を約50%短縮。

### リソースクリーンアップ

古いCloud Runリビジョンを自動削除することで、ストレージコストを削減。

---

## セキュリティ

### シークレット管理

- Workload Identity Federationを使用してキーレス認証
- GitHub Secretsで機密情報を暗号化保存
- ビルド成果物内のシークレットを自動検出

### 脆弱性監査

デプロイ前に `cargo audit` を実行して既知の脆弱性をチェック（オプション）。

---

## 成功指標

自動デプロイが正常に機能していることを示す指標：

- ✅ デプロイ時間: 手動15分 → 自動3-5分以内
- ✅ デプロイ成功率: 95%以上
- ✅ 人的エラー: ゼロ（手順ミスなし）
- ✅ ロールバック時間: 自動（数秒）または手動5分以内
- ✅ ヘルスチェック成功率: 100%

---

## 次のステップ（オプション）

### ステージング環境の追加

`.github/workflows/deploy-staging.yml` を作成して、`develop` ブランチへのpush時にステージング環境にデプロイ。

### モニタリング統合

Cloud Monitoringと連携して、デプロイ後のメトリクスを自動収集。

### E2Eテスト自動化

Playwrightなどを使用して、デプロイ後に自動的にE2Eテストを実行。

---

## 参考リンク

- [Workload Identity Federation](https://cloud.google.com/iam/docs/workload-identity-federation)
- [google-github-actions/auth](https://github.com/google-github-actions/auth)
- [Cloud Run Deployment](https://cloud.google.com/run/docs/deploying)
- [sqlx-cli](https://github.com/launchbadge/sqlx/tree/main/sqlx-cli)
- [Docker Build GitHub Actions](https://github.com/docker/build-push-action)
