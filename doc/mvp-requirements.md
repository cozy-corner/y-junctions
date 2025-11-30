# Y字路検索サービス MVP仕様書

## 概要

東京都内のY字路（三叉路）を地図上で検索・閲覧できるWebサービス。
角度などの条件でフィルタリングし、Y字路愛好家が効率的に目的のY字路を発見できる。

## 技術スタック

| レイヤー | 技術 |
|---------|------|
| Backend | Rust + Axum |
| Frontend | TypeScript + React + Vite |
| 地図 | Leaflet + OpenStreetMap タイル |
| DB | PostgreSQL + PostGIS |
| データソース | Geofabrik kanto-latest.osm.pbf |

## データソース

- URL: https://download.geofabrik.de/asia/japan/kanto-latest.osm.pbf
- 更新頻度: 日次（Geofabrik側）
- 初期構築: 手動実行、必要に応じて再実行

## データ定義

### Y字路の定義

- 3本の道路（highway タグ付き Way）が接続する Node
- 接続する道路は以下の highway タイプを対象とする:
  - primary, secondary, tertiary
  - residential, unclassified
  - living_street, pedestrian

### 角度計算

交差点 Node から各道路の次の Node へのベクトルを算出し、方位角（bearing）を計算。
3本の道路間の角度（3つ）を導出する。

```
angle_1: 最小の角度
angle_2: 中間の角度  
angle_3: 最大の角度
angle_1 + angle_2 + angle_3 = 360°
```

### 角度タイプ分類

| タイプ | 条件 | 説明 |
|--------|------|------|
| sharp | min_angle < 45° | 鋭角Y字 |
| even | 全角度が 100°〜140° | 均等Y字 |
| skewed | max_angle > 200° | 偏りY字 |

## DB設計

### テーブル: y_junctions

```sql
CREATE EXTENSION IF NOT EXISTS postgis;

CREATE TABLE y_junctions (
    id BIGSERIAL PRIMARY KEY,
    osm_node_id BIGINT UNIQUE NOT NULL,
    location GEOGRAPHY(POINT, 4326) NOT NULL,
    
    -- 角度（度数法、小さい順にソート済み）
    angle_1 SMALLINT NOT NULL CHECK (angle_1 BETWEEN 0 AND 180),
    angle_2 SMALLINT NOT NULL CHECK (angle_2 BETWEEN 0 AND 180),
    angle_3 SMALLINT NOT NULL CHECK (angle_3 BETWEEN 0 AND 360),
    
    -- 派生カラム
    min_angle SMALLINT GENERATED ALWAYS AS (angle_1) STORED,
    angle_type VARCHAR(10) GENERATED ALWAYS AS (
        CASE
            WHEN angle_1 < 45 THEN 'sharp'
            WHEN angle_1 >= 100 AND angle_3 <= 140 THEN 'even'
            WHEN angle_3 > 200 THEN 'skewed'
            ELSE 'normal'
        END
    ) STORED,
    
    -- メタデータ
    road_types TEXT[] NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- インデックス
CREATE INDEX idx_y_junctions_location ON y_junctions USING GIST (location);
CREATE INDEX idx_y_junctions_min_angle ON y_junctions (min_angle);
CREATE INDEX idx_y_junctions_angle_type ON y_junctions (angle_type);
```

## API仕様

### Base URL

```
http://localhost:8080/api
```

### エンドポイント

#### GET /junctions

Y字路一覧を取得する。

**Query Parameters**

| パラメータ | 型 | 必須 | 説明 |
|-----------|-----|------|------|
| bbox | string | Yes | バウンディングボックス `min_lon,min_lat,max_lon,max_lat` |
| angle_type | string | No | `sharp`, `even`, `skewed`, `normal` |
| min_angle_lt | int | No | 最小角度がこの値未満 |
| min_angle_gt | int | No | 最小角度がこの値より大きい |
| limit | int | No | 取得件数上限（デフォルト: 500, 最大: 1000） |

**Response**

```json
{
  "type": "FeatureCollection",
  "features": [
    {
      "type": "Feature",
      "geometry": {
        "type": "Point",
        "coordinates": [139.7671, 35.6812]
      },
      "properties": {
        "id": 12345,
        "osm_node_id": 987654321,
        "angles": [42, 138, 180],
        "angle_type": "sharp",
        "road_types": ["residential", "residential", "tertiary"],
        "streetview_url": "https://www.google.com/maps/@35.6812,139.7671,3a,75y,210h,90t"
      }
    }
  ],
  "total_count": 1234
}
```

#### GET /junctions/:id

特定のY字路詳細を取得する。

**Response**

GeoJSON Feature形式で返す（GET /junctions と一貫性を保つため）

```json
{
  "type": "Feature",
  "geometry": {
    "type": "Point",
    "coordinates": [139.7671, 35.6812]
  },
  "properties": {
    "id": 12345,
    "osm_node_id": 987654321,
    "angles": [42, 138, 180],
    "angle_type": "sharp",
    "road_types": ["residential", "residential", "tertiary"],
    "streetview_url": "https://www.google.com/maps/@35.6812,139.7671,3a,75y,210h,90t"
  }
}
```

#### GET /stats

統計情報を取得する。

**Response**

