# 標高データ機能 開発タスクリスト

## 概要

Y字路の標高情報（elevation）と隣接ノード間の高低差を取得・保存する機能を追加します。

### 技術スタック

- **標高データソース**: GSI DEM5A (国土地理院 5mメッシュ標高データ)
  - 解像度: 5m
  - 垂直精度: ±0.3m以内（レーザー測量）
  - カバー範囲: 日本全国の約70%（主要都市部）
- **データ形式**: JPGIS (GML/XML) ファイル
- **Rustクレート**: `glob`, `roxmltree`

### アーキテクチャ方針

- **データ取得**: インポート時にGSI XMLファイルから標高を計算
- **データ保存**: PostgreSQLに計算済み標高値を保存（非正規化設計）
- **検索最適化**: 計算済みフィールド + インデックスで高速検索

### データ設計

```sql
-- y_junctions テーブルに追加するカラム
elevation REAL,                    -- ジャンクションノードの標高（メートル）
neighbor_elevation_1 REAL,         -- bearings[0]方向の隣接ノード標高
neighbor_elevation_2 REAL,         -- bearings[1]方向の隣接ノード標高
neighbor_elevation_3 REAL,         -- bearings[2]方向の隣接ノード標高
elevation_diff_1 REAL,             -- bearings[0]方向との高低差（絶対値）
elevation_diff_2 REAL,             -- bearings[1]方向との高低差（絶対値）
elevation_diff_3 REAL,             -- bearings[2]方向との高低差（絶対値）
min_angle_index SMALLINT,          -- 最小角のインデックス (1-3)
min_elevation_diff REAL,           -- 3つの高低差の最小値
max_elevation_diff REAL,           -- 3つの高低差の最大値
min_angle_elevation_diff REAL      -- 最小角を構成する2本の道路間の高低差
  GENERATED ALWAYS AS (
    CASE min_angle_index
      WHEN 1 THEN ABS(neighbor_elevation_1 - neighbor_elevation_2)
      WHEN 2 THEN ABS(neighbor_elevation_2 - neighbor_elevation_3)
      WHEN 3 THEN ABS(neighbor_elevation_3 - neighbor_elevation_1)
    END
  ) STORED;
```

---

## 🗄️ Phase 1: GSI標高データ基盤実装 ✅

**ゴール**: GSI JPGIS XMLファイルから標高を取得する基盤を実装

**方針変更**: SRTM → GSI（国土地理院）
- **理由**: SRTM (30-90m解像度) では都市部Y字路の標高差測定に不十分
- **採用**: GSI DEM5A (5m解像度) で高精度測定を実現

**成果物**:
- ✅ `backend/src/importer/elevation.rs` - 標高取得モジュール
- ✅ `backend/Cargo.toml` - glob, roxmltree クレート追加

**タスク**:
- [x] データソース選定（SRTM → GSI）
- [x] 依存関係追加（`glob = "0.3"`, `roxmltree = "0.20"`）
- [x] `ElevationProvider`構造体実装
  - [x] `new(data_dir: &str)` - XMLディレクトリパス指定
  - [x] `get_elevation(lat: f64, lon: f64)` - 緯度経度から標高取得
  - [x] XMLファイルのキャッシング機能（HashMap<PathBuf, GsiTile>）
  - [x] JPGIS XML パース処理
- [x] エラーハンドリング
  - [x] XMLファイル未存在の処理
  - [x] パースエラーの処理
- [x] ユニットテスト（5テスト実装）
  - [x] 標高取得の正常系テスト (富士山、東京駅)
  - [x] ファイル未存在時のテスト
  - [x] キャッシング動作のテスト
  - [x] 初期化テスト

**完了条件**:
- ✅ `cargo test` で elevation モジュールのテスト合格（5/5 passed）
- ✅ 富士山頂（35.3606, 138.7274）の標高が約3776m取得できる
- ✅ 東京駅（35.6812, 139.7671）の標高が約3m取得できる
- ✅ ZIP依存を削除、シンプルな実装

