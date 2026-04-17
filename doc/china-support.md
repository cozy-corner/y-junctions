# 中国Y字路対応設計

## 背景・動機

中国本土（上海・北京・深圳等）のY字路データを本プロジェクトに取り込み、他地域と同様に「鋭角の視認性確認」ができるようにする。

中国では以下の制約があるため、従来の Google Street View 連携は使えない:

- **Google Street View**: 中国本土はカバーなし
- **Bing Streetside**: 中国本土はカバーなし
- **高德 / Tencent Map 街景**: Web での座標ベース呼び出し不可
- **Mapillary**: カバレッジが疎、中国では実用外
- **百度地図 全景**: 中国本土で最もカバレッジが充実、**唯一の現実的選択肢**

したがって本対応では **中国域内のY字路のみストリートビュー URL を百度形式に切り替える**。

## 調査結果サマリ

| 項目 | 結果 |
|---|---|
| `https://map.baidu.com/@<mc_x>,<mc_y>,21z#panotype=street&pid=X&panoid=X` 形式で1クリック街景オープン | ✅ 実証済み |
| WGS84 → BD-09MC 変換 | ✅ 公開アルゴリズムで実装可能 |
| panoid 取得（非公式エンドポイント `mapsv0.bdimg.com/?qt=qsdata`） | ✅ APIキー不要で動作確認 |
| 日本から百度街景の閲覧 | ✅ 動作確認 |

## 確定した設計判断

1. **距離フィルタ: 10m**（地上メートル。百度撮影車の撮影間隔が物理的下限）
2. **heading はランタイム計算**（パノラマ位置から Y字路位置へのコンパス方位）
3. **DB に保存するのは外部API由来の事実のみ**: `baidu_panoid`, `baidu_pano_mc_x`, `baidu_pano_mc_y`
4. **保存形式は BD-09 MC**（API応答のネイティブ形式。逆変換不要、ロスレス）
5. **拡張方式**で実装: 既存 `Junction` / `JunctionRow` / `repository.rs` / `to_feature` を一切変更せず、Baidu 関連を独立モジュールに集約
6. **中国判定は Rust 関数**で都度（DB カラムには持たない）
7. **インポートパイプライン**: `import_elevation.rs` と同じ後処理パターン。`import.rs` / `import_two_way.rs` は変更しない
8. **パイロット地域**: 上海中心部 `bbox=121.4,31.15,121.55,31.30`
9. **PR分割**: 2PR方式（機能実装 → データ投入）

## 保存するものと計算するもの

「**外部から取得する必要がある事実は保存、ローカル計算で得られる値は保存しない**」原則:

| 値 | 取得元 | 保存する？ |
|---|---|---|
| `baidu_panoid` | 百度 API（外部）| ✅ |
| `baidu_pano_mc_x`, `baidu_pano_mc_y` | 百度 API レスポンス（外部）| ✅ |
| Y字路の BD-09 MC 座標 | 保存済み (lat/lon) から順変換 | ❌ ランタイム計算 |
| heading (Baidu用) | pano_mc + junction_mc から atan2 | ❌ ランタイム計算 |
| 中国本土判定結果 | lat/lon から計算 | ❌ ランタイム計算 |

## アーキテクチャ: 拡張方式

**方針**: 既存の Junction/Repository/handlers を触らず、Baidu 関連は独立ファイル群に分離。分岐は 1 関数に閉じ込める。

### ファイル構成

```
backend/src/
├── domain/
│   ├── junction.rs                ← 変更なし
│   └── china.rs                   ← 新規（判定・座標変換・Baidu URL生成）
├── db/
│   ├── repository.rs              ← 変更なし（既存 SELECT 5箇所、JunctionRow, From 実装すべて据置）
│   └── baidu_repository.rs        ← 新規（baidu 列の SELECT/UPDATE 専用）
├── importer/
│   ├── mod.rs                     ← `pub mod baidu;` の1行のみ追加
│   └── baidu.rs                   ← 新規（HTTP クライアント: パノラマ情報取得のみ）
├── api/
│   ├── handlers.rs                ← 既存の `to_feature` 呼び出しを `enricher` に置換（3箇所の1行差し替え）
│   └── streetview_enricher.rs     ← 新規（全ての分岐をここに閉じ込める）
└── bin/
    └── import_baidu_panoid.rs     ← 新規（CLI バイナリ、`import_elevation.rs` を雛形）
```

