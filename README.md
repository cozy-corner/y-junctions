# Y字路検索サービス

OpenStreetMapデータからY字路を検出・可視化するWebアプリケーション

## 技術スタック

- **Backend**: Rust + Axum + PostgreSQL/PostGIS + SQLx
- **Frontend**: TypeScript + React + Leaflet
- **Import**: Rust + osmpbf

## Y字路の分類システム

このシステムでは、Y字路を3つのタイプに分類します。分類は3つの分岐角度（angle_1, angle_2, angle_3）のうち、最小角度（angle_1）に基づいて行われます。

### 分類基準

| タイプ | 条件 | 説明 | UIカラー |
|--------|------|------|----------|
| **VerySharp** | angle_1 < 30° | 非常に鋭角なY字路。視認性が低く注意が必要 | <span style="color: #8B5CF6">■</span> 紫 (#8B5CF6) |
| **Sharp** | 30° ≤ angle_1 < 45° | 鋭角なY字路。やや見通しが悪い | <span style="color: #3B82F6">■</span> 明るい青 (#3B82F6) |
| **Normal** | 45° ≤ angle_1 < 60° | 標準的なY字路。比較的見通しが良い | <span style="color: #F59E0B">■</span> 琥珀色 (#F59E0B) |

### インポート時のフィルタリング

データインポート時、以下の条件でフィルタリングが行われます：

- **angle_1 ≥ 60°** の交差点は **T字路とみなして除外** されます
- これにより、実際のY字路（3方向がほぼ均等に分岐する交差点）のみがデータベースに保存されます

### 分類の目的

この分類システムにより、以下が可能になります：

- **視認性の評価**: 最小角度が小さいほど見通しが悪く、注意が必要な交差点
- **データフィルタリング**: UIで特定のタイプのY字路のみを表示可能

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

#### 2. データベースの起動

```bash
# PostgreSQL + PostGISコンテナを起動
docker-compose up -d

# データベースが起動するまで数秒待つ
sleep 5
```

#### 3. 環境変数の設定（メインworktree用）

```bash
# backend/.envファイルを作成
cat > backend/.env <<EOF
DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction
TEST_DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction_test
EOF
```

**注意**: 追加worktreeでは`./scripts/setup-worktree.sh`が自動で.envを作成するため、この手順は不要です。

#### 4. データベーススキーマの作成

```bash
# テスト用DBを作成
docker exec y-junctions-db psql -U y_junction -c "CREATE DATABASE y_junction_test;"

# 開発用DBにマイグレーションを実行
(cd backend && sqlx migrate run)
```

#### 5. データのインポート

**データ配置構成:**

```
~/y-junctions-data/
├── osm/
│   └── shikoku-latest.osm.pbf
└── gsi/
    └── xml/
        ├── FG-GML-*.xml
        └── ...
```

**5-1. Y字路データのインポート**

```bash
# 四国全域のデータをインポート（約1分）
(cd backend && cargo run --bin import -- \
  --input ~/y-junctions-data/osm/shikoku-latest.osm.pbf \
  --bbox 132,33,135,35)
```

**PBFファイルの準備:**
- [Geofabrik](https://download.geofabrik.de/)からダウンロード
- 例: 四国データ `https://download.geofabrik.de/asia/japan/shikoku-latest.osm.pbf`
- `~/y-junctions-data/osm/` に配置

**5-2. 標高データの追加**

```bash
(cd backend && cargo run --bin import-elevation -- \
  --elevation-dir ~/y-junctions-data/gsi)
```

**標高データの準備:**
- [国土地理院 基盤地図情報](https://fgd.gsi.go.jp/download/menu.php)からダウンロード（DEM5A）
- ZIPを解凍し、XMLファイルを `~/y-junctions-data/gsi/xml/` に配置

**インポート結果の確認:**

```bash
# データ件数を確認
docker exec y-junctions-db psql -U y_junction -d y_junction -c "SELECT COUNT(*) FROM y_junctions;"
```

#### 6. バックエンドの起動

```bash
# backend/ディレクトリから実行
(cd backend && cargo run --bin server)
```

バックエンドは `http://localhost:8080` で起動します。

**APIエンドポイント:**

##### GET /api/junctions - Y字路一覧取得

境界ボックス内のY字路を取得します。

**必須パラメータ:**
- `bbox` - 境界ボックス（形式: `min_lon,min_lat,max_lon,max_lat`）

**オプションパラメータ:**
- `angle_type` - 角度タイプでフィルタ（複数指定可: `verysharp`, `sharp`, `normal`）
- `min_angle_gt` - 最小角度の下限（例: `min_angle_gt=30` で angle_1 > 30°）
- `min_angle_lt` - 最小角度の上限（例: `min_angle_lt=45` で angle_1 < 45°）
- `min_angle_elevation_diff` - 最小角高低差の下限（メートル、例: `2.0`）
- `max_angle_elevation_diff` - 最小角高低差の上限（メートル、例: `5.0`）
- `limit` - 取得件数の上限（デフォルト: 1000）

**例:**
```bash
# 四国全域のY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35"

# VerySharpとSharpタイプのみ取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&angle_type=verysharp&angle_type=sharp"

# 最小角度が30°未満のY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&min_angle_lt=30"

# 最小角高低差が2m以上のY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&min_angle_elevation_diff=2"

# 最小角高低差が2m〜5mのY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&min_angle_elevation_diff=2&max_angle_elevation_diff=5"
```

**レスポンス:**
```json
{
  "type": "FeatureCollection",
  "total_count": 1234,
  "features": [
    {
      "type": "Feature",
      "geometry": {
        "type": "Point",
        "coordinates": [133.5, 34.0]
      },
      "properties": {
        "id": 1,
        "osm_node_id": 123456789,
        "angles": [35, 145, 180],
        "angle_type": "sharp",
        "elevation": 245.5,
        "min_elevation_diff": 12.3,
        "max_elevation_diff": 18.7,
        "min_angle_elevation_diff": 15.2,
        "streetview_url": "https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=34.0,133.5"
      }
    }
  ]
}
```

##### GET /api/junctions/:id - 特定のY字路取得

ID指定でY字路の詳細を取得します。

**例:**
```bash
curl "http://localhost:8080/api/junctions/1"
```

##### GET /api/stats - 統計情報取得

データベース全体の統計情報を取得します。

**例:**
```bash
curl "http://localhost:8080/api/stats"
```

**レスポンス:**
```json
{
  "total_count": 1234,
  "by_type": {
    "verysharp": 123,
    "sharp": 456,
    "normal": 567
  }
}
```

#### 7. フロントエンドの起動

```bash
# 別のターミナルで実行
cd frontend
npm install  # 初回のみ
npm run dev
```

フロントエンドは `http://localhost:5173` で起動します。

### Git Worktree Runnerの設定（追加worktree用）

初回セットアップ完了後、以下の設定を行うことで、追加worktree作成時の手間を自動化できます。

```bash
# worktree作成時の自動セットアップを有効化
git gtr config add gtr.hook.postCreate "npm install"
git gtr config add gtr.hook.postCreate "cd frontend && npm install"
git gtr config add gtr.hook.postCreate "mise trust"
git gtr config add gtr.hook.postCreate "./scripts/setup-worktree.sh"

# 設定確認
git config --get-all gtr.hook.postCreate
```

この設定により、`git gtr new <branch>` で新しいworktreeを作成すると、自動的に以下が実行されます：
- 必要な依存関係のインストール
- mise設定ファイルの自動trust
- **データベース設定（backend/.env）の自動作成**（共有DBを使用、インポート不要）

## Worktree運用

**前提**: 上記の初回セットアップとGit Worktree Runner設定が完了していること。

### 新しいworktree作成

```bash
git gtr new feature/xxx
cd ../y-junctions-feature-xxx
(cd backend && cargo test)  # すぐテスト可能
```

### スキーマ変更時（稀）

```bash
# 専用DB作成
docker exec y-junctions-db psql -U y_junction -c \
  "CREATE DATABASE my_feature_db TEMPLATE y_junction;"

# .env書き換えとマイグレーション実行
echo "DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/my_feature_db" > backend/.env
(cd backend && sqlx migrate run)
```

## 開発

### バックエンドのテスト

```bash
(cd backend && cargo test)
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
docker exec -it y-junctions-db psql -U y_junction -d y_junction
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
docker logs y-junctions-db

# 環境変数の確認
cat backend/.env
```

### インポートが失敗する

```bash
# backend/.envファイルが存在するか確認
ls -la backend/.env

# データベースが起動しているか確認
docker exec y-junctions-db psql -U y_junction -d y_junction -c "SELECT 1;"
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
└── docker-compose.yml    # PostgreSQL設定
```

## 本番環境デプロイ

### インフラ管理
- Terraform（`terraform/`ディレクトリ）で管理
- Terraform Cloudでstate管理（`terraform login`が必要）
- mainブランチへのpush時、GitHub Actionsで自動デプロイ

### データインポート（本番環境）
```bash
# データベース接続文字列を取得
cd terraform
DB_URL=$(terraform output -raw neon_connection_uri)

# OSMデータをダウンロード（例：関東地方）
curl -L -o kanto-latest.osm.pbf https://download.geofabrik.de/asia/japan/kanto-latest.osm.pbf

# データインポート
(cd backend && DATABASE_URL="$DB_URL" cargo run --bin import -- \
  --input ../kanto-latest.osm.pbf \
  --bbox "138.5,34.5,140.9,36.5")
```

## ライセンス

MIT
