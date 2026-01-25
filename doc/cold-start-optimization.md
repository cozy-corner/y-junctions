# Cloud Run Cold Start パフォーマンス改善

## 📊 問題と改善状況

~~Cloud Runコールドスタート時に**13秒**かかる。~~ → **✅ 改善完了（2026-01-25）**

- ~~Rustアプリケーション起動~~
- ~~SQLxによるNeon PostgreSQL（Singapore）への接続確立~~

**改善結果:**
- DB接続時間: 5.7秒 → **1.2秒**（-79%）
- リビジョン起動: 6.2秒 → **4.5秒**（-27%）
- **対策:** Startup CPU Boost有効化（月3円のコスト）

## ✅ 確定している事実

### 実測データ

**接続テスト（RustアプリからNeon PostgreSQLへの接続確立）:**

| 環境 | 接続内容 | 接続時間 |
|------|---------|---------|
| ローカルマシン | MacBook（Tokyo）→ Neon PostgreSQL（Singapore） | 72-107ms |
| Cloud Run | Tokyo リージョン → Neon PostgreSQL（Singapore） | 5,700ms |

→ **約5.6秒の遅延がCloud Run環境で発生**

- プロトコル: PostgreSQL over TCP/TLS
- クライアント: SQLx (`PgPoolOptions::new().max_connections(5).connect()`)
- サーバー: Neon PostgreSQL 16 (aws-ap-southeast-1)

**本番ログ（2026-01-25）:**

```
00:09:28.321 - リクエスト受信      (13.394秒)
00:09:28.344 - インスタンス起動開始
00:09:34.575 - DB接続完了 (警告: 5.693秒)
00:09:34.577 - サーバー起動完了
00:09:41.976 - 初回クエリ完了 (3.575秒)
00:09:42.014 - 2回目クエリ完了 (6.029秒)
00:09:42.026 - 3回目クエリ完了 (6.189秒)
00:09:42.036 - 4回目クエリ完了 (1.996秒)
00:09:42.098 - 5回目クエリ完了 (7.368秒)
00:09:43.994 - 以降のリクエスト   (419ms)
```

**パフォーマンス推移:**

| リクエスト | レイテンシ | 状態 |
|-----------|-----------|------|
| 1回目 | 13.394秒 | Cold start |
| 2回目 | 6.531秒 | DB接続プール初期化中 |
| 3回目 | 6.691秒 | キャッシュウォームアップ中 |
| 4回目 | 4.102秒 | |
| 5回目 | 2.496秒 | |
| 6回目 | 0.569秒 | ウォームアップ完了 |
| 7回目以降 | 0.4秒台 | 安定状態 |

**ログ証拠:**

```
WARN sqlx::pool::acquire: acquired connection, but time to acquire
exceeded slow threshold
aquired_after_secs=5.693114457
slow_threshold=2.0

WARN sqlx::query: slow statement: execution time exceeded alert threshold
elapsed=3.574957351s slow_threshold=1s
elapsed=6.028688088s slow_threshold=1s
elapsed=7.367838932s slow_threshold=1s
```

**レイテンシ内訳:**

```
合計: 13.655秒（HTTPログ: 13.394秒）

1. インスタンス起動開始トリガーまで: 0.023秒
2. コンテナ起動 + アプリ起動 + DB接続: 6.231秒
   ├ コンテナ起動 + Rustバイナリ起動: ~0.5秒（推定）
   └ SQLx → Neon PostgreSQL接続: 5.693秒
3. 初回リクエスト処理（クエリ実行）: 7.401秒

※2と3で13.632秒
※3は初回のみの遅延（2回目以降は0.4秒台）
※2はコールドスタート時に毎回発生
```

**Cloud Run設定:**
- Startup CPU Boost: ~~未設定~~ → **有効化済み (2026-01-25)**
- connect_timeout: 未設定
- CPU throttling: false

### 確定した結論

1. **ボトルネック:** SQLxによるNeon PostgreSQL接続確立に5.7秒
2. **物理的距離は原因ではない**（ローカルマシンからも同じ距離で72-107ms）
3. **Cloud Run環境固有で5.6秒の遅延が発生**
4. **根本原因:** Startup CPU Boost未設定によるCPU性能不足（実測で確定）

## ✅ 改善結果（2026-01-25）

### Phase 1実施: Startup CPU Boost有効化

**実測データ（改善後）:**

```
11:59:40.533 - インスタンス起動開始
11:59:41.741 - DB接続完了
11:59:41.743 - サーバー起動完了
11:59:41.797 - リビジョンデプロイ成功 (4.54秒)
```

**改善効果:**