### データフロー

```
(1) OSMインポート (変更なし)
  import.rs / import_two_way.rs
    ↓ PBFパース → 一括INSERT
    ↓ baidu_panoid 等は NULL のまま挿入

(2) Baidu panoid 取得（新規バイナリ）
  import_baidu_panoid.rs
    ↓ find_without_baidu_panoid で NULL 行取得
    ↓ is_in_china_mainland で中国本土内のみフィルタ
    ↓ WGS84 → BD-09 MC 変換（問い合わせ用）
    ↓ mapsv0.bdimg.com/?qt=qsdata で HTTP 問い合わせ (500ms sleep)
    ↓ 距離計算: cos(lat) 補正した地上メートル、10m超は捨てる
    ↓ bulk_update_baidu で (panoid, pano_mc_x, pano_mc_y) を保存

(3) ランタイム (API レスポンス)
  GET /api/junctions
    ↓ repository::find_by_bbox → junctions: Vec<Junction>   (既存 SELECT そのまま)
    ↓ streetview_enricher::enrich_collection(pool, junctions, total_count)
      ├ baidu_repository::find_by_junction_ids(&ids) → HashMap<id, BaiduInfo>  (SELECT +1回)
      ├ 各 junction について build_url(&junction, baidu_map.get(&id)) で URL 決定
      │   ├ 中国本土 + panoid あり → BD-09 MC 変換 + heading 計算 + Baidu URL
      │   ├ 中国本土 + panoid なし → ""（空文字、フロントで非表示）
      │   └ 中国外 → 既存 junction.streetview_url() の Google URL
      └ feature の streetview_url を上書き
    ↓ GeoJSON FeatureCollection を返す
```

### 分岐の場所

分岐（`if china { baidu } else { google }`）は **`streetview_enricher::build_url` 関数 1箇所のみ**。他のファイルには中国関連の条件分岐は一切入れない。

### 戻り値の互換性

- API レスポンスのスキーマ（プロパティ名、型、単位）は **完全に同じ**
- `streetview_url` のコンテンツ形式だけ URL の形が変わる（Google URL → Baidu URL or 空文字）
- フロントは `href={streetview_url}` で扱うのみでパースしないので問題なし
- Baidu URL 時のみフロントで条件付きレンダリング（空文字チェック）

## 中国本土判定 (`is_in_china_mainland`)

- 大枠: `73° ≤ lng ≤ 135°` かつ `18° ≤ lat ≤ 54°`
- 除外（Google Street View が利用可能な中華圏）:
  - 香港: `113.8 ≤ lng ≤ 114.5, 22.1 ≤ lat ≤ 22.6`
  - マカオ: `113.5 ≤ lng ≤ 113.6, 22.1 ≤ lat ≤ 22.2`
  - 台湾: `119.3 ≤ lng ≤ 122.0, 21.9 ≤ lat ≤ 25.3`

境界線上の誤判定は「ストリートビューボタンが出ない」という UX 劣化で済む（精度要件は緩い）。

## 座標変換: WGS84 → BD-09 Mercator

3段階変換（各段階のアルゴリズムは公開）:
1. WGS84 → GCJ-02（中国の国家標準）
2. GCJ-02 → BD-09ll（百度独自、lng/lat 形式）
3. BD-09ll → BD-09 MC（百度独自メルカトル投影、単位はメートル）

外部 crate は使わず `backend/src/domain/china.rs` に実装。**順変換のみで十分**（逆変換は不要）。

## panoid 取得エンドポイント