```json
{
  "total_count": 45678,
  "by_type": {
    "sharp": 5432,
    "even": 12345,
    "skewed": 8901,
    "normal": 19000
  }
}
```

## フロントエンド

### 画面構成

```
+------------------------------------------+
|  Header: Y字路マップ                      |
+------------------------------------------+
| Filter Panel    |                        |
| +-------------+ |                        |
| | 角度タイプ   | |       地図エリア        |
| | □ 鋭角      | |                        |
| | □ 均等      | |    [Y字路ポイント]      |
| | □ 偏り      | |                        |
| +-------------+ |                        |
| | 最小角度     | |                        |
| | [====●===]  | |                        |
| | 0°    180°  | |                        |
| +-------------+ |                        |
|                 |                        |
| 検索結果: N件   |                        |
+------------------------------------------+
```

### コンポーネント

| コンポーネント | 責務 |
|--------------|------|
| App | 全体レイアウト、状態管理 |
| MapView | Leaflet地図表示、マーカー描画 |
| FilterPanel | フィルタ条件入力 |
| JunctionPopup | Y字路詳細ポップアップ |
| StatsDisplay | 検索結果件数表示 |

### 状態管理

```typescript
interface AppState {
  // 地図状態
  bounds: LatLngBounds | null;
  zoom: number;
  
  // フィルタ条件
  filters: {
    angleTypes: ('sharp' | 'even' | 'skewed' | 'normal')[];
    minAngleLt: number | null;
    minAngleGt: number | null;
  };
  
  // データ
  junctions: Junction[];
  isLoading: boolean;
  totalCount: number;
}
```

### 地図操作

- 初期表示: 東京駅周辺 (139.7671, 35.6812), zoom 14
- 移動・ズーム時に bbox を更新し API 再取得
- デバウンス: 300ms

## バッチ処理

### 処理フロー

```
1. PBFファイル読み込み
   ↓
2. 1st pass: 全Wayをスキャン
   - highway タグ付きWayのNode IDを収集
   - 各NodeのWay接続数をカウント
   ↓
3. 接続数 == 3 のNodeを抽出（Y字路候補）
   ↓
4. 2nd pass: 対象NodeとWayの座標を取得
   ↓
5. 角度計算
   - 各Wayの次Nodeへの方位角を算出
   - 3方位角から角度を計算
   ↓
6. PostgreSQLへバルクインサート
```

### 実行方法

```bash
# データ取得
wget https://download.geofabrik.de/asia/japan/kanto-latest.osm.pbf -O data/kanto-latest.osm.pbf

# バッチ実行
cargo run --bin import -- --input data/kanto-latest.osm.pbf --bbox 138.9,35.5,140.0,35.9
```

### bbox（東京都）

```
min_lon: 138.9
min_lat: 35.5
max_lon: 140.0
max_lat: 35.9
```

## ディレクトリ構成

```
y-junction/
├── backend/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs          # API サーバー
│       ├── bin/
│       │   └── import.rs    # バッチ処理
│       ├── api/
│       │   ├── mod.rs
│       │   ├── handlers.rs
│       │   └── routes.rs
│       ├── db/
│       │   ├── mod.rs
│       │   └── repository.rs
│       ├── domain/
│       │   ├── mod.rs
│       │   └── junction.rs
│       ├── importer/
│       │   ├── mod.rs
│       │   ├── parser.rs    # PBF解析
│       │   └── calculator.rs # 角度計算
│       └── config.rs
├── frontend/
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── components/
│       │   ├── MapView.tsx
│       │   ├── FilterPanel.tsx
│       │   ├── JunctionPopup.tsx
│       │   └── StatsDisplay.tsx
│       ├── hooks/
│       │   └── useJunctions.ts
│       ├── api/
│       │   └── client.ts
│       └── types/
│           └── index.ts
├── data/
│   └── .gitkeep
├── docker-compose.yml
└── README.md
```

## 依存関係

### Backend (Cargo.toml)

```toml
[package]
name = "y-junction-backend"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "server"
path = "src/main.rs"

[[bin]]
name = "import"
path = "src/bin/import.rs"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["runtime-tokio", "postgres", "json"] }
osmpbf = "0.3"
geo = "0.28"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.5", features = ["cors"] }
tracing = "0.1"
tracing-subscriber = "0.3"
clap = { version = "4", features = ["derive"] }
dotenvy = "0.15"
```

### Frontend (package.json)

```json
{
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "leaflet": "^1.9.4",
    "react-leaflet": "^4.2.1"
  },
  "devDependencies": {
    "@types/leaflet": "^1.9.8",
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0",
    "@vitejs/plugin-react": "^4.2.0"
  }
}
```

## 環境構築

### docker-compose.yml

```yaml
version: '3.8'
services:
  db:
    image: postgis/postgis:16-3.4
    environment:
      POSTGRES_USER: y_junction
      POSTGRES_PASSWORD: y_junction
      POSTGRES_DB: y_junction
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data

volumes:
  pgdata:
```

### 環境変数 (.env)

```
DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction
```

## 今後の拡張候補（MVP対象外）

- ユーザー認証・投稿機能
- 写真アップロード
- お気に入り登録
- Street View埋め込み表示
- 全国対応
- モバイルアプリ