**実装**:
```rust
pub struct ElevationProvider {
    cache: HashMap<PathBuf, GsiTile>,  // XMLファイルパス → タイル
    data_dir: String,
}

impl ElevationProvider {
    pub fn new(data_dir: &str) -> Self { /* ... */ }

    pub fn get_elevation(&mut self, lat: f64, lon: f64) -> Result<Option<f64>> {
        // XMLファイルを列挙
        let pattern = format!("{}/xml/*.xml", self.data_dir);

        // 各XMLファイルをチェック（キャッシュ優先）
        // 座標を含むタイルから標高取得
    }
}
```

**データ配置**:
```
# リポジトリ外の任意の場所に配置（推奨）
~/y-junctions-data/
├── osm/
│   └── japan-latest.osm.pbf
└── gsi/
    └── xml/
        ├── FG-GML-5338-05-00-DEM5A-*.xml
        ├── FG-GML-5338-05-01-DEM5A-*.xml
        └── ... (解凍済みXMLファイル)
```

**注意**: リポジトリ内にデータを置かない（OSM PBFとGSI XMLで数GB～数十GBになるため）。

---

## 🔧 Phase 2: データモデル拡張

**ゴール**: 標高データを扱うためのデータ構造を拡張

**成果物**:
- `backend/src/importer/detector.rs` - JunctionForInsert構造体拡張
- `backend/src/domain/junction.rs` - Junction構造体拡張

**タスク**:
- [ ] `JunctionForInsert`構造体に標高フィールド追加
  ```rust
  pub struct JunctionForInsert {
      // 既存フィールド...
      pub elevation: Option<f64>,
      pub neighbor_elevations: Option<[f64; 3]>,
      pub elevation_diffs: Option<[f64; 3]>,
      pub min_angle_index: Option<i16>,
  }
  ```
- [ ] ヘルパーメソッド実装
  - [ ] `calculate_min_angle_index(angles: &[i16; 3]) -> i16`
  - [ ] `calculate_elevation_diffs(base: f64, neighbors: &[f64; 3]) -> [f64; 3]`
  - [ ] `calculate_min_max_diffs(diffs: &[f64; 3]) -> (f64, f64)`
- [ ] `Junction`構造体に標高フィールド追加
  ```rust
  pub struct Junction {
      // 既存フィールド...
      pub elevation: Option<f64>,
      pub min_elevation_diff: Option<f64>,
      pub max_elevation_diff: Option<f64>,
      pub min_angle_elevation_diff: Option<f64>,
  }
  ```
- [ ] ユニットテスト
  - [ ] 最小角インデックス計算のテスト
  - [ ] 高低差計算のテスト

**完了条件**:
- [ ] `cargo test` でドメインモデルのテスト合格
- [ ] 標高データがOptionalで扱える（XMLファイルがない場合もエラーにならない）

**工数**: 小（半日程度）

---

## 🔄 Phase 3: インポート処理統合

**ゴール**: OSMインポート時に標高データを取得・計算

**成果物**:
- `backend/src/importer/parser.rs` - parse_pbf関数修正
- `backend/src/importer/mod.rs` - elevationモジュール公開

**タスク**:
- [ ] `parse_pbf`関数にgsi_dir引数追加
  ```rust
  pub fn parse_pbf(
      input_path: &str,
      gsi_dir: Option<&str>,  // 追加
      min_lon: f64,
      min_lat: f64,
      max_lon: f64,
      max_lat: f64,
  ) -> Result<Vec<JunctionForInsert>>
  ```
- [ ] ElevationProviderの初期化
- [ ] 3rd passで標高取得処理追加
  - [ ] ジャンクションノードの標高取得
  - [ ] 3つの隣接ノードの標高取得
  - [ ] 高低差計算
  - [ ] 最小角インデックス計算
- [ ] ログ出力追加
  - [ ] 標高取得成功/失敗の統計
  - [ ] 例: "Elevation data retrieved: 1500/2000 (75%)"
- [ ] エラーハンドリング
  - [ ] XMLファイルがない場合は標高なしで続行
  - [ ] 一部のノードで標高が取得できない場合の処理

**完了条件**:
- [ ] `cargo run --bin import -- --input test.pbf --gsi-dir data/gsi --bbox ...` が成功
- [ ] 標高データが取得され、JunctionForInsertに格納される
- [ ] ログに標高取得の統計が表示される

