# Y字路検索サービス 開発タスクリスト

## 並行開発戦略

- 3つの領域（Import, Backend, Frontend）で並行開発可能
- 各Phase = 1 PR
- Phase内の全タスクを完了させてからPR作成

---

## 🗄️ Import (データインポート)

### Phase 1: CLIツール基盤とPBFパーサー骨格 ✅

**ゴール**: コマンドラインツールとPBFファイル読み込みの基盤を構築

**成果物**:
- `backend/Cargo.toml` - importバイナリ定義追加
- `backend/src/lib.rs` - importerモジュール公開
- `backend/src/bin/import.rs` - CLI実装
- `backend/src/importer/mod.rs` - モジュール定義
- `backend/src/importer/parser.rs` - PBF読み込み骨格

**タスク**:
- [x] Cargo.tomlに`[[bin]]` import追加
- [x] clap でCLI引数パース実装（--input, --bbox）
- [x] osmpbfクレートでPBFファイルオープン確認
- [x] エラーハンドリング基盤（anyhow）

**完了条件**:
- ✅ `cargo run --bin import -- --help` でヘルプ表示
- ✅ `cargo run --bin import -- --input test.pbf --bbox 139,35,140,36` でファイル読み込み開始（まだ処理なし）

---

### Phase 2: Y字路検出ロジック ✅

**ゴール**: OSMデータからY字路候補（3本道路が接続するNode）を抽出

**成果物**:
- `backend/src/importer/parser.rs` - 2パス処理実装
- `backend/src/importer/detector.rs` - Y字路判定ロジック

**タスク**:
- [x] 1st pass: highway付きWayのNode IDをHashSetに収集
- [x] 各NodeのWay接続数をHashMapでカウント
- [x] 接続数==3のNodeをフィルタリング
- [x] highwayタイプチェック（residential, tertiary等）
- [x] 2nd pass: 該当NodeとWayの座標を取得（DenseNode対応含む）

**完了条件**:
- ✅ テスト用PBFでY字路候補が抽出される（四国PBFで61,679個、香川県エリアで19,785個）
- ✅ ログに「Found X Y-junction candidates」と出力

**実装メモ**:
- DenseNodes形式のNode読み取りに対応（`Element::DenseNode`の処理を追加）
- GeofabrikのPBFファイルはデフォルトでDenseNodes形式を使用

---

### Phase 3: 角度計算とデータモデル ✅

**ゴール**: Y字路の3つの角度を計算し、タイプ分類

**成果物**:
- `backend/src/importer/calculator.rs` - 角度計算
- `backend/src/domain/junction.rs` - Junctionモデル（importで使用）

**タスク**:
- [x] geo crateで方位角（bearing）計算
- [x] 3方向の角度を算出・昇順ソート
- [x] angle_type分類ロジック（verysharp/sharp/normal）
- [x] Junction構造体定義（osm_node_id, location, angles, road_types）

**完了条件**:
- ✅ Y字路候補に対して角度計算完了
- ✅ ログに各Y字路の角度情報出力（例: `Node 123: [45°, 135°, 180°] type=sharp`）

---

### Phase 4: データベース投入 ✅

**ゴール**: 計算済みY字路データをPostgreSQLへバルクインサート

**成果物**:
- `backend/src/importer/inserter.rs` - バルクインサート処理

**タスク**:
- [x] sqlxでPostgreSQL接続
- [x] トランザクション開始
- [x] バッチ挿入（1000件ずつ等）
- [x] 進捗表示（`Inserted 1000/5000...`）
- [x] エラー時ロールバック

**完了条件**:
- ✅ 小規模PBFファイルでエンドツーエンド動作（shikoku-latest.pbf、高松市周辺で確認）
- ✅ データベースにy_junctionsレコードが挿入される（1,968件挿入成功）
- ⚠️ `cargo run --bin import -- --input kanto-latest.pbf --bbox 138.9,35.5,140.0,35.9` が成功（四国PBFで代用確認、動作は検証済み）

**依存**: Backend Phase 1完了（DBスキーマ必要）

**実装メモ**:
- バルクインサート: 1つのINSERT文で最大1000件を挿入
- トランザクション処理でエラー時の自動ロールバック実装
- 処理時間: 約52秒（1,968件）
- 成功率: 100%
- Phase 5として road_types順序の課題を追加

---

### Phase 5: road_types順序の検討と修正（オプショナル） ✅

**ゴール**: road_typesの順序が角度に対応するかを検討し、必要なら実装修正

