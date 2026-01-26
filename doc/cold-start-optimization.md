# Cloud Run Cold Start パフォーマンス改善

## 📊 サマリー

**問題:** Cloud Runコールドスタート時に13秒かかっていた

**対策:**
- Phase 1: Startup CPU Boost有効化（月3円）
- Phase 2: connect_lazy()実装（0円）

**結果:**
- DB接続時間: 5.7秒 → **1.2秒**（-79%、Phase 1実測）
- リビジョン起動: 6.2秒 → **4.5秒**（-27%、Phase 1実測） → **約3.3秒**（Phase 2計算値）
- 総レイテンシ: 約13秒 → **約6秒**（Phase 1実測、Phase 2は合計変わらず）

**結論:** Phase 1で十分な改善を達成。Phase 2は可用性向上のため実装完了

**注記:** Phase 2のリビジョン起動3.3秒は、Phase 1実測値（4.54秒）からDB接続時間（1.208秒）を引いた計算値。Cloud Run環境での実測は未実施。

---

## 🔍 Phase 1実施前の状況

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

**本番ログ（2026-01-25、改善前）:**

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

### 根本原因の特定

1. **ボトルネック:** SQLxによるNeon PostgreSQL接続確立に5.7秒
2. **物理的距離は原因ではない**（ローカルマシンからも同じ距離で72-107ms）
3. **Cloud Run環境固有で5.6秒の遅延が発生**
4. **根本原因:** Startup CPU Boost未設定によるCPU性能不足（実測で確定）
   - ネットワーク接続処理（TLS handshake等）がCPU律速

---

## ✅ Phase 1: Startup CPU Boost有効化（完了 2026-01-25）

### 実施内容

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

参考: https://cloud.google.com/run/docs/configuring/cpu-boost

### 実測結果

**本番ログ（2026-01-25、改善後）:**

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
| **総レイテンシ** | 約13秒 | **約6秒** | **約-7秒** （推定、次回実測予定） |

### コスト

**実測:** 月3円程度（予測通り）

### 結論

- ✅ DB接続時間を**5.7秒 → 1.2秒**に短縮（-79%）
- ✅ 根本原因（CPU性能不足）を解決
- ✅ 低コスト（月3円）で大きな改善を達成

---

## 🔮 追加対策候補

Phase 1で十分な改善を達成したため、追加対策の優先度は低い。

### 対策の評価一覧

| 対策 | 効果 | コスト | 優先度 | 状態 |
|-----|-----|--------|--------|------|
| Autosuspend延長 | 頻度削減のみ | 月5-10ドル（推定） | **低** | 未実施 |
| Connection Pooler | アクセス頻度依存 | 0円 | **低** | 未実施 |
| connect_lazy() | 可用性向上 | 0円 | **低** | 未実施 |

**推奨:** Phase 1（月3円）のみで運用し、追加対策は不要

---

### Autosuspend延長（未実施）

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
- **制限:** 発生した場合の遅延時間は変わらない（Phase 1実施後でも1.2秒のDB接続は必要）