**工数**: 中（1日程度）

**依存**: Phase 1, 2完了

**実装ポイント**:
```rust
// 3rd pass内での標高取得
let mut elevation_provider = gsi_dir.map(|dir| ElevationProvider::new(dir));

for junction in &y_junctions {
    // 既存の角度計算...

    // 標高取得
    let junction_elevation = elevation_provider
        .as_mut()
        .and_then(|p| p.get_elevation(junction.lat, junction.lon).ok().flatten());

    let neighbor_elevations = if let Some(provider) = elevation_provider.as_mut() {
        // 3つの隣接ノードの標高を取得
        Some([/* ... */])
    } else {
        None
    };

    // 高低差計算
    let elevation_diffs = /* ... */;
    let min_angle_index = Some(JunctionForInsert::calculate_min_angle_index(&angles));
}
```

---

## 🗄️ Phase 4: データベーススキーマ拡張

**ゴール**: 標高データを保存するためのDBスキーマ変更

**成果物**:
- `backend/migrations/003_add_elevation.sql` - マイグレーションSQL

**タスク**:
- [ ] マイグレーションSQL作成
  - [ ] 標高カラム追加（elevation, neighbor_elevation_1~3）
  - [ ] 高低差カラム追加（elevation_diff_1~3）
  - [ ] 最小角インデックス追加（min_angle_index）
  - [ ] 計算済みカラム追加（min_elevation_diff, max_elevation_diff）
  - [ ] Generated Column追加（min_angle_elevation_diff）
- [ ] インデックス作成
  - [ ] `CREATE INDEX idx_y_junctions_elevation ON y_junctions (elevation)`
  - [ ] `CREATE INDEX idx_y_junctions_min_elevation_diff ON y_junctions (min_elevation_diff)`
  - [ ] `CREATE INDEX idx_y_junctions_min_angle_elevation_diff ON y_junctions (min_angle_elevation_diff)`
- [ ] コメント追加（各カラムの説明）
- [ ] マイグレーション実行テスト

**完了条件**:
- [ ] `sqlx migrate run` でマイグレーション成功
- [ ] `\d y_junctions` で新しいカラムが表示される
- [ ] Generated Columnが正しく動作する

**工数**: 小（半日程度）

**依存**: Phase 3完了（実装確定後）

**マイグレーションSQL例**:
```sql
-- 003_add_elevation.sql

-- 標高データカラム追加
ALTER TABLE y_junctions
ADD COLUMN elevation REAL,
ADD COLUMN neighbor_elevation_1 REAL,
ADD COLUMN neighbor_elevation_2 REAL,
ADD COLUMN neighbor_elevation_3 REAL,
ADD COLUMN elevation_diff_1 REAL CHECK (elevation_diff_1 >= 0),
ADD COLUMN elevation_diff_2 REAL CHECK (elevation_diff_2 >= 0),
ADD COLUMN elevation_diff_3 REAL CHECK (elevation_diff_3 >= 0),
ADD COLUMN min_angle_index SMALLINT CHECK (min_angle_index BETWEEN 1 AND 3),
ADD COLUMN min_elevation_diff REAL CHECK (min_elevation_diff >= 0),
ADD COLUMN max_elevation_diff REAL CHECK (max_elevation_diff >= 0),
ADD COLUMN min_angle_elevation_diff REAL GENERATED ALWAYS AS (
    CASE min_angle_index
        WHEN 1 THEN ABS(neighbor_elevation_1 - neighbor_elevation_2)
        WHEN 2 THEN ABS(neighbor_elevation_2 - neighbor_elevation_3)
        WHEN 3 THEN ABS(neighbor_elevation_3 - neighbor_elevation_1)
    END
) STORED;

-- インデックス作成
CREATE INDEX idx_y_junctions_elevation
    ON y_junctions (elevation)
    WHERE elevation IS NOT NULL;

CREATE INDEX idx_y_junctions_min_elevation_diff
    ON y_junctions (min_elevation_diff)
    WHERE min_elevation_diff IS NOT NULL;

CREATE INDEX idx_y_junctions_min_angle_elevation_diff
    ON y_junctions (min_angle_elevation_diff)
    WHERE min_angle_elevation_diff IS NOT NULL;

-- コメント
COMMENT ON COLUMN y_junctions.elevation IS 'ジャンクションノードの標高（メートル、GSI DEM5Aデータ由来）';
COMMENT ON COLUMN y_junctions.min_angle_index IS '最小角のインデックス（1=angle_1, 2=angle_2, 3=angle_3）';
COMMENT ON COLUMN y_junctions.min_angle_elevation_diff IS '最小角を構成する2本の道路間の標高差（メートル）';
```