**背景**:
- 現在の実装では`road_types`はHashSetから取得しているため順序が不定
- PostgreSQLの配列型は順序を保持するが、挿入前の段階で順序が保証されていない
- `road_types[i]`が`angle_i`に対応する道路タイプを表すべきかを検討する必要がある

**成果物**:
- バックエンドから`road_types`を完全削除

**タスク**:
- [x] 仕様書を確認し、road_typesと角度の対応関係が必要かを判断
- [x] road_typesの必要性を検討
- [x] バックエンドからroad_typesを削除

**完了条件**:
- ✅ road_typesの必要性が明確になった（MVPとして不要）
- ✅ バックエンドからroad_typesが完全削除
- ✅ ユニットテスト成功（18テスト全てパス）
- ✅ 統合テスト成功（14テスト全てパス）

**依存**: Import Phase 4完了

**検討結果と実装**:
- 検討の結果、road_typesはMVPとして不要と判断
- UIで表示しても検索・フィルタリングできず、実用性が低い
- バックエンドから完全削除を実施：
  - DBマイグレーション（`road_types TEXT[]`列削除）
  - ドメインモデル（`Junction`構造体）
  - インポート処理（`detector.rs`, `parser.rs`, `inserter.rs`）
  - リポジトリ層（`repository.rs`）
- フロントエンドは未修正（APIから返らないため自然に非表示になる）

---

## 🔌 Backend (API)

### Phase 1: データベース環境とスキーマ

**ゴール**: PostgreSQL + PostGIS環境とテーブル定義を完成

**成果物**:
- `docker-compose.yml` - PostgreSQL + PostGIS設定
- `backend/migrations/001_create_y_junctions.sql` - テーブル定義
- `.env.example` - 環境変数テンプレート

**タスク**:
- [x] docker-compose.yml作成（postgis/postgis:16-3.4）
- [x] マイグレーションSQL作成
  - PostGIS拡張有効化
  - y_junctionsテーブル（シンプル設計: GENERATED列削除）
  - GIST/BTREEインデックス
- [x] sqlx-cli導入とマイグレーション実行手順
- [x] .envファイル設定

**完了条件**:
- [x] `docker-compose up -d` でPostgreSQL起動
- [x] `sqlx migrate run` でテーブル作成成功
- [x] `psql` で `\d y_junctions` が表示される

**実装メモ**:
- angle_type, min_angleのGENERATED列を削除（一般的な設計を採用）
- angle_1に直接インデックス作成
- 分類ロジックは必要に応じてアプリケーション層で実装

---

### Phase 2: ドメインモデルとリポジトリ層 ✅

**ゴール**: Junction型とデータアクセスロジックを実装

**成果物**:
- `backend/src/domain/mod.rs`
- `backend/src/domain/junction.rs` - ドメインモデル
- `backend/src/db/mod.rs` - DB接続プール
- `backend/src/db/repository.rs` - リポジトリ実装

**タスク**:
- [x] Junction構造体とAngleType enum
- [x] GeoJSON変換実装（`to_feature()`, `to_feature_collection()`）
- [x] sqlxでDB接続プール作成
- [x] `find_by_bbox()` 実装（bbox + フィルタ対応）
- [x] `find_by_id()` 実装
- [x] `count_by_type()` 実装
- [x] ユニットテスト追加（9テスト、全て合格）

**完了条件**:
- [x] ユニットテストでリポジトリメソッド動作確認
- [x] モックデータで各メソッドが正しいSQLを実行

**実装メモ**:
- QueryBuilderをヘルパー関数に分離して可読性を向上
- angle_typeの分類ロジックはアプリケーション層で実装（DB側のGENERATED列なし）
- chrono crateを追加してDateTime型をサポート

---

### Phase 3: API エンドポイント実装 ✅

**ゴール**: REST APIの3つのエンドポイントを完成

**成果物**:
- `backend/src/api/mod.rs`
- `backend/src/api/handlers.rs` - ハンドラー関数
- `backend/src/api/routes.rs` - ルーティング
- `backend/src/main.rs` - 更新（APIマウント）

**タスク**:
- [x] `GET /api/junctions` ハンドラー
  - クエリパラメータ解析（bbox, angle_type, min_angle_lt/gt, limit）
  - バリデーション
  - GeoJSON FeatureCollection レスポンス
  - Street View URL生成
- [x] `GET /api/junctions/:id` ハンドラー
- [x] `GET /api/stats` ハンドラー
- [x] CORS設定（tower-http）
- [x] エラーレスポンス（JSON形式）

