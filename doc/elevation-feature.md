# 標高データ機能 開発タスクリスト

## 概要

Y字路の標高情報（elevation）と隣接ノード間の高低差を取得・保存する機能を追加します。

### 技術スタック

- **標高データソース**: SRTM1 (Shuttle Radar Topography Mission)
  - 解像度: 約30m (1 arc-second)
  - 垂直精度: ±6-16m
  - カバー範囲: 全世界（日本を含む）
- **データ形式**: HGT (Height) バイナリファイル
- **Rustクレート**: `srtm` または `hgt`

### アーキテクチャ方針

- **データ取得**: インポート時にSRTM HGTファイルから標高を計算
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

## 🗄️ Phase 1: SRTM基盤実装

**ゴール**: SRTM HGTファイルから標高を取得する基盤を実装

**成果物**:
- `backend/src/importer/elevation.rs` - 標高取得モジュール
- `backend/Cargo.toml` - srtmクレート追加

**タスク**:
- [ ] srtmクレート依存関係追加（`srtm = "0.3"`）
- [ ] `ElevationProvider`構造体実装
  - [ ] `new(data_dir: &str)` - HGTディレクトリパス指定
  - [ ] `get_elevation(lat: f64, lon: f64)` - 緯度経度から標高取得
  - [ ] HGTファイルのキャッシング機能（HashMap利用）
  - [ ] タイル座標計算（N35E138形式）
- [ ] エラーハンドリング
  - [ ] HGTファイル未存在の処理
  - [ ] 海域・欠損値（-32768）の処理
- [ ] ユニットテスト
  - [ ] 標高取得の正常系テスト
  - [ ] ファイル未存在時のテスト
  - [ ] キャッシング動作のテスト

**完了条件**:
- [ ] `cargo test` で elevation モジュールのテスト合格
- [ ] 富士山頂（35.3606, 138.7274）の標高が約3776m取得できる
- [ ] 東京駅（35.6812, 139.7671）の標高が約3m取得できる

**工数**: 中（1日程度）

**実装例**:
```rust
pub struct ElevationProvider {
    tiles: HashMap<(i32, i32), srtm::Tile>,
    data_dir: String,
}

impl ElevationProvider {
    pub fn new(data_dir: &str) -> Self { /* ... */ }

    pub fn get_elevation(&mut self, lat: f64, lon: f64) -> Result<Option<f64>> {
        // タイル座標計算
        let tile_lat = lat.floor() as i32;
        let tile_lon = lon.floor() as i32;

        // HGTファイル読み込み（キャッシュ利用）
        // 標高値取得
    }
}
```

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
- [ ] 標高データがOptionalで扱える（HGTファイルがない場合もエラーにならない）

**工数**: 小（半日程度）

---

## 🔄 Phase 3: インポート処理統合

**ゴール**: OSMインポート時に標高データを取得・計算

**成果物**:
- `backend/src/importer/parser.rs` - parse_pbf関数修正
- `backend/src/importer/mod.rs` - elevationモジュール公開

**タスク**:
- [ ] `parse_pbf`関数にsrtm_dir引数追加
  ```rust
  pub fn parse_pbf(
      input_path: &str,
      srtm_dir: Option<&str>,  // 追加
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
  - [ ] HGTファイルがない場合は標高なしで続行
  - [ ] 一部のノードで標高が取得できない場合の処理

**完了条件**:
- [ ] `cargo run --bin import -- --input test.pbf --srtm-dir data/srtm --bbox ...` が成功
- [ ] 標高データが取得され、JunctionForInsertに格納される
- [ ] ログに標高取得の統計が表示される

**工数**: 中（1日程度）

**依存**: Phase 1, 2完了

**実装ポイント**:
```rust
// 3rd pass内での標高取得
let mut elevation_provider = srtm_dir.map(|dir| ElevationProvider::new(dir));

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
COMMENT ON COLUMN y_junctions.elevation IS 'ジャンクションノードの標高（メートル、SRTM1データ由来）';
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

### SRTMデータのダウンロード

**必要なHGTファイル（日本の場合）**:
- 北緯24°〜46° × 東経123°〜146°
- 陸地のみで約150-200タイル
- 合計サイズ: 約3.75-5GB

**ダウンロード方法**:

1. **OpenTopography S3バケット（推奨・認証不要）**
   ```bash
   # AWS CLIインストール
   brew install awscli  # macOS

   # 日本の範囲をダウンロード
   mkdir -p data/srtm
   for lat in {24..46}; do
     for lon in {123..146}; do
       aws s3 cp \
         s3://raster/SRTM_GL1/N${lat}E${lon}.hgt \
         data/srtm/ \
         --endpoint-url https://opentopography.s3.sdsc.edu \
         --no-sign-request 2>/dev/null && \
         echo "Downloaded N${lat}E${lon}.hgt"
     done
   done
   ```

2. **インタラクティブツール**
   - https://dwtkns.com/srtm30m/
   - 地図上でクリックしてダウンロード

3. **NASA Earthdata（公式）**
   - https://search.earthdata.nasa.gov/
   - アカウント登録が必要

### .gitignoreへの追加

```bash
# data/srtm/*.hgt
echo "data/srtm/*.hgt" >> .gitignore
```

---

## 🧪 テスト戦略

### ユニットテスト

- **Phase 1**: ElevationProviderの動作確認
- **Phase 2**: 高低差計算ロジックの確認
- **Phase 3**: 標高取得処理の確認（モックHGTファイル使用）

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
- [ ] READMEに必要な手順を追記（SRTMダウンロード方法など）

---

## 🚀 デプロイメモ

### 本番環境での実行

```bash
# 1. SRTMデータのダウンロード（本番サーバーで実行）
mkdir -p data/srtm
# ... ダウンロードスクリプト実行 ...

# 2. マイグレーション実行
sqlx migrate run

# 3. データの再インポート
cargo run --bin import -- \
  --input data/japan-latest.osm.pbf \
  --srtm-dir data/srtm \
  --min-lon 123.0 --max-lon 146.0 \
  --min-lat 24.0 --max-lat 46.0

# 4. インポート後、HGTファイルは削除可能（任意）
# rm -rf data/srtm
```

### パフォーマンス目標

- インポート時間: +20-30%増（標高取得のオーバーヘッド）
- API応答時間: 変化なし（インデックス使用）
- ストレージ増加: 約40MB（100万レコードの場合）

---

## 🔗 関連ドキュメント

- [SRTM - OpenStreetMap Wiki](https://wiki.openstreetmap.org/wiki/SRTM)
- [NASA SRTM Documentation](https://lpdaac.usgs.gov/products/srtmgl1v003/)
- [srtm crate documentation](https://docs.rs/srtm/)