- URL: `https://mapsv0.bdimg.com/?qt=qsdata&x=<bd09mc_x>&y=<bd09mc_y>&l=17`
- 必須ヘッダ: `User-Agent: Mozilla/5.0 ...`
- 応答JSON例（成功時）:
  ```json
  {
    "content": {
      "RoadName": "延安东路",
      "id": "09000300122101111452570287I",
      "x": 1352377000,
      "y": 364085900
    },
    "result": {"action": 0, "error": 0}
  }
  ```
- 応答JSON例（カバレッジ無し）: `{"result":{"action":0,"error":404}}`
- `content.x`, `content.y` は BD-09 MC × 100 スケール（`/100` で元のMC座標）

## 距離計算（緯度補正）

Mercator 投影はスケールが緯度依存なので、**`cos(lat)` で補正した地上メートル**で判定:

```rust
let pano_mc_x = content.x as f64 / 100.0;
let pano_mc_y = content.y as f64 / 100.0;
let dx_mc = query_mc_x - pano_mc_x;
let dy_mc = query_mc_y - pano_mc_y;
let mc_distance = (dx_mc * dx_mc + dy_mc * dy_mc).sqrt();
let ground_distance = mc_distance * lat.to_radians().cos();
if ground_distance > 10.0 {
    return Ok(None);  // 距離超過で不採用
}
```

上海 (lat 31°) で補正係数 ≈ 0.857、北京 (lat 40°) で ≈ 0.766。

## heading 計算（ランタイム）

BD-09 MC 空間で `atan2` によりコンパス方位（0=北, 時計回り）を計算:

```rust
let dx = junction_mc_x - pano_mc_x;
let dy = junction_mc_y - pano_mc_y;
let bearing = dx.atan2(dy).to_degrees();  // atan2(東成分, 北成分)
let heading = if bearing < 0.0 { bearing + 360.0 } else { bearing };
```

**atan2 を BD-09 MC 空間で直接使える理由**: MC は投影上の直交座標で、x軸が東、y軸が北。距離 10m 以内なら Mercator スケール誤差はほぼゼロで Haversine 計算と実質同じ結果。

## 百度街景URL形式

```
https://map.baidu.com/@<junction_mc_x>,<junction_mc_y>,21z#panotype=street&pid=<panoid>&panoid=<panoid>&heading=<heading>&pitch=0&l=21&tn=B_NORMAL_MAP&sc=0&newmap=1&shareurl=1
```

## 実装計画

### PR1: 機能実装

**依存関係の追加**（`backend/Cargo.toml`）:
```toml
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
```
既存 import が `#[tokio::main]` なので reqwest の async API がそのまま使える。

**新規ファイル**:

- `backend/src/domain/china.rs`
  - `is_in_china_mainland(lng: f64, lat: f64) -> bool`
  - `wgs84_to_gcj02(lng, lat) -> (f64, f64)`
  - `gcj02_to_bd09ll(lng, lat) -> (f64, f64)`
  - `bd09ll_to_bd09mc(lng, lat) -> (f64, f64)`
  - `wgs84_to_bd09mc(lng, lat) -> (f64, f64)`（コンビネータ）
  - `compute_baidu_heading(pano_mc_x, pano_mc_y, junction_mc_x, junction_mc_y) -> f64`
  - `baidu_panorama_url(panorama: &BaiduPanorama, junction: &Junction) -> String`（内部で座標変換と heading 計算を行う）
  - ユニットテスト: 変換精度、中国判定境界値、heading 4方位

- `backend/src/domain/china.rs` に共通型を定義
  - `BaiduPanorama { panoid: String, pano_mc_x: f64, pano_mc_y: f64 }` （HTTP応答パース結果と DB レコードの両方で使用する共通構造体）

- `backend/src/db/baidu_repository.rs`
  - `find_by_junction_ids(pool, ids: &[i64]) -> HashMap<i64, BaiduPanorama>`
    - SELECT: `SELECT id, baidu_panoid, baidu_pano_mc_x, baidu_pano_mc_y FROM y_junctions WHERE id = ANY($1) AND baidu_panoid IS NOT NULL`
  - `find_without_baidu_panoid(pool) -> Vec<Junction>` （中国本土判定はアプリ層）
  - `bulk_update_baidu(pool, updates: &[(i64, BaiduPanorama)]) -> usize`