**完了条件**:
- ✅ `curl http://localhost:8080/api/junctions?bbox=139,35,140,36` でGeoJSON取得
- ✅ `curl http://localhost:8080/api/junctions/1` で単一データ取得
- ✅ `curl http://localhost:8080/api/stats` で統計情報取得

**依存**: Phase 2完了

**実装メモ**:
- 一般的なRust/Axumの作法に従った構成（handlers, routes, main分離）
- ハンドラー層にパラメータ型とエラー型を配置（小規模プロジェクト向け）
- Service層なし（Handler → Repository直接呼び出し）
- PgPoolをStateとして直接使用（AppState構造体不要）
- CORS設定でフロントエンド連携準備完了

---

### Phase 4: API テスト実装 ✅

**ゴール**: APIエンドポイントの自動テストを実装し、品質を保証

**成果物**:
- `backend/tests/api_tests.rs` - 統合テスト
- または `backend/src/api/handlers.rs` - ユニットテスト追加

**タスク**:
- [x] テストヘルパー実装（テスト用DBセットアップ等）
- [x] `GET /api/junctions` のテスト
  - 正常系: bbox指定でデータ取得
  - 正常系: フィルタ（angle_type, min_angle）動作確認
  - 異常系: 不正なbbox（バリデーションエラー）
  - 異常系: 範囲外のbbox
- [x] `GET /api/junctions/:id` のテスト
  - 正常系: 存在するIDでデータ取得
  - 異常系: 存在しないIDで404
- [x] `GET /api/stats` のテスト
  - 正常系: 統計情報取得
  - データあり/なしで正しいレスポンス
- [x] エラーレスポンスのテスト
  - ステータスコード確認
  - JSONフォーマット確認

**完了条件**:
- ✅ `cargo test` で全テストが合格（14個の統合テスト実装済み）
- ✅ 各エンドポイントの正常系・異常系をカバー
- ✅ テストカバレッジが十分（主要パスをカバー）

**依存**: Phase 3完了

---

### Phase 5: Street View URL修正 ✅

**ゴール**: Google Maps Street View URLを正しい形式に修正

**成果物**:
- `backend/src/domain/junction.rs` - streetview_url()メソッド修正

**タスク**:
- [x] streetview_url()を新しいAPI形式に変更
  - 現在: `https://www.google.com/maps/@{lat},{lon},3a,75y,{heading}h,90t`
  - 修正後: `https://www.google.com/maps/@?api=1&map_action=pano&viewpoint={lat},{lon}`
- [x] テストの更新（streetview_urlのURL形式チェック）

**完了条件**:
- ✅ Street View URLが新しいAPI形式に変更された
- ✅ `test_streetview_url`テストが合格
- ✅ 全ユニットテスト（18個）が合格

**理由**:
- 現在の実装では古いURL形式を使用しており、Street Viewが正しく表示されない
- Frontend Phase 4で発見された問題

**実装メモ**:
- Google Maps URLs API公式ドキュメントに基づいた形式に変更
- 必須パラメータのみ使用（api=1, map_action=pano, viewpoint）
- オプションパラメータ（heading, pitch, fov）は省略（必要に応じて後で追加可能）

---

## 🎨 Frontend (UI)

### Phase 1: 型定義とAPIクライアント

**ゴール**: TypeScript型定義とバックエンドAPI通信基盤を構築

**成果物**:
- `frontend/src/types/index.ts` - 型定義
- `frontend/src/api/client.ts` - APIクライアント

**タスク**:
- [x] Junction型、AngleType型、GeoJSON型定義
- [x] FilterParams型、AppState型定義
- [x] `fetchJunctions(bbox, filters)` 実装
- [x] `fetchJunctionById(id)` 実装
- [x] `fetchStats()` 実装
- [x] エラーハンドリング（try-catch + Error型）

**完了条件**:
- TypeScriptコンパイルエラーなし
- モック応答でAPIクライアント動作確認

---

### Phase 2: 地図コンポーネントとマーカー表示 ✅

**ゴール**: Leaflet地図上にY字路マーカーを表示

**成果物**:
- `frontend/src/components/MapView.tsx` - 地図コンポーネント
- `frontend/src/hooks/useJunctions.ts` - データ取得フック
- `frontend/src/App.tsx` - 更新（MapView統合）

**タスク**:
- [x] react-leafletでベース地図表示
- [x] OpenStreetMapタイル設定
- [x] 初期位置: 東京駅 (139.7671, 35.6812), zoom 14
- [x] bounds変更イベントでAPI再取得（デバウンス300ms）
- [x] useJunctionsフック実装（ローディング状態管理）
- [x] Marker表示（angle_typeで色分け）

