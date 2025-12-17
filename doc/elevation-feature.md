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
backend/data/gsi/
  └── xml/
      ├── FG-GML-5338-05-00-DEM5A-*.xml
      ├── FG-GML-5338-05-01-DEM5A-*.xml
      └── ... (解凍済みXMLファイル)
```

---

## 🔧 Phase 2: データモデル拡張 ✅

**ゴール**: 標高データを扱うためのデータ構造を拡張

**成果物**:
- `backend/src/importer/detector.rs` - JunctionForInsert構造体拡張
- `backend/src/domain/junction.rs` - Junction構造体拡張

**タスク**:
- [x] `JunctionForInsert`構造体に標高フィールド追加
  ```rust
  pub struct JunctionForInsert {
      // 既存フィールド...
      pub elevation: Option<f64>,
      pub neighbor_elevations: Option<[f64; 3]>,
      pub elevation_diffs: Option<[f64; 3]>,
      pub min_angle_index: Option<i16>,
      pub min_elevation_diff: Option<f64>,
      pub max_elevation_diff: Option<f64>,
  }
  ```
- [x] ヘルパーメソッド実装
  - [x] `calculate_min_angle_index(angles: &[i16; 3]) -> i16` - 1-based (1,2,3)を返すよう実装
  - [x] `calculate_elevation_diffs(base: f64, neighbors: &[f64; 3]) -> [f64; 3]`
  - [x] `calculate_min_max_diffs(diffs: &[f64; 3]) -> (f64, f64)`
- [x] `Junction`構造体に標高フィールド追加
  ```rust
  pub struct Junction {
      // 既存フィールド...
      pub elevation: Option<f64>,
      pub min_elevation_diff: Option<f64>,
      pub max_elevation_diff: Option<f64>,
      pub min_angle_elevation_diff: Option<f64>,
  }
  ```
- [x] ユニットテスト
  - [x] 最小角インデックス計算のテスト (1,2,3を期待)
  - [x] 高低差計算のテスト
- [x] `to_feature()` メソッド更新（標高データをGeoJSON propertiesに追加）
- [x] 既存の初期化箇所の修正（None値で初期化）

**完了条件**:
- ✅ `cargo test` でドメインモデルのテスト合格（26テスト全て成功）
- ✅ 標高データがOptionalで扱える（XMLファイルがない場合もエラーにならない）
- ✅ `cargo fmt` と `cargo clippy` チェック成功
- ✅ Phase 7の検索要件を満たすデータ構造（elevation, min_elevation_diff, min_angle_elevation_diff）

**工数**: 小（半日程度）

**実装メモ**:
- `calculate_min_angle_index` は1-based (1,2,3) を返す（PostgreSQL CHECK制約とCASE文に対応）
- 全フィールドに `#[allow(dead_code)]` 属性を付与（Phase 3以降で使用）
- `elevation_diffs` はジャンクションノードと隣接ノードの高低差を計算（junction-to-neighbor）
- `min_angle_elevation_diff` はPostgreSQLのGenerated Columnで自動計算（neighbor-to-neighbor）

---

## 🔄 Phase 3: インポート処理統合 ✅

**ゴール**: OSMインポート時に標高データを取得・計算

**成果物**:
- `backend/src/bin/import.rs` - `--elevation-dir`オプション追加
- `backend/src/importer/mod.rs` - `elevation_dir`引数追加
- `backend/src/importer/parser.rs` - 標高取得ロジック実装

**タスク**:
- [x] `parse_pbf`関数に`elevation_dir`引数追加（`gsi_dir`→`elevation_dir`に変更）
  ```rust
  pub fn parse_pbf(
      input_path: &str,
      elevation_dir: Option<&str>,  // 実装詳細を隠蔽
      min_lon: f64,
      min_lat: f64,
      max_lon: f64,
      max_lat: f64,
  ) -> Result<Vec<JunctionForInsert>>
  ```
- [x] ElevationProviderの初期化
- [x] 3rd passで標高取得処理追加
  - [x] ジャンクションノードの標高取得
  - [x] 3つの隣接ノードの標高取得
  - [x] 高低差計算
  - [x] 最小角インデックス計算
- [x] ログ出力追加
  - [x] 標高取得成功/失敗の統計
  - [x] `ElevationStats`構造体で統計管理
  - [x] `log_elevation_stats()`関数で詳細ログ出力
- [x] エラーハンドリング
  - [x] XMLファイルがない場合は標高なしで続行
  - [x] 一部のノードで標高が取得できない場合の処理

**完了条件**:
- ⚠️ `cargo run --bin import -- --input test.pbf --elevation-dir data/gsi --bbox ...` が成功（実行したが地理的不一致により標高データ0件）
- ✅ 標高データが取得され、JunctionForInsertに格納される（コード実装完了）
- ✅ ログに標高取得の統計が表示される（ヘルパー関数実装済み）
- ✅ `cargo test` 全テスト合格（29個、parser.rsに3個のユニットテストを追加）
- ✅ `cargo fmt --check` 合格
- ✅ `cargo clippy -- -D warnings` 合格

