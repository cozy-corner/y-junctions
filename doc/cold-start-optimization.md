# Cloud Run Cold Start パフォーマンス改善

## 📊 サマリー

**問題:** Cloud Runコールドスタート時に13秒かかっていた

**対策:** Startup CPU Boost有効化（月3円）

**結果:**
- DB接続時間: 5.7秒 → **1.2秒**（-79%）
- リビジョン起動: 6.2秒 → **4.5秒**（-27%）
- 総レイテンシ: 約13秒 → **約6秒**（推定）

**結論:** Phase 1で十分な改善を達成。追加対策は費用対効果が低い

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
| Autosuspend延長 | 頻度削減のみ | 月19-24ドル | **低** | 未実施 |
| Connection Pooler | 不明 | 要調査 | **低** | 未調査 |
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
- **Neon有料プラン（Neon Pro以上）へのアップグレード: 月19ドル〜（必須）**
  - 無料プランではautosuspend設定の変更不可
  - 出典: [Neon Scale to Zero](https://neon.com/docs/guides/auto-suspend-compute)
- compute使用時間増加による追加費用: 0-5ドル/月程度（アクセスパターン次第）
- **合計推定コスト: 月19-24ドル**

**評価:** Phase 1で既に1.2秒まで改善済みのため、費用対効果が悪い

参考: [Neon Compute Lifecycle](https://neon.com/docs/introduction/compute-lifecycle)

---

### Connection Pooler（効果不明 ❓）

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

**効果:**
- **Phase 1実施前の想定:** TCP/TLS/認証オーバーヘッド削減（不明）
- **Phase 1実施後の状況:** DB接続時間は既に1.2秒まで改善済み
- **推定:** さらなる改善効果は限定的

**制限:**
- コールドスタート時は毎回Cloud Run → Pooler間の接続が必要
- Neon compute起動時間（500ms〜数秒）は変わらない

**コスト要件:** 要調査（無料プランで利用可能か不明）

**評価:** 効果が限定的、Phase 1で既に改善済み

---

### connect_lazy()（副次的メリットあり 🔄）

```rust
let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect_lazy(&database_url);
```

**仕組み:**
- `.connect()`は起動時に接続を確立
- `.connect_lazy()`は初回リクエスト時まで遅延

**効果:**

| 項目 | 現在（connect） | connect_lazy使用時 |
|-----|----------------|-------------------|
| リビジョン起動時間 | 4.54秒 | **3.3秒**（-1.2秒、推定） |
| 初回リクエスト処理 | 即座 | +1.2秒（DB接続） |
| **ユーザー体感** | **変わらず**（合計は同じ） | **変わらず** |

**副次的メリット:**
1. **可用性向上**: DB接続エラーでもサーバーは起動できる
2. **デバッグ改善**: 起動とDB接続の問題を切り分けやすい
3. **Cloud Runヘルスチェック**: 起動完了が早まる可能性

**制限:**
- ユーザー体感のレイテンシは変わらない
- 初回リクエストが1.2秒遅くなる（起動で短縮した分）

**コスト:** 0円（コード変更のみ）

**評価:** ユーザー体感改善なし。可用性向上のメリットはあるが優先度は低い

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
