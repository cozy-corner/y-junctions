# デプロイ手順

## 前提条件

### 必要なツール
- `gcloud` CLI
- `terraform` (via mise)
- `psql` (PostgreSQL Client via libpq)

### 必要な設定
`terraform/terraform.tfvars`を作成：
```hcl
project_id           = "y-junctions-prod"
region               = "asia-northeast1"
neon_api_key         = "your-neon-api-key"
backend_service_name = "y-junctions-api"
backend_image        = "asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions/backend:latest"
frontend_bucket_name = "y-junctions-prod-frontend"
```

## 初回デプロイ

### 1. インフラストラクチャ
```bash
cd terraform
mise exec -- terraform init
mise exec -- terraform apply
```

### 2. バックエンド
```bash
cd backend
gcloud builds submit --tag asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions/backend:latest .
```

### 3. データベースマイグレーション
```bash
# 接続文字列取得
cd terraform
DB_URL=$(mise exec -- terraform output -raw neon_connection_uri)

# マイグレーション実行
cd ../backend
/opt/homebrew/opt/libpq/bin/psql "$DB_URL" -f migrations/001_create_y_junctions.sql
```

### 4. フロントエンド
```bash
cd frontend
npm run build
gsutil -m rsync -r -d dist/ gs://y-junctions-prod-frontend
```

## 更新デプロイ

### バックエンド更新
```bash
cd backend
gcloud builds submit --tag asia-northeast1-docker.pkg.dev/y-junctions-prod/y-junctions/backend:latest .
# Cloud Runが自動的に新しいイメージをデプロイ
```

### フロントエンド更新
```bash
cd frontend
npm run build
gsutil -m rsync -r -d dist/ gs://y-junctions-prod-frontend
```

## デプロイ確認

### バックエンド
```bash
curl https://y-junctions-api-o32oa7fija-an.a.run.app/health
curl https://y-junctions-api-o32oa7fija-an.a.run.app/api/stats
```

### フロントエンド
ブラウザで開く：`https://storage.googleapis.com/y-junctions-prod-frontend/index.html`

**注意**: キャッシュがある場合はハードリフレッシュ（Cmd+Shift+R）

## 次回デプロイ時に必要な作業

### データインポート【未実施】
```bash
cd backend
# OSMデータダウンロード
wget https://download.geofabrik.de/asia/japan/kanto-latest.osm.pbf

# インポート実行
cargo run --bin import -- --input kanto-latest.osm.pbf --database-url "$DB_URL"
```

### フロントエンド配信改善【推奨】
現在：Cloud Storage直接配信（キャッシュ問題あり）
推奨：Firebase Hosting または Cloud Run（Nginx）に移行