- `backend/src/importer/baidu.rs`
  - `fetch_nearest_panorama(client, lng, lat) -> Result<Option<BaiduPanorama>>`
  - タイムアウト 5s、リトライ 1回、User-Agent 設定

- `backend/src/api/streetview_enricher.rs`
  - `enrich_collection(pool, junctions, total) -> Result<Value>`
  - `enrich_feature(pool, junction) -> Result<Value>`（単体用）
  - **内部 `build_url(j, baidu) -> String` が唯一の分岐点**
    ```rust
    fn build_url(j: &Junction, baidu: Option<&BaiduPanorama>) -> String {
        if china::is_in_china_mainland(j.lon, j.lat) {
            baidu
                .map(|b| china::baidu_panorama_url(b, j))
                .unwrap_or_default()
        } else {
            j.streetview_url()  // 既存の Google URL
        }
    }
    ```

- `backend/src/bin/import_baidu_panoid.rs`
  - CLI バイナリ（`import_elevation.rs` を雛形にコピー）
  - DB プール初期化 → `import_baidu_panoid_data(pool)` 呼び出し
  - オプション `--refresh` で既存 panoid も再取得

- `backend/migrations/007_add_baidu_panoid.sql`
  ```sql
  ALTER TABLE y_junctions
    ADD COLUMN baidu_panoid VARCHAR NULL,
    ADD COLUMN baidu_pano_mc_x DOUBLE PRECISION NULL,
    ADD COLUMN baidu_pano_mc_y DOUBLE PRECISION NULL;
  ```

**既存ファイルの変更（最小限）**:

- `backend/src/importer/mod.rs`
  - `pub mod baidu;` の 1行追加
  - `import_baidu_panoid_data(pool) -> Result<usize>` 関数を追加
    - 逐次 async ループで HTTP 問い合わせ、500ms sleep、距離フィルタ、bulk update

- `backend/src/api/handlers.rs`
  - `get_junctions`: `Junction::to_feature_collection(...)` を `streetview_enricher::enrich_collection(&pool, junctions, total_count).await?` に置換（1行）
  - `get_junction_by_id`: `junction.to_feature()` を `streetview_enricher::enrich_feature(&pool, junction).await?` に置換（1行）
  - `get_junction_by_osm_node_id`: 同上（1行）
  - 合計 3 行の差し替え（新規コードではなく既存行の置換）

- `frontend/src/components/JunctionPopup.tsx`
  - 条件付きレンダリング: `{streetview_url && <a href={streetview_url}>...</a>}`

**変更なし（拡張方式の恩恵）**:
- `backend/src/domain/junction.rs` （Junction 構造体、`streetview_url()`、`to_feature()`、全テスト）
- `backend/src/db/repository.rs` （JunctionRow、From 実装、5 SELECT 全て）
- `backend/src/bin/import.rs` / `import_two_way.rs`
- `backend/tests/api_tests.rs` （baidu_* カラムは NULLABLE なので既存 INSERT そのまま動く）
- `frontend/src/types/index.ts`
- `frontend/src/hooks/useJunctions.ts`

**テスト**:
- `china.rs` のユニットテスト（座標変換、中国判定境界値、heading 4方位、距離補正）
- `baidu.rs` のモックテスト（距離フィルタ、404ハンドリング、JSONパース）
- `baidu_repository.rs` のテスト（find_by_junction_ids、bulk_update）
- `streetview_enricher.rs` のテスト（中国/非中国それぞれで URL が正しく組まれる）
- 既存テスト全通過（特に `test_streetview_url` と API テスト一式）

**コミット前チェック**（CLAUDE.md 準拠）:
```bash
cargo test --manifest-path backend/Cargo.toml
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml -- -D warnings
cd frontend && npm test && npm run typecheck && npm run format:check && npm run lint
```

### PR2: データ投入