**完了条件**:
- ✅ `npm run dev` で地図表示
- ✅ 地図移動時にコンソールで新しいbbox確認
- ✅ マーカーが表示される（バックエンドが起動していれば実データ、なければモック）

**依存**: Backend Phase 3推奨（API実装済みだと実データでテスト可）

**実装メモ**:
- モックデータサポート（useMockDataオプション）でバックエンド実装前でもテスト可能
- エラー時の自動フォールバック機能（useJunctionsフック内で処理）
- ローディング・エラー・件数表示などのUI要素はPhase 3で実装予定

---

### Phase 3: フィルターパネルとポップアップ ✅

**ゴール**: フィルタリング機能とY字路詳細表示

**成果物**:
- `frontend/src/components/FilterPanel.tsx` - フィルタUI
- `frontend/src/components/JunctionPopup.tsx` - ポップアップ
- `frontend/src/components/StatsDisplay.tsx` - 統計表示
- `frontend/src/hooks/useFilters.ts` - フィルタ状態管理

**タスク**:
- [x] FilterPanel実装
  - angle_typeチェックボックス（verysharp/sharp/normal）
  - min_angleスライダー（0-60°）
  - フィルタリセットボタン
- [x] JunctionPopup実装
  - 角度表示
  - 道路タイプ表示
  - Street Viewリンク
- [x] StatsDisplay実装（検索結果件数）
- [x] useFiltersフック（フィルタ変更時API再取得）
- [x] 型定義修正（angle_type配列対応）
- [x] APIクライアント修正（複数angle_typeのクエリパラメータ送信）

**完了条件**:
- ✅ フィルタ変更でマーカーが絞り込まれる（バックエンドAPI連携時）
- ✅ マーカークリックでポップアップ表示
- ✅ 検索結果件数が表示される

**依存**: Phase 2完了

**実装メモ**:
- バックエンドAPIは複数のangle_typeを配列で受け取る仕様（Vec<AngleType>）
- フロントエンドはangle_type配列をクエリパラメータとして送信（例: ?angle_type=sharp&angle_type=even）
- 左サイドバー（フィルタ + 統計）+ 右側地図のレイアウト実装済み
- 動作確認は実APIで行う（App.tsx で useMockData={false} に変更）

---

### Phase 4: スタイリングと最適化 ✅

**ゴール**: UIデザイン完成とパフォーマンス向上

**成果物**:
- `frontend/src/App.css` - スタイル
- 最適化されたコンポーネント

**タスク**:
- [x] レイアウト実装（左サイドバー + 右地図）
- [x] レスポンシブ対応
- [x] React.memo適用（MapView, FilterPanel, StatsDisplay）
- [x] useMemo/useCallback最適化
- [x] 大量マーカー対策（1000件以上で警告表示等）

**完了条件**:
- ✅ デザインが仕様書のワイヤーフレームに近い
- ✅ 500件マーカーでも滑らかに動作
- ✅ モバイル表示で崩れない

**依存**: Phase 3完了

**実装メモ**:
- App.cssでスタイルを整理（インラインスタイルから移行）
- モバイルでサイドバーをトグル可能に実装
- 角度タイプラベルを直感的な名前に変更（超鋭角、鋭角、広角）
- マーカー色を最小角度でグラデーション化（青→水色→黄緑→赤）
- 最小角度フィルターをレンジスライダーに改善
- Street View URLを新しいAPI形式に修正
- CodeRabbitレビュー対応（React.memo最適化、アクセシビリティ改善）

---

### Phase 5: road_typesの削除 ✅

**ゴール**: フロントエンドからroad_types（道路タイプ）の表示・処理を削除

**背景**:
- 現在の実装ではJunctionPopupでroad_typesを表示しているが、この情報が不要であることが判明
- データの簡素化とUIの見やすさを向上させるため、road_types関連の機能を削除する

**成果物**:
- `frontend/src/types/index.ts` - 型定義修正
- `frontend/src/components/JunctionPopup.tsx` - road_types表示削除
- `frontend/src/hooks/useJunctions.ts` - モックデータ修正

**タスク**:
- [x] Junction型からroad_typesフィールドを削除
- [x] JunctionPopupコンポーネントからroad_types表示を削除
- [x] 関連するCSSスタイルの削除（該当する場合）
- [x] 型エラーがないことを確認（`npm run typecheck`）