---

## 💾 Phase 5: インサート処理更新 ✅

**ゴール**: 標高データをデータベースに保存

**成果物**:
- `backend/src/importer/inserter.rs` - insert_junctions関数修正
- `backend/src/db/repository.rs` - find_by_bbox関数修正

**タスク**:
- [x] `insert_junctions`関数のSQL修正
  - [x] INSERT文に標高カラム追加
  - [x] プレースホルダー追加（$10, $11, ...）
  - [x] バインド処理追加
- [x] バルクインサートの対応
  - [x] 1000件バッチでの標高データ保存確認
- [x] `find_by_bbox`関数のSELECT修正
  - [x] 標高カラムを取得対象に追加
  - [x] Junction構造体へのマッピング
- [x] テストデータ更新
  - [x] api_tests.rs のテストデータに標高追加
- [x] エラーハンドリング
  - [x] NULL値の扱い（Option型）

**完了条件**:
- ✅ インポート時に標高データがDBに保存される
- ✅ `cargo test` で全テスト合格（ユニットテスト29個、統合テスト14個）
- ✅ SELECT時に標高データが正しく取得される

**工数**: 中（1日程度）

**依存**: Phase 4完了（マイグレーション実行済み）

**実装メモ**:
- **型の不一致修正**: DB（REAL/FLOAT4）とRust（Option<f64>→Option<f32>）の型を統一
- **原因**: Phase 3でf64を選択、Phase 4でREALを選択したため不一致が発生
- **解決**: repository.rsのJunctionRow構造体をOption<f32>に変更し、Fromトレイトでf64へキャスト
- **INSERT文**: 11個の標高関連カラムを追加（PARAMS_PER_ROW: 9→19）
- **バルクインサート**: 1000件バッチでの一括挿入に対応
- **テスト**: 全43テスト合格（cargo fmt、cargo clippy も合格）

**実装ポイント**:
```rust
// inserter.rs
sqlx::query(
    r#"
    INSERT INTO y_junctions (
        osm_node_id, location,
        angle_1, angle_2, angle_3,
        bearings,
        elevation,
        neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3,
        elevation_diff_1, elevation_diff_2, elevation_diff_3,
        min_angle_index,
        min_elevation_diff, max_elevation_diff
    ) VALUES (
        $1, ST_SetSRID(ST_MakePoint($2, $3), 4326),
        $4, $5, $6,
        ARRAY[$7, $8, $9],
        $10,
        $11, $12, $13,
        $14, $15, $16,
        $17,
        $18, $19
    )
    "#
)
.bind(junction.osm_node_id)
// ... 既存のバインド ...
.bind(junction.elevation)
.bind(junction.neighbor_elevations.map(|e| e[0]))
.bind(junction.neighbor_elevations.map(|e| e[1]))
.bind(junction.neighbor_elevations.map(|e| e[2]))
// ... 続く
```

---

## 🔌 Phase 6: API拡張 ✅

**ゴール**: 標高データをAPIで取得・フィルタリング可能にする

**成果物**:
- `backend/src/api/handlers.rs` - クエリパラメータ追加
- `backend/src/db/repository.rs` - フィルタロジック追加
- `backend/tests/api_tests.rs` - 統合テスト追加

**タスク**:
- [x] `JunctionQuery`構造体にフィルタパラメータ追加
  ```rust
  pub struct JunctionQuery {
      // 既存フィールド...
      pub min_angle_elevation_diff: Option<f64>,
  }
  ```
- [x] `find_by_bbox`関数にWHERE句追加
  - [x] min_angle_elevation_diffフィルタ
