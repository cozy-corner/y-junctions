# Y字路検索サービス 開発タスクリスト

## 並行開発戦略

- 3つの領域（Import, Backend, Frontend）で並行開発可能
- 各Phase = 1 PR
- Phase内の全タスクを完了させてからPR作成

---

## 🗄️ Import (データインポート)

### Phase 1: CLIツール基盤とPBFパーサー骨格

**ゴール**: コマンドラインツールとPBFファイル読み込みの基盤を構築

**成果物**:
- `backend/Cargo.toml` - importバイナリ定義追加
- `backend/src/bin/import.rs` - CLI実装
- `backend/src/importer/mod.rs` - モジュール定義
- `backend/src/importer/parser.rs` - PBF読み込み骨格

**タスク**:
- [ ] Cargo.tomlに`[[bin]]` import追加
- [ ] clap でCLI引数パース実装（--input, --bbox）
- [ ] osmpbfクレートでPBFファイルオープン確認
- [ ] エラーハンドリング基盤（anyhow/thiserror）

**完了条件**:
- `cargo run --bin import -- --help` でヘルプ表示
- `cargo run --bin import -- --input test.pbf --bbox 139,35,140,36` でファイル読み込み開始（まだ処理なし）

---

### Phase 2: Y字路検出ロジック

**ゴール**: OSMデータからY字路候補（3本道路が接続するNode）を抽出

**成果物**:
- `backend/src/importer/parser.rs` - 2パス処理実装
- `backend/src/importer/detector.rs` - Y字路判定ロジック

**タスク**:
- [ ] 1st pass: highway付きWayのNode IDをHashSetに収集
- [ ] 各NodeのWay接続数をHashMapでカウント
- [ ] 接続数==3のNodeをフィルタリング
- [ ] highwayタイプチェック（residential, tertiary等）
- [ ] 2nd pass: 該当NodeとWayの座標を取得

**完了条件**:
- テスト用PBFでY字路候補が抽出される
- ログに「Found X Y-junction candidates」と出力

---

### Phase 3: 角度計算とデータモデル

**ゴール**: Y字路の3つの角度を計算し、タイプ分類

**成果物**:
- `backend/src/importer/calculator.rs` - 角度計算
- `backend/src/domain/junction.rs` - Junctionモデル（importで使用）

**タスク**:
- [ ] geo crateで方位角（bearing）計算
- [ ] 3方向の角度を算出・昇順ソート
- [ ] angle_type分類ロジック（sharp/even/skewed/normal）
- [ ] Junction構造体定義（osm_node_id, location, angles, road_types）

**完了条件**:
- Y字路候補に対して角度計算完了
- ログに各Y字路の角度情報出力（例: `Node 123: [45°, 135°, 180°] type=sharp`）

---

### Phase 4: データベース投入

**ゴール**: 計算済みY字路データをPostgreSQLへバルクインサート

**成果物**:
- `backend/src/importer/inserter.rs` - バルクインサート処理

**タスク**:
- [ ] sqlxでPostgreSQL接続
- [ ] トランザクション開始
- [ ] バッチ挿入（1000件ずつ等）
- [ ] 進捗表示（`Inserted 1000/5000...`）
- [ ] エラー時ロールバック

**完了条件**:
- 小規模PBFファイルでエンドツーエンド動作
- データベースにy_junctionsレコードが挿入される
- `cargo run --bin import -- --input kanto-latest.pbf --bbox 138.9,35.5,140.0,35.9` が成功

**依存**: Backend Phase 1完了（DBスキーマ必要）

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

### Phase 2: ドメインモデルとリポジトリ層

**ゴール**: Junction型とデータアクセスロジックを実装

**成果物**:
- `backend/src/domain/mod.rs`
- `backend/src/domain/junction.rs` - ドメインモデル
- `backend/src/db/mod.rs` - DB接続プール
- `backend/src/db/repository.rs` - リポジトリ実装

**タスク**:
- [ ] Junction構造体とAngleType enum
- [ ] GeoJSON変換実装（`to_feature()`, `to_feature_collection()`）
- [ ] sqlxでDB接続プール作成
- [ ] `find_by_bbox()` 実装（bbox + フィルタ対応）
- [ ] `find_by_id()` 実装
- [ ] `count_by_type()` 実装

**完了条件**:
- ユニットテストでリポジトリメソッド動作確認
- モックデータで各メソッドが正しいSQLを実行

---

### Phase 3: API エンドポイント実装

**ゴール**: REST APIの3つのエンドポイントを完成