**完了条件**:
- ✅ Junction型にroad_typesが含まれていない（types/index.ts:14, 28）
- ✅ ポップアップにroad_typesが表示されない（JunctionPopup.tsx:32-42削除済み）
- ✅ TypeScriptの型チェックが通る（`npm run typecheck`成功）
- ⚠️ 実際にブラウザで表示して動作確認（次のステップ）

**実装メモ**:
- JunctionとJunctionProperties両方の型定義からroad_typesを削除
- JunctionPopupから道路タイプセクション全体を削除
- useJunctionsフックのモックデータからもroad_typesを削除（3箇所）
- CSS削除は不要（インラインスタイルのみ使用）

---

### Phase 6: 直線分岐（Skewed）の削除 ✅

**ゴール**: フィルターから直線分岐（Skewed）タイプを削除し、シンプルな3分類に変更

**背景**:
- 直線分岐（`angle_3 > 200`）はわかりにくい
- 需要があるか不明
- UIをシンプルにするため削除

**成果物**:
- `backend/src/domain/junction.rs` - AngleType enumから`Skewed`を削除
- `backend/src/api/handlers.rs` - パース処理から削除
- `backend/src/db/repository.rs` - クエリロジックから削除
- `frontend/src/types/index.ts` - AngleType型から削除
- `frontend/src/components/FilterPanel.tsx` - UIから削除
- `frontend/src/components/MapView.tsx` - マーカー色から削除
- `frontend/src/components/JunctionPopup.tsx` - ラベルから削除
- `frontend/src/hooks/useFilters.ts` - デフォルト値から削除
- `frontend/src/hooks/useJunctions.ts` - モックデータを修正
- `doc/mvp-requirements.md` - ドキュメント更新
- `doc/todo.md` - ドキュメント更新

**タスク**:
- [x] Backend: AngleType enumから`Skewed`を削除し、分類ロジックを修正
- [x] Backend: ユニットテストを修正
- [x] Backend: API handlers、repositoryから`skewed`の参照を削除
- [x] Backend: cargo test、cargo fmt、cargo clippyを実行
- [x] Frontend: AngleType型から`'skewed'`を削除
- [x] Frontend: FilterPanel、MapView、JunctionPopupから直線分岐のUIを削除
- [x] Frontend: useFilters、useJunctionsから`skewed`の参照を削除
- [x] Frontend: npm run typecheck、lint、formatを実行
- [x] Docs: mvp-requirements.mdを現在の実装に合わせて更新
- [x] Docs: todo.mdを更新

**完了条件**:
- ✅ Backend: AngleTypeが3つ（verysharp/sharp/normal）になった
- ✅ Backend: 全ユニットテスト（17個）が成功
- ✅ Backend: cargo fmt、clippyチェックが成功
- ✅ Frontend: フィルターパネルに3つのチェックボックスのみ表示
- ✅ Frontend: 型チェック、Lint、フォーマットが成功
- ✅ Docs: 全ドキュメントが更新され、skewedの参照が0件

**新しい分類ロジック**:
- **verysharp**（超鋭角）: `angle_1 < 30°`
- **sharp**（鋭角）: `30° ≤ angle_1 < 45°`
- **normal**（広角）: `angle_1 ≥ 45°`

**実装メモ**:
- 以前のSkewed（`angle_3 > 200`）は最小角度に基づいて再分類される
- `angle_3 > 200`の条件をSQLとRustコードから削除
- フィルターのロジックを4→3に変更（`angleTypes.length < 4` → `angleTypes.length < 3`）
- コードベース全体でskewedの参照が完全に削除された

---

## 🛠️ 改善タスク

### Street View方向修正 ✅

**ゴール**: OSMデータから道路の実際の方位角を計算・保存し、正確な方向でStreet Viewを開く

**背景**:
現在のStreet View URLには方向（heading）パラメータがなく、開いた際にY字路の方向を向いていないことが多い。現在のデータには道路の絶対的な方位角情報がないため、角度データ（angle_1, angle_2, angle_3）からは推定できない。OSMの道路データ（Way）からY字路に接続する各道路の方位角（bearing）を計算する必要がある。

**アプローチ**:
1. インポート時に各道路の方位角を計算
   - Y字路ノードから次のノードへの方位角を計算（geo crateのbearing関数使用）
   - 3本の道路それぞれのbearingを保存
2. データベースに方位角データを追加
   - `bearings REAL[3]` 列追加（北を0度として0-360度）
3. Street View URL生成時に適切なheadingを計算
   - 最も鋭角な道路の方向、または3方向の中間方向など

