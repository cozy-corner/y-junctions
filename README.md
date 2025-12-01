# Y字路検索サービス

OpenStreetMapデータからY字路を検出・可視化するWebアプリケーション

## 技術スタック

- **Backend**: Rust + Axum + PostgreSQL/PostGIS + SQLx
- **Frontend**: TypeScript + React + Leaflet
- **Import**: Rust + osmpbf

## 環境構築

### 前提条件

- Docker & Docker Compose
- Rust (最新版)
- Node.js 18+
- PostgreSQL クライアント（psql）

### セットアップ手順

#### 1. リポジトリのクローン

```bash
git clone <repository-url>
cd y-junctions
```

#### 2. 環境変数の設定

```bash
# .envファイルを作成
cat > .env <<EOF
DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction
EOF
```

#### 3. データベースの起動

```bash
# PostgreSQL + PostGISコンテナを起動
docker-compose up -d

# データベースが起動するまで数秒待つ
sleep 5
```

#### 4. データベーススキーマの作成

```bash
# マイグレーションを実行
docker exec -i integration-db-1 psql -U y_junction -d y_junction < backend/migrations/001_create_y_junctions.sql
```

#### 5. データのインポート

OSM PBFファイルから Y字路データをインポートします。

```bash
# 四国全域のデータをインポート（約1分）
cargo run --manifest-path backend/Cargo.toml --bin import -- \
  --input /path/to/shikoku-latest.pbf \
  --bbox 132,33,135,35
```

**PBFファイルの入手方法:**
- [Geofabrik](https://download.geofabrik.de/) からダウンロード
- 例: 四国データ `https://download.geofabrik.de/asia/japan/shikoku-latest.osm.pbf`

**インポート結果の確認:**

```bash
# データ件数を確認
docker exec integration-db-1 psql -U y_junction -d y_junction -c "SELECT COUNT(*) FROM y_junctions;"
```

#### 6. バックエンドの起動

```bash
# 別のターミナルで実行
cd backend
cargo run --bin server
```

バックエンドは `http://localhost:8080` で起動します。

**APIエンドポイント:**
- `GET /api/junctions?bbox=132,33,135,35` - Y字路一覧取得
- `GET /api/junctions/:id` - 特定のY字路取得
- `GET /api/stats` - 統計情報取得

#### 7. フロントエンドの起動

```bash
# 別のターミナルで実行
cd frontend
npm install  # 初回のみ
npm run dev
```

フロントエンドは `http://localhost:5173` で起動します。

## 開発

### バックエンドのテスト

```bash
cd backend
cargo test
```

### フロントエンドのテスト

```bash
cd frontend
npm run typecheck
npm run lint
```

### データベースの接続

```bash
# psqlでデータベースに接続
docker exec -it integration-db-1 psql -U y_junction -d y_junction
```

### テーブル構造の確認

```sql
-- テーブル定義を表示
\d y_junctions

-- データのサンプル表示
SELECT id, osm_node_id, angle_1, angle_2, angle_3,
       ST_AsText(location) as location
FROM y_junctions
LIMIT 10;
```

## トラブルシューティング

### ポート5432が使用中

```bash
# 既存のPostgreSQLコンテナを停止
docker ps | grep postgres
docker stop <container-id>
```

### データベース接続エラー

```bash
# データベースコンテナの状態確認
docker ps
docker logs integration-db-1

# 環境変数の確認
cat .env
```

### インポートが失敗する

```bash
# .envファイルが存在するか確認
ls -la .env

# データベースが起動しているか確認
docker exec integration-db-1 psql -U y_junction -d y_junction -c "SELECT 1;"
```

## プロジェクト構成

```
.
├── backend/               # Rustバックエンド
│   ├── src/
│   │   ├── main.rs       # APIサーバー
│   │   ├── bin/
│   │   │   └── import.rs # データインポートツール
│   │   ├── api/          # APIハンドラー
│   │   ├── db/           # データベースリポジトリ
│   │   ├── domain/       # ドメインモデル
│   │   └── importer/     # PBFパーサー
│   ├── migrations/       # DBマイグレーション
│   └── Cargo.toml
├── frontend/             # Reactフロントエンド
│   ├── src/
│   │   ├── components/   # UIコンポーネント
│   │   ├── api/          # APIクライアント
│   │   └── hooks/        # カスタムフック
│   └── package.json
├── docker-compose.yml    # PostgreSQL設定
└── .env                  # 環境変数
```

## ライセンス

MIT
