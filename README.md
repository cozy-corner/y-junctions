# Y字路検索サービス

OpenStreetMapデータからY字路を検出・可視化するWebアプリケーション

## 技術スタック

- **Backend**: Rust + Axum + PostgreSQL/PostGIS + SQLx
- **Frontend**: TypeScript + React + Leaflet
- **Import**: Rust + osmpbf

## Y字路の分類システム

このシステムでは、Y字路を4つのタイプに分類します。分類は3つの分岐角度（angle_1, angle_2, angle_3）のうち、最小角度（angle_1）と最大角度（angle_3）に基づいて行われます。

### 分類基準

| タイプ | 条件 | 説明 | UIカラー |
|--------|------|------|----------|
| **VerySharp** | angle_1 < 30° | 非常に鋭角なY字路。視認性が低く注意が必要 | <span style="color: #0000cc">■</span> 濃い青 (#0000cc) |
| **Sharp** | 30° ≤ angle_1 < 45° | 鋭角なY字路。やや見通しが悪い | <span style="color: #3399ff">■</span> 明るい青 (#3399ff) |
| **Normal** | 45° ≤ angle_1 < 60° | 標準的なY字路。比較的見通しが良い | <span style="color: #88dd44">■</span> 緑 (#88dd44) |
| **Skewed** | angle_3 > 200° | 歪んだY字路。ほぼ一直線に近い形状 | <span style="color: #9900ff">■</span> 紫 (#9900ff) |

**注意:** Skewedタイプは他の条件より優先されます（angle_3 > 200°の場合、angle_1の値に関わらずSkewedと判定）。

### インポート時のフィルタリング

データインポート時、以下の条件でフィルタリングが行われます：

- **angle_1 ≥ 60°** の交差点は **T字路とみなして除外** されます
- これにより、実際のY字路（3方向がほぼ均等に分岐する交差点）のみがデータベースに保存されます

### 分類の目的

この分類システムにより、以下が可能になります：

- **視認性の評価**: 最小角度が小さいほど見通しが悪く、注意が必要な交差点
- **道路設計の分析**: Skewedタイプは特殊な形状を持ち、設計上の制約がある可能性
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

#### 2. Git Worktree Runner の設定

このプロジェクトでは [git-worktree-runner](https://github.com/coderabbitai/git-worktree-runner) を使用してworktreeを管理します。

```bash
# worktree作成時の自動セットアップを有効化
git gtr config add gtr.hook.postCreate "npm install"
git gtr config add gtr.hook.postCreate "cd frontend && npm install"
git gtr config add gtr.hook.postCreate "mise trust"

# 設定確認
git config --get-all gtr.hook.postCreate
```

この設定により、`git gtr new <branch>` で新しいworktreeを作成すると、自動的に以下が実行されます：
- 必要な依存関係（husky, lint-staged, フロントエンド開発ツール）のインストール
- mise設定ファイル（.mise.toml）の自動trust（worktree間のcd移動時のエラー回避）

#### 3. 環境変数の設定

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

##### GET /api/junctions - Y字路一覧取得

境界ボックス内のY字路を取得します。

**必須パラメータ:**
- `bbox` - 境界ボックス（形式: `min_lon,min_lat,max_lon,max_lat`）

**オプションパラメータ:**
- `angle_type` - 角度タイプでフィルタ（複数指定可: `verysharp`, `sharp`, `normal`, `skewed`）
- `min_angle_gt` - 最小角度の下限（例: `min_angle_gt=30` で angle_1 > 30°）
- `min_angle_lt` - 最小角度の上限（例: `min_angle_lt=45` で angle_1 < 45°）
- `limit` - 取得件数の上限（デフォルト: 1000）

**例:**
```bash
# 四国全域のY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35"

# VerySharpとSharpタイプのみ取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&angle_type=verysharp&angle_type=sharp"

# 最小角度が30°未満のY字路を取得
curl "http://localhost:8080/api/junctions?bbox=132,33,135,35&min_angle_lt=30"
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
    "normal": 567,
    "skewed": 88
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