**メリット**:
- 正確な方位角データに基づく、信頼性の高い方向設定
- 将来的に「特定の道路方向から見る」などの機能拡張が可能
- データとして再利用可能

**必要な作業**:
- データベーススキーマ変更（マイグレーション）
- 既存データの再インポート
- フロントエンド・バックエンドの修正

**成果物**:
- `backend/migrations/002_add_bearings.sql` - マイグレーション
- `backend/src/domain/junction.rs` - bearingsフィールド追加
- `backend/src/importer/calculator.rs` - bearing計算実装
- `backend/src/importer/detector.rs` - JunctionForInsert構造体にbearings追加
- `backend/src/importer/parser.rs` - bearing計算呼び出し実装
- `backend/src/importer/inserter.rs` - bearings保存
- `backend/src/db/repository.rs` - bearings読み込み
- `frontend/src/types/index.ts` - bearingsフィールド追加
- `frontend/src/hooks/useJunctions.ts` - モックデータにbearings追加
- API修正（bearingsをレスポンスに含める）

**タスク**:
- [x] データベーススキーマ変更
  - bearings列追加（REAL[3] NOT NULL）
- [x] インポート処理修正
  - Y字路の各道路について、次のノード座標を取得
  - geo::Point::bearing()で方位角計算（3方向）
  - bearings配列を時計回り順で保存（ソートしない）
  - 角度とbearingsの対応関係を維持
- [x] ドメインモデル修正
  - Junction構造体にbearings: Vec<f32>フィールド追加
  - streetview_url()でheading計算ロジック実装
  - angle_type()でローカルソート（データはソートしない）
- [x] API修正
  - レスポンスにbearingsを含める
  - GeoJSON propertiesにbearings追加
- [x] フロントエンド型定義修正
  - JunctionProperties型にbearings: number[]追加
- [x] テスト修正
  - バックエンドユニットテスト（18テスト）
  - バックエンド統合テスト（14テスト）
  - フロントエンド型チェックとビルド
- [x] 動作確認
  - 全テスト合格を確認

**完了条件**:
- ✅ データベースにbearingsデータが保存される（REAL[3] NOT NULL列追加）
- ✅ Street View URLに正確なheadingパラメータが含まれる
- ✅ 全テストが合格（バックエンド32テスト、フロントエンドビルド成功）

**工数**: 中（1日程度）

**依存**: Import Phase 4完了

**実装メモ**:
- **角度とbearingsの対応関係を維持**:
  - 角度をソートせず、時計回り順で保存
  - angle_1は bearings[0] と bearings[1] の間
  - angle_2は bearings[1] と bearings[2] の間
  - angle_3は bearings[2] と bearings[0] の間
- **heading計算ロジック**:
  - 最小角度を見つける
  - その角度を形成する2つのbearingを特定
  - 2つのbearingの平均を計算（360度のまたぎに対応）
- **データ型**:
  - データベース: `REAL[3]` (PostgreSQL配列)
  - Rust: `Vec<f32>`
  - TypeScript: `number[]`
- **テスト対応**:
  - 全てのJunctionテストデータにbearings追加
  - INSERT文にARRAY[$7, $8, $9]構文使用
- **既存データ**: 全削除して再インポート
- **マイグレーション統合**:
  - 002_add_bearings.sqlに角度制約の更新を統合（angle_1, angle_2も360度まで許容）
  - 003_update_angle_constraints.sqlは削除（002に統合したため不要）
- **デプロイ確認**:
  - 香川県エリア（bbox: 133.5,34.0,134.5,34.5）で3257件のY字路をインポート成功
  - バックエンド（http://localhost:8080）とフロントエンド（http://localhost:3001）起動確認
  - Street View URLにheadingパラメータが含まれることを確認

---

### セキュリティ脆弱性の対応

**ゴール**: cargo auditで検出された脆弱性を修正し、CI/CDで脆弱性検出を有効化

**背景**:
現在のワークフローでは `cargo audit || true` により脆弱性が見つかってもCIが成功している。2つの脆弱性と2つの警告が検出されている：

**検出された問題**:
1. **sqlx 0.7.4** - Binary Protocol Misinterpretation (RUSTSEC-2024-0363)
   - 修正版: 0.8.1以上にアップグレード可能
   - 優先度: High
2. **rsa 0.9.9** - Marvin Attack / タイミングサイドチャネル攻撃 (RUSTSEC-2023-0071)
   - 深刻度: 5.9 (medium)
   - 修正版: なし（sqlx-mysql経由の推移的依存）
   - 優先度: Medium