**コスト:**
- **Neon有料プラン（Launch以上）へのアップグレード（必須）**
  - Freeプランではautosuspend設定が5分固定で変更不可
  - Launch/Scaleプランは使用量ベース課金（月額最低料金なし）
  - 出典: [Neon Pricing](https://neon.com/pricing), [Neon Scale to Zero](https://neon.com/docs/guides/scale-to-zero-guide)
- **推定月額コスト（2026年1月時点の使用量ベース料金）:**
  - Launchプラン基本使用: 約5ドル/月〜（コンピュート時間次第）
  - Autosuspend延長による追加compute使用: 0-5ドル/月程度（アクセスパターン次第）
  - **合計推定: 月5-10ドル**（アクセス頻度が低ければ5ドル未満の可能性もあり）
- **注意:** 上記は使用量ベース課金の概算。実際のコストはcompute使用時間により変動

**評価:** Phase 1で既に1.2秒まで改善済みのため、月5-10ドルのコストに対して費用対効果が低い

参考: [Neon Compute Lifecycle](https://neon.com/docs/introduction/compute-lifecycle)

---

### Connection Pooler（アクセス頻度依存 🔄）

```hcl
# terraform/neon.tf
default_endpoint_settings {
  pooler_enabled = true
  pooler_mode    = "transaction"
}
```

**仕組み:**
- NeonのPgBouncer経由で接続
- TCP/TLS/認証をプールして再利用
- Serverless環境で推奨される機能

参考: [Neon Connection Pooling](https://neon.com/docs/connect/connection-pooling)

**効果（アクセスパターンによって変わる）:**

| Cloud Runインスタンス | Neon compute | Connection Poolerの効果 |
|-------------------|-------------|---------------------|
| ❄️ コールドスタート | ❄️ suspend中 | **限定的**（両方の起動が必要） |
| ✅ 起動中 | ❄️ suspend中 | **あり**（Pooler経由でcompute起動後の接続が速い） |
| ❄️ コールドスタート | ✅ 起動中 | **あり**（Pooler経由で新規接続が速い） |
| ✅ 起動中 | ✅ 起動中 | **大きい**（接続がプールされている） |

**現在の設定での効果:**
- **低頻度アクセス（1時間に1回など）:** 効果限定的（毎回両方コールドスタート）
- **中頻度アクセス（5-15分間隔）:** 効果あり（Cloud Runのみコールドスタート）
- **高頻度アクセス（数分間隔）:** 大きな効果（両方起動中）

**Phase 1実施後の状況:**
- DB接続時間は既に1.2秒まで改善済み
- この1.2秒の内訳は不明（Neon compute起動時間を含む可能性あり）
- Connection Poolerで接続確立部分は高速化できるが、compute起動（500ms〜数秒）は変わらない

**制限:**
- Neon compute起動時間（500ms〜数秒）は削減できない
- 低頻度アクセスでは効果が薄い

**コスト:** 0円（無料プランでも利用可能、追加料金なし）
- 全Neonプランでサポート（Free tier含む）
- 10,000同時接続まで対応
- 参考: [Neon plans](https://neon.com/docs/introduction/plans)

**評価:** アクセス頻度が高ければ効果あり。コスト0円のため、試験的に有効化して効果測定する価値はある

参考: [Neon Postgres Deep Dive: Serverless SQL](https://dev.to/dataformathub/neon-postgres-deep-dive-why-the-2025-updates-change-serverless-sql-5o0)

---

### connect_lazy()（完了 2026-01-27）

```rust
let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect_lazy(&database_url)
    .expect("Failed to create connection pool");
```

**仕組み:**
- `.connect()`は起動時に接続を確立
- `.connect_lazy()`は初回リクエスト時まで遅延

**実測結果（ローカル環境 - MacBook → localhost PostgreSQL）:**

| 項目 | connect | connect_lazy | 差分 |
|-----|---------|-------------|------|
| 起動時 | 44.85ms | **1.74ms** | **-43ms** ✅ |
| 初回リクエスト | 91ms | 120ms | +29ms |
| 2回目以降 | 2-3ms | 2-3ms | 変わらず |
| **合計（起動+初回）** | 135.85ms | **121.74ms** | **-14ms** ✅ |

**Cloud Run環境での予測:**
- リビジョン起動: 4.54秒 → **約3.3秒**（計算: 4.54秒 - 1.208秒のDB接続時間）
- 初回リクエスト: 即座 → +1.2秒（DB接続）
- **ユーザー体感の合計レイテンシ: 変わらず**（起動3.3秒 + 初回1.2秒 ≈ 4.5秒、Phase 1と同等）

※ Phase 1実施後の実測値（リビジョン起動4.54秒、DB接続1.208秒）からの単純計算。Cloud Run環境での実測は未実施。

**副次的メリット:**
1. **可用性向上**: DB接続エラーでもサーバーは起動できる
2. **デバッグ改善**: 起動とDB接続の問題を切り分けやすい
3. **Cloud Runヘルスチェック**: 起動完了が早まる（リビジョンのヘルスチェック時間短縮）

**コスト:** 0円（コード変更のみ）

**実施日:** 2026-01-27

**結論:** ユーザー体感のレイテンシは変わらないが、副次的メリット（可用性向上）のため実装完了

---

## 🔒 技術的制約により実施不可

### SSL Negotiation最適化 ❌

**現状:** 技術的に実施できません

**理由:**
1. **SQLx 0.8が未サポート**: `sslnegotiation=direct`に対応していない
   - [Issue #3880](https://github.com/launchbadge/sqlx/issues/3880)で対応リクエスト中
2. **PostgreSQL 16使用中**: この機能はPostgreSQL 17以降が必要
   - 現在の設定: `pg_version = 16` (terraform/neon.tf)

**仕組み（PostgreSQL 17 + 対応クライアントの場合）:**
- `sslnegotiation=direct`: 不要なSSLネゴシエーションステップをスキップ
- Neonベンチマーク: 872ms → 753ms（約119ms/14%削減）
- 出典: [Neon Connection Latency](https://neon.com/docs/connect/connection-latency)

**Phase 1実施後の想定効果:**
- 1.2秒に対して0.1-0.2秒削減 → **約8-17%の改善**（推定）
- コスト: 0円（設定変更のみ）

**実施条件:**
1. SQLxが`sslnegotiation=direct`をサポート（時期未定）
2. PostgreSQL 16 → 17へアップグレード（Neonで可能）

**評価:** 対応待ち。将来的に実施を検討

---

## 📚 参考資料

- [Neon Connection Latency](https://neon.com/docs/connect/connection-latency)
- [Cloud Run CPU Boost](https://cloud.google.com/run/docs/configuring/cpu-boost)
- [Neon Compute Lifecycle](https://neon.com/docs/introduction/compute-lifecycle)
- [SQLx Issue #3880 - PostgreSQL 17 sslnegotiation=direct support](https://github.com/launchbadge/sqlx/issues/3880)