- [x] GeoJSON出力に標高データ追加（Phase 5で完了済み）
  - [x] properties.elevationに含める
  - [x] properties.min_elevation_diffに含める
  - [x] properties.max_elevation_diffに含める
  - [x] properties.min_angle_elevation_diffに含める
- [x] 統合テスト追加
  - [x] min_angle_elevation_diffフィルタのテスト
  - [x] レスポンスに標高データが含まれるテスト
  - [x] 複合フィルタのテスト

**完了条件**:
- ✅ `GET /api/junctions?bbox=...&min_angle_elevation_diff=10` でフィルタリングできる
- ✅ レスポンスJSONに標高データが含まれる
- ✅ `cargo test` で統合テスト合格（ユニットテスト29個、統合テスト17個）

**工数**: 中（1日程度）

**依存**: Phase 5完了

**実装メモ**:
- **フィルタ削減**: ニーズに基づき以下を削除
  - ❌ elevation範囲フィルタ（min_elevation, max_elevation）- 全くニーズがない
  - ❌ 高低差フィルタ（min_elevation_diff, max_elevation_diff）- 今のところニーズがない
  - ✅ 最小角の高低差フィルタ（min_angle_elevation_diff）のみ実装
- **テスト結果**:
  - ユニットテスト: 29個全てパス
  - 統合テスト: 17個全てパス
  - cargo fmt, clippy: 警告なし

**APIレスポンス例**:
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
        "id": 1,
        "osm_node_id": 123456,
        "angles": [30, 150, 180],
        "elevation": 245.5,
        "min_elevation_diff": 12.3,
        "max_elevation_diff": 18.7,
        "min_angle_elevation_diff": 15.2
      }
    }
  ]
}
```

---

## 🎨 Phase 7: フロントエンド表示 ✅

**ゴール**: UIで標高データを表示・フィルタリング

**成果物**:
- ✅ `frontend/src/types/index.ts` - 型定義更新
- ✅ `frontend/src/components/FilterPanel.tsx` - 標高フィルタ追加
- ✅ `frontend/src/components/JunctionPopup.tsx` - 標高表示追加

**タスク**:
- [x] JunctionProperties型に標高フィールド追加（`min_angle_elevation_diff`）
- [x] FilterPanelに標高フィルタUI追加
  - [x] 最小角高低差スライダー（0-50m）
- [x] JunctionPopupに標高情報表示
  - [x] 最小角を構成する道路間の高低差

**完了条件**:
- ✅ フィルタパネルで最小角標高差フィルタが動作する
- ✅ ポップアップに最小角標高差が表示される
- ✅ `npm run typecheck` 合格

**工数**: 中（1日程度）

**依存**: Phase 6完了

**実装メモ**:
- **最小構成で実装**: ニーズが最も高い「最小角の標高差」のみに絞って実装
- **FilterPanel (124-155行目)**:
  - 0-50mのスライダー（1m刻み）
  - 0の場合はフィルタなし（null）
  - リセットボタンでnullに戻す
- **JunctionPopup (28-32行目)**:
  - `min_angle_elevation_diff`が定義されている場合のみ表示
  - 小数点1桁で表示（例: 15.2m）
- **型定義**:
  - `JunctionProperties.min_angle_elevation_diff?: number` (types/index.ts:31)
  - `FilterParams.min_angle_elevation_diff?: number` (types/index.ts:62)
- **削除した機能**: ユーザーフィードバックに基づき、使用頻度の低い標高関連の表示・フィルタを削除

---

## 📦 データ準備

### GSIデータのダウンロード

**データソース**: 国土地理院 基盤地図情報 数値標高モデル (DEM5A)

**ダウンロード方法**:

1. **国土地理院 基盤地図情報ダウンロードサービス**
   - URL: https://fgd.gsi.go.jp/download/menu.php
   - 手順:
     1. 「数値標高モデル（5mメッシュ DEM）」を選択
     2. 対象地域を地図上で選択（複数選択可能）
     3. 「ダウンロードファイル確認へ」をクリック
     4. ZIPファイルをダウンロード

2. **データの解凍と配置**
   ```bash
   # リポジトリ外の任意の場所にデータディレクトリを作成
   mkdir -p ~/y-junctions-data/gsi/xml

   # ダウンロードしたZIPファイルを解凍
   unzip -j ~/Downloads/'FG-GML-*.zip' -d ~/y-junctions-data/gsi/xml/
   ```

3. **インポート時のパス指定**
   ```bash
   # OSM PBFファイルとGSI標高データを指定
   cargo run --bin import -- \
     --input ~/y-junctions-data/osm/japan-latest.osm.pbf \
     --elevation-dir ~/y-junctions-data/gsi \
     --bbox 132,33,135,35
   ```

**必要なデータ範囲**:
- 対象地域: インポート対象のOSM PBFファイルがカバーする範囲
- 例: 東京都全域の場合、約100-150タイル
- 合計サイズ: 数百MB〜数GB（対象範囲による）

**注意**:
- リポジトリ内にデータを置かない（数GB～数十GBになるため）
- データは1回ダウンロードすれば使い回せる
- CI/本番環境では別途データ配置が必要

---

## 🧪 テスト戦略

### ユニットテスト

- **Phase 1**: ElevationProviderの動作確認
- **Phase 2**: 高低差計算ロジックの確認
- **Phase 3**: 標高取得処理の確認（テスト用XMLファイル使用）

### 統合テスト

- **Phase 5**: データベースへの保存・取得確認
- **Phase 6**: APIエンドポイントの動作確認

### E2Eテスト

- **Phase 7**: ブラウザでの表示・フィルタリング確認

---

## 📋 完了チェックリスト

### コミット前チェック

- [ ] Backend: `cargo test` 全テスト合格
- [ ] Backend: `cargo fmt` 実行
- [ ] Backend: `cargo clippy -- -D warnings` 合格
- [ ] Frontend: `npm run typecheck` 合格（Phase 7の場合）
- [ ] Frontend: `npm run lint` 合格（Phase 7の場合）
- [ ] Frontend: `npm run format:check` 合格（Phase 7の場合）

### PR作成前チェック

- [ ] doc/elevation-feature.md の該当Phaseを完了マーク
- [ ] 完了条件をすべて満たしている
- [ ] READMEに必要な手順を追記（GSIダウンロード方法など）

---

## 🚀 デプロイメモ

### 本番環境での実行

```bash
# 1. データディレクトリの作成と配置（本番サーバーで実行）
mkdir -p /var/lib/y-junctions-data/osm
mkdir -p /var/lib/y-junctions-data/gsi/xml