3. **paste 1.0.15** - メンテナンスされていない (RUSTSEC-2024-0436)
   - sqlx-core経由で使用
   - 優先度: Low
4. **rustls-pemfile 1.0.4** - メンテナンスされていない (RUSTSEC-2025-0134)
   - sqlx-core経由で使用
   - 優先度: Low

**成果物**:
- `backend/Cargo.toml` - sqlxバージョンアップ
- `.github/workflows/deploy.yml` - `|| true` 削除

**タスク**:
- [ ] sqlxを0.8.1以上にアップグレード
  - Cargo.tomlの依存関係を更新
  - マイグレーション手順の確認（0.7→0.8で破壊的変更がある可能性）
  - 全テストが通ることを確認
- [ ] paste, rustls-pemfileの問題が解決されたか確認
  - sqlx 0.8.xで依存関係が更新されている可能性
- [ ] rsa脆弱性の影響範囲を評価
  - MySQL接続を使用していない場合は影響なし
  - 必要に応じて対応策を検討
- [ ] ワークフローから `|| true` を削除
  - `.github/workflows/deploy.yml` の105行目を修正
  - 今後の脆弱性を自動検出できるようにする

**完了条件**:
- sqlxが0.8.1以上にアップグレードされている
- cargo auditで Critical/High の脆弱性が0件
- CI/CDで脆弱性が検出された場合、ビルドが失敗する
- 全テスト（ユニット、統合）が成功

**工数**: 中（半日～1日程度）