**工数**: 中（1日程度）

**依存**: Phase 1, 2完了

**実装メモ**:
- **命名規則の改善**:
  - `gsi_dir` → `elevation_dir`（実装詳細を隠蔽）
  - `ElevationData` → `JunctionElevation`（セマンティックな命名）
- **エラーハンドリング**:
  - `elevation_dir = None` → 標高取得をスキップ、後方互換性維持
  - `get_elevation()` エラー → ログ警告して`None`、処理続行
  - 一部の隣接ノードのみ標高取得 → 全て`None`（整合性維持）
- **標高の2つの用途**:
  1. `elevation_diffs` (junction→neighbor): 道路の傾斜分析用
  2. `min_angle_elevation_diff` (neighbor↔neighbor): Y字路の左右の高低差（Phase 4でDB Generated Columnとして実装予定）
- **コード品質**:
  - Clippy警告なし（type complexity解消、redundant closure削除）
  - 構造体でセマンティックな意味を表現
- **ヘルパー関数**:
  - `ElevationStats`: 統計情報管理
  - `JunctionElevation`: 標高データ構造
  - `get_elevation_data()`: 標高取得処理
  - `log_elevation_stats()`: 統計ログ出力
- **ユニットテスト**（parser.rs）:
  - `test_elevation_stats_initialization`: ElevationStats構造体の初期化検証
  - `test_junction_elevation_structure_none`: JunctionElevation構造体（Noneの場合）の検証
  - `test_junction_elevation_structure_with_data`: JunctionElevation構造体（データありの場合）の検証

**次のステップ**:
- Phase 4（データベーススキーマ拡張）へ進む
- Phase 5（インサート処理更新）で実際にDBへ標高データを保存

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

## 🗄️ Phase 4: データベーススキーマ拡張 ✅

**ゴール**: 標高データを保存するためのDBスキーマ変更

**成果物**:
- `backend/migrations/003_add_elevation.sql` - マイグレーションSQL

**タスク**:
- [x] マイグレーションSQL作成
  - [x] 標高カラム追加（elevation, neighbor_elevation_1~3）
  - [x] 高低差カラム追加（elevation_diff_1~3）
  - [x] 最小角インデックス追加（min_angle_index）
  - [x] 計算済みカラム追加（min_elevation_diff, max_elevation_diff）
  - [x] Generated Column追加（min_angle_elevation_diff）
- [x] インデックス作成
  - [x] `CREATE INDEX idx_y_junctions_elevation ON y_junctions (elevation)`
  - [x] `CREATE INDEX idx_y_junctions_min_elevation_diff ON y_junctions (min_elevation_diff)`
  - [x] `CREATE INDEX idx_y_junctions_min_angle_elevation_diff ON y_junctions (min_angle_elevation_diff)`
- [x] コメント追加（各カラムの説明）
- [x] マイグレーション実行テスト

**完了条件**:
- ✅ `sqlx migrate run` でマイグレーション成功
- ✅ `\d y_junctions` で新しいカラムが表示される（11カラム追加）
- ✅ Generated Columnが正しく動作する（min_angle_elevation_diffの自動計算を確認）

**工数**: 小（半日程度）

**依存**: Phase 3完了（実装確定後）

**実装メモ**:
- マイグレーション003_add_elevation.sqlを作成し、全ての標高関連カラムを追加
- 3つのインデックスを作成（WHERE句でNULL値を除外）
- 全カラムにコメントを追加（高低差のコメントは「何と何の差」か明示）
- Generated Column (min_angle_elevation_diff) はmin_angle_indexに基づいて自動計算される

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

## 💾 Phase 5: インサート処理更新

**ゴール**: 標高データをデータベースに保存

**成果物**:
- `backend/src/importer/inserter.rs` - insert_junctions関数修正
- `backend/src/db/repository.rs` - find_by_bbox関数修正

**タスク**:
- [ ] `insert_junctions`関数のSQL修正
  - [ ] INSERT文に標高カラム追加
  - [ ] プレースホルダー追加（$10, $11, ...）
  - [ ] バインド処理追加
- [ ] バルクインサートの対応
  - [ ] 1000件バッチでの標高データ保存確認
- [ ] `find_by_bbox`関数のSELECT修正
  - [ ] 標高カラムを取得対象に追加
  - [ ] Junction構造体へのマッピング
- [ ] テストデータ更新
  - [ ] api_tests.rs のテストデータに標高追加
- [ ] エラーハンドリング
  - [ ] NULL値の扱い（Option型）

**完了条件**:
- [ ] インポート時に標高データがDBに保存される
- [ ] `cargo test` で全テスト合格（統合テスト含む）
- [ ] SELECT時に標高データが正しく取得される

**工数**: 中（1日程度）

**依存**: Phase 4完了（マイグレーション実行済み）

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

## 🔌 Phase 6: API拡張

**ゴール**: 標高データをAPIで取得・フィルタリング可能にする

**成果物**:
- `backend/src/api/handlers.rs` - クエリパラメータ追加
- `backend/src/db/repository.rs` - フィルタロジック追加