# OSM PBFファイルを配置
cp japan-latest.osm.pbf /var/lib/y-junctions-data/osm/

# GSI基盤地図情報からダウンロードしたZIPファイルを解凍
unzip -j 'FG-GML-*.zip' -d /var/lib/y-junctions-data/gsi/xml/

# 2. マイグレーション実行
sqlx migrate run

# 3. データの再インポート
cargo run --bin import -- \
  --input /var/lib/y-junctions-data/osm/japan-latest.osm.pbf \
  --elevation-dir /var/lib/y-junctions-data/gsi \
  --bbox 123.0,24.0,146.0,46.0

# 4. インポート後、XMLファイルは削除可能（任意）
# rm -rf /var/lib/y-junctions-data/gsi/xml
```

### パフォーマンス目標

- インポート時間: +20-30%増（標高取得のオーバーヘッド）
- API応答時間: 変化なし（インデックス使用）
- ストレージ増加: 約40MB（100万レコードの場合）

---

## 🔧 Phase 8: インポート処理の2段階コミット化（改善提案）

**ゴール**: OSMデータと標高データのインポートを分離し、より堅牢な処理を実装

**現在の問題**:
- OSMデータ取得 → 標高データ取得 → 一括挿入 → コミット
- 標高データ取得で失敗すると、OSMデータも全て失われる
- 標高データは外部依存（GSI XML）で失敗しやすい

**提案する改善**:
1. **Phase 1: OSMデータのみ先にコミット**
   - PBFから取得したY字路データ（座標、角度、bearings）を先にINSERT & COMMIT
   - トランザクション1: OSMデータの永続化
2. **Phase 2: 標高データをUPDATEで追加**
   - 保存済みのY字路に対して標高データを取得
   - トランザクション2: 標高データの更新
   - 失敗してもOSMデータは残る

**メリット**:
- OSMデータと標高データが独立
- 標高取得失敗でもY字路データは保存される
- 標高データは後から再取得可能
- より堅牢なインポート処理

**実装方針**:
```rust
// Phase 1: OSM data only
pub async fn import_osm_data(pool: &PgPool, junctions: Vec<JunctionForInsert>) -> Result<()> {
    let mut tx = pool.begin().await?;
    // INSERT without elevation fields
    tx.commit().await?;
    Ok(())
}