**参考**:
- [RUSTSEC-2024-0363](https://rustsec.org/advisories/RUSTSEC-2024-0363)
- [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071)
- sqlx 0.8 migration guide（該当する場合）

---

## 🔗 統合・テスト・デプロイ

### Phase: エンドツーエンド動作確認

**ゴール**: 全コンポーネント統合して実データで動作確認

**タスク**:
- [ ] docker-compose up でDB起動
- [ ] 小規模PBFファイル（渋谷区等）でインポート実行
- [ ] Backend起動、API動作確認
- [ ] Frontend起動、地図にデータ表示確認
- [ ] フィルタリング動作確認
- [ ] 関東latest.pbf（東京都範囲）で本番データ投入
- [ ] データ品質チェック（件数、分布確認）

**完了条件**:
- 東京都内のY字路が地図上に表示される
- フィルタとポップアップが正常動作
- パフォーマンスが許容範囲内

---

## ⚙️ ワークフロー

### ✅ Phase 1: lint-staged修正（monorepo対応）

**ゴール**: pre-commit hookが確実にエラーを検出するように修正

**問題**:
現在の`.lintstagedrc.js`は`cd frontend &&`コマンドを使用しているが、lint-stagedはシェルコマンドとして実行せず、`cd`をコマンド名、`frontend`、`&&`、`npm`等を引数としてパースする。このため、エラーが発生してもタスクが[COMPLETED]となり、エラーが検出されない（false positive）。

**影響**:
- pre-commit hookが通過してもCIで失敗する
- Prettierによるフォーマット済みファイルがコミットされない
- ESLintエラーが見逃される

**解決策**: Option A（推奨） - サブディレクトリごとに`.lintstagedrc.js`を配置

**なぜこの方法を選択するか**:
1. Git hooksは常にリポジトリルートから実行される（どこでcommitしても同じ動作）
2. `cd frontend`をシェルスクリプト内で実行すれば確実に動作する
3. 各ディレクトリの`.lintstagedrc.js`はシンプルな相対パス指定のみで済む
4. エラーコードが正しく伝播する

**成果物**:
- `frontend/.lintstagedrc.js` - フロントエンド用lint-staged設定（新規）
- `backend/.lintstagedrc.js` - バックエンド用lint-staged設定（新規）
- `.husky/pre-commit` - 更新（サブディレクトリでlint-staged実行）
- `.lintstagedrc.js` - 削除または空にする

**タスク**:
- [x] `frontend/.lintstagedrc.js`を作成
  ```javascript
  export default {
    '**/*.{ts,tsx}': (filenames) => [
      'npm run typecheck',
      `eslint --fix ${filenames.join(' ')}`,
      `prettier --write ${filenames.join(' ')}`,
    ],
    '**/*.css': (filenames) => [
      `prettier --write ${filenames.join(' ')}`,
    ],
  };
  ```
- [x] `backend/.lintstagedrc.js`を作成
  ```javascript
  export default {
    '**/*.rs': (filenames) => [
      `cargo fmt -- ${filenames.join(' ')}`,
      'cargo clippy --all-targets --all-features -- -D warnings',
    ],
  };
  ```
- [x] `.husky/pre-commit`を更新
  ```bash
  #!/bin/sh
  set -e

  # Check which directories have changes
  FRONTEND_CHANGED=$(git diff --cached --name-only | grep "^frontend/" || true)
  BACKEND_CHANGED=$(git diff --cached --name-only | grep "^backend/" || true)

  # Run lint-staged in subdirectories
  if [ -n "$FRONTEND_CHANGED" ]; then
    echo "Running lint-staged in frontend..."
    (cd frontend && npx lint-staged) || exit $?
  fi

  if [ -n "$BACKEND_CHANGED" ]; then
    echo "Running lint-staged in backend..."
    (cd backend && npx lint-staged) || exit $?
  fi
  ```
- [x] ルートの`.lintstagedrc.js`を削除
- [x] テスト: 意図的なESLintエラーでpre-commit hookが失敗することを確認

**実装メモ**:
- テスト実行をpre-commitから削除（frontend, backend両方）
- テストはCI（GitHub Actions）でのみ実行
- commitを高速化し、ローカル環境でのDB不要に

**テスト手順**:
1. フロントエンドファイルに意図的なESLintエラーを追加
2. `git add`して`git commit`を実行
3. pre-commit hookがエラーを検出してコミットが失敗することを確認
4. エラーを修正して再度コミット
5. コミットが成功することを確認
6. Prettierで整形されたファイルが自動でaddされることを確認

**完了条件**:
- ✅ pre-commit hookがESLintエラーを確実に検出する
- ✅ pre-commit hookが型エラーを確実に検出する
- ✅ Prettierで整形されたファイルが確実にコミットされる
- ✅ CI（GitHub Actions）とローカルpre-commit hookの結果が一致する
- ✅ リポジトリルート、frontend、backendどのディレクトリからcommitしても同じ動作をする

**参考**:
- lint-stagedはシェルコマンドを実行しない（`cd && command`が動作しない）
- Git hooksは常にリポジトリルートから実行される
- `git diff --cached --name-only`は常にリポジトリルートからの相対パスを返す

---

### Phase 2: Worktree自動セットアップ

**ゴール**: 新しいworktreeでnpm installを自動実行する仕組みを実装

**問題**:
新しいgit worktreeを作成した際、`node_modules`がインストールされていないため、pre-commit hookが動作しない。手動で`npm install`を実行する必要があるが、忘れる可能性がある。

**影響**:
- Pre-commit hookが`npx: command not found`エラーで失敗する
- lint-stagedが実行されず、コード品質チェックがスキップされる
- 開発者が手動セットアップを忘れる可能性

**解決策**: pre-commitで依存関係を自動検出・インストール

**成果物**:
- `.husky/pre-commit` - 自動検出・インストール機能追加

**タスク**:
- [ ] `.husky/pre-commit`に依存関係チェックを追加
  ```bash
  # Check if dependencies are installed
  if [ ! -d "node_modules" ]; then
    echo "📦 Installing root dependencies..."
    npm install || exit 1
  fi

  if [ ! -d "frontend/node_modules" ]; then
    echo "📦 Installing frontend dependencies..."
    (cd frontend && npm install) || exit 1
  fi
  ```
- [ ] エラーメッセージの改善（わかりやすいガイダンス）
- [ ] テスト: 新しいworktreeでcommitを試す

**完了条件**:
- 新しいworktreeで初回commit時に自動でnpm installが実行される
- 依存関係がない場合、明確なメッセージが表示される
- セットアップ忘れによるエラーが発生しない

**参考**:
- `node_modules`は`.gitignore`されているため、worktreeごとに独立
- 初回commitは時間がかかるが、2回目以降は高速

---

## 📋 開発優先順位

### 推奨開始順序

1. **最優先**: Backend Phase 1（他の全Phaseの依存元）
2. **並行開始**:
   - Import Phase 1-2
   - Backend Phase 2
3. **並行開始**:
   - Import Phase 3-4
   - Backend Phase 3
   - Frontend Phase 1-2
4. **並行開始**:
   - Frontend Phase 3-4
5. **統合**: エンドツーエンドテスト

### 並行開発の注意点

- Import Phase 4はBackend Phase 1完了後に開始
- Frontend Phase 2はBackend Phase 3完了後だと実データでテスト可能（必須ではない）
- 各Phaseは独立したブランチで作業、PR作成