1. `china-latest.osm.pbf` を Geofabrik からダウンロード → `~/y-junctions-data/osm/`
2. ローカルDB に上海中心部 bbox でインポート:
   ```bash
   (cd backend && ./target/release/import \
     --input ~/y-junctions-data/osm/china-latest.osm.pbf \
     --bbox 121.4,31.15,121.55,31.30)
   (cd backend && ./target/release/import_two_way \
     --input ~/y-junctions-data/osm/china-latest.osm.pbf \
     --bbox 121.4,31.15,121.55,31.30)
   ```
3. panoid 取得バッチ実行:
   ```bash
   (cd backend && ./target/release/import_baidu_panoid)
   ```
4. インポートログから記録:
   - Y字路件数 / panoid 成功 / 距離超過 / 404 / エラー
5. サンプル数件を日本からブラウザで開いて動作確認
6. ヒット率を見て続行判断（後述「成功基準」）
7. `/deploy-data` で本番反映
8. `doc/data-updates.md` 更新
9. PR作成: `data/china-shanghai`

## 再インポート時の挙動

- OSM PBF 再インポート: `ON CONFLICT (osm_node_id) DO NOTHING` により既存 Y字路の baidu_* カラムは維持
- `import_baidu_panoid`: デフォルトで `baidu_panoid IS NULL` のみ対象（レジューム性）
- 経年劣化で panoid が古くなった場合は `--refresh` フラグで全中国Y字路を再取得

## パフォーマンス影響

拡張方式では API リクエストごとに DB クエリが **1 → 2 回**に増える:
1. `find_by_bbox`: 既存（変更なし）
2. `baidu_repository::find_by_junction_ids`: 新規

2 つ目のクエリは主キー `id` 配列での SELECT なのでインデックスで高速。実測で数 ms 追加見込み。既存 bbox 検索（数十 ms）との比率で誤差レベル。

## 将来のストラテジーパターン化

現在の `build_url` の if/else は、将来的にストラテジーパターンに置換可能:
```rust
trait StreetViewProvider {
    fn supports(&self, j: &Junction) -> bool;
    fn url(&self, j: &Junction, baidu: Option<&BaiduInfo>) -> Option<String>;
}
```
置換は `build_url` 内部のみで、他モジュールへの波及なし。**今は不要**、必要になったら導入。

## リスクと緩和策

| リスク | 緩和策 |
|---|---|
| 非公式エンドポイント `mapsv0.bdimg.com` 閉鎖・仕様変更 | 失敗時は panoid なしで続行。影響は中国 SV のみ |
| 百度からのレート制限 / BAN | 500ms sleep、User-Agent ブラウザ値、リトライ1回 |
| 場所ズレ（panorama が Y字路から離れた地点） | 10m 距離フィルタ（cos(lat) 補正） + heading を Y字路方向に |
| panoid の経年劣化 | `--refresh` オプションで明示的再取得 |
| OSM中国データのカバレッジが日本より薄い | パイロットで実測、取得件数で判断 |
| 日本からのネットワーク経路不安定 | ユーザ環境依存のため対応外 |
| フロントで空文字 streetview_url 表示 | 条件付きレンダリング `{streetview_url && <a>}` |

## 成功基準

事前の数値目標は設けず、以下を**実測・手動検証**で評価:

- パイロット（上海中心部）で Y字路が抽出できる
- panoid 取得の成功/失敗の内訳が統計として取れる
- 実ヒット率を見て PR2 をマージするか継続検討するかを判断
- 成功分のサンプル数件で日本からブラウザで開き、Y字路の様子が実際に確認できる
- 既存の日本データ側のストリートビュー動作に回帰が無い（全テスト通過 + 実環境で `streetview_url` が従来通り出る）

## 将来的な拡張（スコープ外）

- 天地图 (CGCS2000) タイル切替オプション（中国からのアクセス速度対策）
- Mapillary との併用（百度でカバーされない地点のフォールバック）
- 非公式エンドポイント廃止時の代替実装（百度公式 JS API + APIキー方式）
- ストラテジーパターンへのリファクタ（プロバイダ追加時）
- 他の中国都市への展開（北京・深圳・成都など）