// Phase 2: Elevation data (optional)
pub async fn update_elevation_data(pool: &PgPool, elevation_dir: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    // UPDATE elevation fields
    tx.commit().await?;
    Ok(())
}
```

**成果物**:
- `backend/src/importer/mod.rs` - 2段階インポート処理
- `backend/src/importer/inserter.rs` - OSMデータのみのINSERT関数追加
- `backend/src/importer/elevation_updater.rs` - 標高データUPDATE処理（新規）
- `backend/src/bin/import.rs` - `--elevation-only` オプション追加

**タスク**:
- [ ] OSMデータのみをINSERTする関数を実装
  - [ ] `insert_osm_data()` - 標高フィールドを除外したINSERT
  - [ ] トランザクション1: OSMデータの永続化
- [ ] 標高データをUPDATEする関数を実装
  - [ ] `elevation_updater.rs` 新規作成
  - [ ] `update_elevation_data()` - 既存レコードに標高を追加
  - [ ] トランザクション2: 標高データの更新
- [ ] CLIオプション追加
  - [ ] `--elevation-only` フラグ（標高データのみ更新）
  - [ ] デフォルトは2段階実行（OSM → 標高）
- [ ] エラーハンドリング改善
  - [ ] Phase 1失敗時は即座に終了
  - [ ] Phase 2失敗時はログ出力して継続
- [ ] テスト追加
  - [ ] OSMデータのみのインポートテスト
  - [ ] 標高データの後付けテスト

**完了条件**:
- [ ] OSMデータと標高データが独立してインポート可能
- [ ] 標高取得失敗でもOSMデータは保存される
- [ ] `--elevation-only` で既存データに標高を追加できる
- [ ] 全テスト合格

**工数**: 中（1-2日程度）

**依存**: Phase 5完了

**優先度**: 中（現行の実装でも動作するが、より堅牢性を求める場合に実装）

---

## 🔍 Phase 9: 高低差フィルタの範囲検索対応 ✅

**ゴール**: 最小角標高差フィルタを「最小値のみ」から「最小値と最大値の両方を指定可能」な範囲検索に変更

**背景**:
Phase 6で実装した`min_angle_elevation_diff`フィルタは下限値のみの指定だったが、範囲指定（「1.0m以上3.0m以下」のような条件）のニーズがあった。既存の角度範囲フィルタと同じUIパターンを採用し、統一感のあるUXを実現する。

**成果物**:
- ✅ `backend/src/api/handlers.rs` - max_angle_elevation_diffパラメータ追加
- ✅ `backend/src/db/repository.rs` - SQL WHERE句にmax条件追加
- ✅ `backend/tests/api_tests.rs` - テストケース4個追加
- ✅ `frontend/src/types/index.ts` - 型定義更新
- ✅ `frontend/src/hooks/useFilters.ts` - 状態管理を配列に変更
- ✅ `frontend/src/components/FilterPanel.tsx` - 2つのスライダーUI実装
- ✅ `frontend/src/App.tsx` - 統合
- ✅ `frontend/src/api/client.ts` - APIクライアント更新

**タスク**:
- [ ] Backend: `JunctionsQuery`構造体に`max_angle_elevation_diff`フィールド追加
- [ ] Backend: バリデーション実装
  - [ ] max_angle_elevation_diff: 0-10の範囲チェック
  - [ ] 両方指定時: min <= max をチェック
- [ ] Backend: `FilterParams`構造体に`max_angle_elevation_diff`追加
- [ ] Backend: `add_elevation_filters()`関数にSQL条件追加
  - [ ] `WHERE min_angle_elevation_diff <= ?`
- [ ] Backend: 統合テスト4個追加
  - [ ] 最大値のみ指定（正常系）
  - [ ] 範囲指定（両方指定）（正常系）
  - [ ] min > max エラー（異常系）
  - [ ] max > 10 エラー（異常系）
- [ ] Frontend: 型定義更新
  - [ ] `FilterParams.max_angle_elevation_diff?: number`
- [ ] Frontend: `useFilters`フックの状態管理変更
  - [ ] `minAngleElevationDiff: number | null` → `elevationDiffRange: [number, number]`
  - [ ] デフォルト値: `[0, 10]`
  - [ ] `toFilterParams()`で2つのパラメータに分割して送信
- [ ] Frontend: FilterPanelコンポーネントUI変更
  - [ ] 1つのスライダー → 2つの独立したスライダー（角度範囲フィルタと同じパターン）
  - [ ] 最小値スライダー: 0-10m（ステップ0.5m）
  - [ ] 最大値スライダー: 0-10m（ステップ0.5m）
  - [ ] 値の衝突防止ロジック（最小間隔0.5m）
  - [ ] リセットボタン
- [ ] Frontend: App.tsx統合
- [ ] Frontend: APIクライアント更新
  - [ ] max_angle_elevation_diffパラメータ送信

**完了条件**:
- ✅ API: `GET /api/junctions?bbox=...&min_angle_elevation_diff=2&max_angle_elevation_diff=5` で範囲検索可能
- ✅ API: 最小値のみ、最大値のみ、両方のいずれも指定可能（完全な下位互換性）
- ✅ UI: 0-10mの範囲を2つのスライダーで直感的に指定可能
- ✅ UI: 最大値10を選択した場合「10m以上」と表示され、上限なしでフィルタリング
- ✅ UI: 角度範囲フィルタと統一されたデザイン
- ✅ Backend: 全テスト合格（ユニットテスト29個 + 統合テスト18個 = 47個）
- ✅ Backend: cargo fmt, cargo clippy 合格
- ✅ Frontend: npm run typecheck, lint, build 合格
- ✅ 下位互換性維持（既存のmin_angle_elevation_diffパラメータは変更なし）

**工数**: 中（4-6時間）

**依存**: Phase 6, 7完了

**実装メモ**:
- **下位互換性の保証**:
  - 既存パラメータ`min_angle_elevation_diff`は変更なし
  - 新規パラメータ`max_angle_elevation_diff`はオプショナル
  - 既存のAPIユーザーは影響を受けない
- **UIパターンの統一**:
  - 角度範囲フィルタ（FilterPanel.tsx 76-121行目）と同じ実装パターン
  - 2つの独立したスライダー
  - handleMinChange/handleMaxChangeで値の衝突を防ぐ
- **最大範囲の拡張**: 5m → 10m（ユーザー要求による）
- **「10m以上」の実装**:
  - スライダーの最大値を10に設定
  - 10を選択した場合、UIに「10m以上」と表示
  - バックエンドにはmax_angle_elevation_diffを送信しない（上限なし）
  - これにより10m以上のすべてのY字路が表示される
- **デフォルト値の扱い**:
  - Frontend: `[0, 10]` = 「全範囲（0m 〜 10m以上）」
  - Backend: 初期値の場合はパラメータを送信しない
  - `toFilterParams()`で判定: `[0] > 0` または `[1] < 10` の場合のみ送信
  - デフォルト状態では全データを表示（フィルタなし）

**テスト実行コマンド**:
```bash
# Backend
cargo test --manifest-path backend/Cargo.toml
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml -- -D warnings

# Frontend
cd frontend
npm run typecheck
npm run format:check
npm run lint
npm run build
```

---

## 🔗 関連ドキュメント

- [国土地理院 基盤地図情報](https://fgd.gsi.go.jp/)
- [基盤地図情報ダウンロードサービス](https://fgd.gsi.go.jp/download/menu.php)
- [JPGIS (GML) 仕様](https://www.gsi.go.jp/common/000194267.pdf)
- [roxmltree crate documentation](https://docs.rs/roxmltree/)
- [glob crate documentation](https://docs.rs/glob/)
