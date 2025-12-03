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
# URL取得
cd terraform
BACKEND_URL=$(mise exec -- terraform output -raw backend_url)

# ヘルスチェック
curl $BACKEND_URL/health
curl $BACKEND_URL/api/stats
```

### フロントエンド
```bash
# URL取得
cd terraform
FRONTEND_URL=$(mise exec -- terraform output -raw frontend_bucket_url)

# URLを表示
echo "ブラウザで開く: $FRONTEND_URL"
```

**注意**: キャッシュがある場合はハードリフレッシュ（Cmd+Shift+R）

## データインポート

### OSMデータのダウンロードとインポート
```bash
cd backend

# OSMデータダウンロード（関東地方）
curl -L -o kanto-latest.osm.pbf https://download.geofabrik.de/asia/japan/kanto-latest.osm.pbf

# 接続文字列取得
cd ../terraform
DB_URL=$(mise exec -- terraform output -raw neon_connection_uri)

# インポート実行
# --bbox: 境界ボックス (min_lon,min_lat,max_lon,max_lat)
cd ../backend
DATABASE_URL="$DB_URL" cargo run --bin import -- \
  --input kanto-latest.osm.pbf \
  --bbox "138.5,34.5,140.9,36.5"
```

**実行結果（2025-12-03）:**
- 処理したウェイ総数: 8,964,117
- Y字路件数: 34,165件
- データベースサイズ: 7.5 MB
- 処理時間: 約5分

## 次回デプロイ時に必要な作業

### フロントエンド配信改善【推奨】
現在：Cloud Storage直接配信（キャッシュ問題あり）
推奨：Firebase Hosting または Cloud Run（Nginx）に移行