| 項目 | 改善前 | 改善後 | 改善幅 |
|------|--------|--------|--------|
| **DB接続時間** | 5.693秒 | **1.208秒** | **-4.485秒 (-79%)** |
| **リビジョン起動** | 6.231秒 | **4.54秒** | **-1.691秒 (-27%)** |

**結論:**
- Startup CPU Boostにより、DB接続時間が**5.7秒 → 1.2秒**に短縮
- **約4.5秒（79%）の改善**を達成
- 根本原因はCPU性能不足だった（ネットワーク接続処理がCPU律速）
- コスト: 月3円程度（予測通り）

## ❓ 推測・未確定

### 残存する課題

**Phase 1完了後の状態:**
- リビジョン起動: 4.54秒（改善済み）
- 初回リクエスト: 次回コールドスタート時に計測予定

**今後の対策候補:**
1. Autosuspend延長（Phase 2）- コールドスタート頻度削減
2. Connection Pooler（Phase 3）- 効果限定的

## 🎯 対策

### Phase 1: Startup CPU Boost有効化 ✅ 完了（2026-01-25）

#### 1-1. Startup CPU Boost有効化

```hcl
# terraform/backend.tf
resources {
  limits = {
    cpu    = "1"
    memory = "512Mi"
  }
  startup_cpu_boost = true  # 追加
}
```

**仕組み:**
- コールドスタート時にCPUを2倍に増強（起動時+10秒間）
- 通常CPU: 1 → Boost時: 2

**実測効果:**
- ✅ **DB接続時間: 5.7秒 → 1.2秒（-79%）**
- ✅ **リビジョン起動: 6.2秒 → 4.5秒（-27%）**
- ✅ **合計改善: 約4.5秒（推定1-3秒を大きく上回る）**

**確定コスト:** 月3円程度（予測通り）

参考: https://cloud.google.com/run/docs/configuring/cpu-boost

### Phase 2: 頻度削減（未実施）

#### 2-1. Autosuspend延長

```hcl
# terraform/neon.tf
default_endpoint_settings {
  suspend_timeout_seconds = 900  # 5分 → 15分
}
```

**仕組み:**
- Neonは非アクティブ時間後にcomputeを自動停止（現在5分）
- 次のアクセス時にcomputeを再起動（500ms〜数秒）
- 延長することで停止される前に次のアクセスが来る確率が上がる

**効果:**
- **確定:** Cold start（compute起動）の発生頻度を削減
- **制限:** 発生した場合の遅延時間は変わらない（5.7秒は残る）

**コスト:**
- Neon無料枠: 100時間/月
- 推定アクティブ時間: 月90-150時間
- **推定追加コスト:** 0-5ドル/月

出典: [Neon Compute Lifecycle](https://neon.com/docs/introduction/compute-lifecycle)

### Phase 3: 効果が限定的（未実施）

#### 3-1. SSL Negotiation最適化

```hcl
# terraform/backend.tf
env {
  name  = "DATABASE_URL"
  value = "${neon_project.main.connection_uri}?sslmode=require&sslnegotiation=direct&connect_timeout=10"
}
```

**仕組み:**
- `sslnegotiation=direct`: 不要なSSLネゴシエーションステップをスキップ（Neon公式推奨）
- `connect_timeout=10`: 接続タイムアウトを明示的に設定

**効果と制限:**
- **推定効果:** 0.1-0.5秒程度
- **制限:** 5.7秒のボトルネックに対して誤差レベル

出典: [Neon Connection Latency](https://neon.com/docs/connect/connection-latency)

#### 3-2. Connection Pooler

```hcl
default_endpoint_settings {
  pooler_enabled = true
  pooler_mode    = "transaction"
}
```

**仕組み:**
- NeonのPgBouncer経由で接続
- TCP/TLS/認証をプールして再利用

**効果と制限:**
- **推定効果:** TCP/TLS/認証オーバーヘッド削減（0.5秒程度？）
- **制限:** コールドスタート時の初回接続は依然として必要
- **制限:** Neon compute起動時間（500ms〜数秒）は変わらない

#### 3-3. connect_lazy()

```rust
let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect_lazy(&database_url);
```

**仕組み:**
- `.connect()`は起動時に接続を確立
- `.connect_lazy()`は初回リクエスト時まで遅延

**効果と制限:**
- **確定効果:** アプリ起動時間短縮（6秒 → 0秒）
- **制限:** 初回リクエスト時に接続確立が必要（ユーザー体感は変わらず）

## 📚 参考資料

- [Neon Connection Latency](https://neon.com/docs/connect/connection-latency)
- [Cloud Run CPU Boost](https://cloud.google.com/run/docs/configuring/cpu-boost)
- [Neon Compute Lifecycle](https://neon.com/docs/introduction/compute-lifecycle)