**タスク**:
- [ ] `JunctionQuery`構造体にフィルタパラメータ追加
  ```rust
  pub struct JunctionQuery {
      // 既存フィールド...
      pub min_elevation: Option<f64>,
      pub max_elevation: Option<f64>,
      pub min_elevation_diff: Option<f64>,
      pub max_elevation_diff: Option<f64>,
      pub min_angle_elevation_diff: Option<f64>,
  }
  ```
- [ ] `find_by_bbox`関数にWHERE句追加
  - [ ] elevation範囲フィルタ
  - [ ] min_elevation_diffフィルタ
  - [ ] min_angle_elevation_diffフィルタ
- [ ] GeoJSON出力に標高データ追加
  - [ ] properties.elevationに含める
  - [ ] properties.min_elevation_diffに含める
- [ ] APIドキュメント更新（コメント）
- [ ] 統合テスト追加
  - [ ] 標高フィルタのテスト
  - [ ] レスポンスに標高データが含まれるテスト

**完了条件**:
- [ ] `GET /api/junctions?bbox=...&min_elevation_diff=10` でフィルタリングできる
- [ ] レスポンスJSONに標高データが含まれる
- [ ] `cargo test` で統合テスト合格

**工数**: 中（1日程度）

**依存**: Phase 5完了

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

## 🎨 Phase 7: フロントエンド表示（オプション）

**ゴール**: UIで標高データを表示・フィルタリング

**成果物**:
- `frontend/src/types/index.ts` - 型定義更新
- `frontend/src/components/FilterPanel.tsx` - 標高フィルタ追加
- `frontend/src/components/JunctionPopup.tsx` - 標高表示追加

**タスク**:
- [ ] JunctionProperties型に標高フィールド追加
- [ ] FilterPanelに標高フィルタUI追加
  - [ ] 標高範囲スライダー（0-4000m）
  - [ ] 最小高低差スライダー（0-500m）
  - [ ] 最小角高低差スライダー（0-500m）
- [ ] JunctionPopupに標高情報表示
  - [ ] ジャンクション標高
  - [ ] 最小/最大高低差
  - [ ] 最小角を構成する道路間の高低差
- [ ] マーカー色を標高で変える（オプション）
  - [ ] 標高が高いほど濃い色
  - [ ] または高低差で色分け
- [ ] ツールチップに標高表示（オプション）

**完了条件**:
- [ ] フィルタパネルで標高フィルタが動作する
- [ ] ポップアップに標高情報が表示される
- [ ] `npm run typecheck` 合格

**工数**: 中（1日程度）

**依存**: Phase 6完了

**優先度**: 低（バックエンド完成後に実装）

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
   # ダウンロードフォルダからbackendディレクトリへ移動
   cd backend
   mkdir -p data/gsi
   mv ~/Downloads/FG-GML-*.zip data/gsi/

   # XMLファイルを解凍
   cd data/gsi
   mkdir -p xml
   unzip -j '*.zip' -d xml/
   ```

**必要なデータ範囲**:
- 対象地域: インポート対象のOSM PBFファイルがカバーする範囲
- 例: 東京都全域の場合、約100-150タイル
- 合計サイズ: 数百MB〜数GB（対象範囲による）

### .gitignoreへの追加

```bash
# GSI XMLデータは大きいのでgit管理外
echo "backend/data/gsi/*.zip" >> .gitignore
echo "backend/data/gsi/xml/*.xml" >> .gitignore
```

**注意**:
- テスト用の小規模XMLファイル（数ファイル）はリポジトリに含めてもよい
- CI/本番環境では別途データ配置が必要（Phase 2/3で検討）

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
# 1. GSIデータのダウンロードと配置（本番サーバーで実行）
mkdir -p data/gsi/xml
# GSI基盤地図情報からダウンロードしたZIPファイルを解凍
unzip -j 'FG-GML-*.zip' -d data/gsi/xml/

# 2. マイグレーション実行
sqlx migrate run

# 3. データの再インポート
cargo run --bin import -- \
  --input data/japan-latest.osm.pbf \
  --gsi-dir data/gsi \
  --min-lon 123.0 --max-lon 146.0 \
  --min-lat 24.0 --max-lat 46.0

# 4. インポート後、XMLファイルは削除可能（任意）
# rm -rf data/gsi
```

### パフォーマンス目標

- インポート時間: +20-30%増（標高取得のオーバーヘッド）
- API応答時間: 変化なし（インデックス使用）
- ストレージ増加: 約40MB（100万レコードの場合）

---

## 🔗 関連ドキュメント

- [国土地理院 基盤地図情報](https://fgd.gsi.go.jp/)
- [基盤地図情報ダウンロードサービス](https://fgd.gsi.go.jp/download/menu.php)
- [JPGIS (GML) 仕様](https://www.gsi.go.jp/common/000194267.pdf)
- [roxmltree crate documentation](https://docs.rs/roxmltree/)
- [glob crate documentation](https://docs.rs/glob/)