**成果物**:
- `backend/src/api/mod.rs`
- `backend/src/api/handlers.rs` - ハンドラー関数
- `backend/src/api/routes.rs` - ルーティング
- `backend/src/main.rs` - 更新（APIマウント）

**タスク**:
- [ ] `GET /api/junctions` ハンドラー
  - クエリパラメータ解析（bbox, angle_type, min_angle_lt/gt, limit）
  - バリデーション
  - GeoJSON FeatureCollection レスポンス
  - Street View URL生成
- [ ] `GET /api/junctions/:id` ハンドラー
- [ ] `GET /api/stats` ハンドラー
- [ ] CORS設定（tower-http）
- [ ] エラーレスポンス（JSON形式）

**完了条件**:
- `curl http://localhost:8080/api/junctions?bbox=139,35,140,36` でGeoJSON取得
- `curl http://localhost:8080/api/junctions/1` で単一データ取得
- `curl http://localhost:8080/api/stats` で統計情報取得

**依存**: Phase 2完了

---

## 🎨 Frontend (UI)

### Phase 1: 型定義とAPIクライアント

**ゴール**: TypeScript型定義とバックエンドAPI通信基盤を構築

**成果物**:
- `frontend/src/types/index.ts` - 型定義
- `frontend/src/api/client.ts` - APIクライアント

**タスク**:
- [ ] Junction型、AngleType型、GeoJSON型定義
- [ ] FilterParams型、AppState型定義
- [ ] `fetchJunctions(bbox, filters)` 実装
- [ ] `fetchJunctionById(id)` 実装
- [ ] `fetchStats()` 実装
- [ ] エラーハンドリング（try-catch + Error型）

**完了条件**:
- TypeScriptコンパイルエラーなし
- モック応答でAPIクライアント動作確認

---

### Phase 2: 地図コンポーネントとマーカー表示

**ゴール**: Leaflet地図上にY字路マーカーを表示

**成果物**:
- `frontend/src/components/MapView.tsx` - 地図コンポーネント
- `frontend/src/hooks/useJunctions.ts` - データ取得フック
- `frontend/src/App.tsx` - 更新（MapView統合）

**タスク**:
- [ ] react-leafletでベース地図表示
- [ ] OpenStreetMapタイル設定
- [ ] 初期位置: 東京駅 (139.7671, 35.6812), zoom 14
- [ ] bounds変更イベントでAPI再取得（デバウンス300ms）
- [ ] useJunctionsフック実装（ローディング状態管理）
- [ ] Marker表示（angle_typeで色分け）

**完了条件**:
- `npm run dev` で地図表示
- 地図移動時にコンソールで新しいbbox確認
- マーカーが表示される（バックエンドが起動していれば実データ、なければモック）

**依存**: Backend Phase 3推奨（API実装済みだと実データでテスト可）

---

### Phase 3: フィルターパネルとポップアップ

**ゴール**: フィルタリング機能とY字路詳細表示

**成果物**:
- `frontend/src/components/FilterPanel.tsx` - フィルタUI
- `frontend/src/components/JunctionPopup.tsx` - ポップアップ
- `frontend/src/components/StatsDisplay.tsx` - 統計表示
- `frontend/src/hooks/useFilters.ts` - フィルタ状態管理

**タスク**:
- [ ] FilterPanel実装
  - angle_typeチェックボックス（sharp/even/skewed/normal）
  - min_angleスライダー（0-180°）
  - フィルタリセットボタン
- [ ] JunctionPopup実装
  - 角度表示
  - 道路タイプ表示
  - Street Viewリンク
- [ ] StatsDisplay実装（検索結果件数）
- [ ] useFiltersフック（フィルタ変更時API再取得）

**完了条件**:
- フィルタ変更でマーカーが絞り込まれる
- マーカークリックでポップアップ表示
- 検索結果件数が表示される

**依存**: Phase 2完了

---

### Phase 4: スタイリングと最適化

**ゴール**: UIデザイン完成とパフォーマンス向上

**成果物**:
- `frontend/src/App.css` - スタイル
- 最適化されたコンポーネント

**タスク**:
- [ ] レイアウト実装（左サイドバー + 右地図）
- [ ] レスポンシブ対応
- [ ] React.memo適用（MapView, FilterPanel）
- [ ] useMemo/useCallback最適化
- [ ] 大量マーカー対策（1000件以上で警告表示等）

**完了条件**:
- デザインが仕様書のワイヤーフレームに近い
- 500件マーカーでも滑らかに動作
- モバイル表示で崩れない

**依存**: Phase 3完了

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
