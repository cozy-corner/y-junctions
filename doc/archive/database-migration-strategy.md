# データベース移行戦略: Neon ⇔ CockroachDB 切り替え可能設計

## 概要

Neon PostgreSQL と CockroachDB Serverless を環境変数のみで切り替え可能にする設計計画。

## 背景

### 現状
- **データベース**: Neon PostgreSQL
- **データ量**: 305 MB（669,041件のY字路データ）
- **使用率**: Neon無料枠の60%（305MB / 512MB）

### 課題
1. **容量制限**: 今後の海外データ追加で無料枠（512MB）を超過
2. **スケーラビリティ**: 全世界展開時に数GB～十数GB規模
3. **Neon compute起動**: 500ms～数秒（doc/cold-start-optimization.md）

### 目標
- **データベースを環境変数のみで切り替え可能にする**
- コード分岐を排除（ストラテジーパターン不要）
- 両DBで完全に同じコードで動作

## 技術的実現方法

### 共通SQL戦略

PostgreSQL互換の標準関数のみを使用することで、分岐を完全に排除。

#### 修正箇所

**ファイル**: `backend/src/db/repository.rs:88-96`

```diff
 fn add_bbox_filter(builder: &mut QueryBuilder<sqlx::Postgres>, bbox: (f64, f64, f64, f64)) {
-    builder.push("WHERE location && ST_MakeEnvelope(");
+    builder.push("WHERE ST_Intersects(location::geometry, ST_MakeEnvelope(");
     builder.push_bind(bbox.0);
     builder.push(", ");
     builder.push_bind(bbox.1);
     builder.push(", ");
     builder.push_bind(bbox.2);
     builder.push(", ");
     builder.push_bind(bbox.3);
-    builder.push(", 4326)");
+    builder.push(", 4326))");
 }
```

#### 技術的根拠

1. **PostGIS**: `ST_Intersects`は内部で`&&`演算子を使用後、exact checkを実行
2. **CockroachDB**: `ST_Intersects`も空間インデックスを利用
3. **パフォーマンス**: 両DBで空間インデックスが効くため、実用上の差はほぼない

**出典**:
- [PostGIS Performance Tips](https://postgis.net/docs/manual-3.0/performance_tips.html)
- [CockroachDB ST_Intersects](https://www.cockroachlabs.com/docs/stable/st_intersects)

### 互換性検証済み機能

| 機能 | Neon | CockroachDB | 備考 |
|------|------|-------------|------|
| `GEOGRAPHY(POINT, 4326)` | ✅ | ✅ | 完全互換 |
| `ST_MakeEnvelope()` | ✅ | ✅ | 完全互換 |
| `ST_X()`, `ST_Y()` | ✅ | ✅ | 完全互換 |
| `ST_Intersects()` | ✅ | ✅ | 両方でインデックス利用 |
| `GENERATED ALWAYS STORED` | ✅ | ✅ | 完全互換 |
| `REAL[3]` 配列型 | ✅ | ✅ | 完全互換 |
| 部分インデックス (`WHERE`) | ✅ | ✅ | 完全互換 |
| `LEAST()` 関数 | ✅ | ✅ | PostgreSQL互換 |
| `CREATE EXTENSION postgis` | ✅ | ✅ | CockroachDBは組み込み、構文は許容 |
| `USING GIST` | ✅ | ✅ | CockroachDBではGIN作成、構文互換 |
| `BIGSERIAL` | ✅ | ✅ | 完全互換 |

### CockroachDB互換性のための修正

CockroachDBはPostgreSQL互換を謳っているが、以下の機能は非対応のため修正が必要。

#### 1. アドバイザリロック（`pg_advisory_lock()`）

**詳細**: PostgreSQLのアプリケーションレベルでの排他制御機能。sqlxはマイグレーション実行時にこの機能を使用して、複数のプロセスが同時にマイグレーションを実行しないようにロックをかける。

```sql
-- PostgreSQLでの動作例
SELECT pg_advisory_lock(12345);  -- ロック取得
-- マイグレーション実行
SELECT pg_advisory_unlock(12345);  -- ロック解放
```

**使用目的**: 複数のテストプロセスが並行実行されても、マイグレーションは1つずつ順番に実行される。

**問題**: CockroachDBはこの関数が実装されていない（`unknown function: pg_advisory_lock()`エラー）。

**影響範囲**: テストコード内でのマイグレーション実行。

**解決策**: `set_locking(false)`でロックを無効化。

**ファイル**: `backend/tests/api_tests.rs`

```diff
 // マイグレーション実行（テストDB初回実行時にスキーマを作成）
+// CockroachDB用にアドバイザリロックを無効化（pg_advisory_lock非対応のため）
 sqlx::migrate!("./migrations")
+    .set_locking(false)
     .run(&pool)
     .await
     .expect("Failed to run migrations");
```

**注意**: コマンドライン版の`sqlx migrate run`はロックなしで動作するため修正不要。

#### 2. TRUNCATE TABLE構文

**詳細**: `RESTART IDENTITY`は`TRUNCATE`コマンドのオプションで、テーブルをクリアする際に自動採番カウンター（SERIAL、IDENTITYなど）を初期値にリセットする。

```sql
-- PostgreSQLでの動作例
CREATE TABLE test (id SERIAL PRIMARY KEY, name TEXT);
INSERT INTO test (name) VALUES ('Alice'), ('Bob');  -- id: 1, 2

-- RESTART IDENTITYなし
TRUNCATE TABLE test;
INSERT INTO test (name) VALUES ('Charlie');  -- id: 3 (続きから)

-- RESTART IDENTITYあり
TRUNCATE TABLE test RESTART IDENTITY;
INSERT INTO test (name) VALUES ('Dave');  -- id: 1 (リセット)
```

**使用目的**: テスト時にテーブルをクリーンな状態（IDが1から）に戻す。

**問題**: CockroachDBは`RESTART IDENTITY`構文が非対応（`syntax error`）。

**影響範囲**: テストコードのテーブルクリア処理。

**解決策**: `RESTART IDENTITY`句を削除。CockroachDBでは`unique_rowid()`を使用しているため、RESTART不要でも各行にユニークなIDが自動生成される。

**ファイル**: `backend/tests/api_tests.rs`

```diff
-sqlx::query("TRUNCATE TABLE y_junctions RESTART IDENTITY CASCADE")
+// CockroachDBはRESTART IDENTITYをサポートしていないため削除
+sqlx::query("TRUNCATE TABLE y_junctions CASCADE")
     .execute(&pool)
     .await
     .expect("Failed to truncate table");
```

**動作**: CockroachDBでは`unique_rowid()`を使用しているため、RESTART不要でも自動的にユニークなIDが生成される。

#### 3. 検証結果

**テスト実行**:
```bash
# CockroachDBで全テスト合格
TEST_DATABASE_URL='postgres://root@localhost:26257/y_junction_test?sslmode=disable' cargo test
# 結果: 28 passed; 0 failed
```

**互換性**: 上記2つの修正により、PostgreSQLとCockroachDBの両方で同じコードが動作する。

## 実装計画（PR単位）

### PR #1: コード修正（DB抽象化）

**変更内容**: `backend/src/db/repository.rs:88-96` を修正
**検証**: 既存テストが全て通過

### PR #2: ローカル開発環境にCockroachDB追加 ✅

**変更内容**:
1. `docker-compose.yml`にCockroachDBサービスを追加
   - イメージ: `cockroachdb/cockroach:latest`
   - ポート: 26257 (SQL), 8081 (Web UI)
   - モード: `start-single-node --insecure`
2. `backend/.env.example`にCockroachDB接続文字列を追加
3. `backend/tests/api_tests.rs`の修正
   - `set_locking(false)` - アドバイザリロック無効化
   - `TRUNCATE TABLE` - RESTART IDENTITY削除

**初期セットアップ手順**:
```bash
# 1. コンテナ起動
docker-compose up -d

# 2. データベース作成
docker exec y-junctions-cockroachdb ./cockroach sql --insecure -e "CREATE DATABASE y_junction;"
docker exec y-junctions-cockroachdb ./cockroach sql --insecure -e "CREATE DATABASE y_junction_test;"

# 3. マイグレーション実行
DATABASE_URL='postgres://root@localhost:26257/y_junction?sslmode=disable' \
  sqlx migrate run --source backend/migrations

DATABASE_URL='postgres://root@localhost:26257/y_junction_test?sslmode=disable' \
  sqlx migrate run --source backend/migrations
```

**切り替え方法**:
```bash
# backend/.env を編集してDBを切り替え

# PostgreSQL使用時
DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction
TEST_DATABASE_URL=postgres://y_junction:y_junction@localhost:5432/y_junction_test

# CockroachDB使用時
DATABASE_URL=postgres://root@localhost:26257/y_junction?sslmode=disable
TEST_DATABASE_URL=postgres://root@localhost:26257/y_junction_test?sslmode=disable
```

**検証結果**:
- ✅ CockroachDBで`cargo test`が全て通過（28テスト）
- ✅ マイグレーション実行成功（5つのマイグレーション）
- ✅ テーブル作成確認（GENERATED列含む）
- ✅ 環境変数切り替えのみで動作確認

### PR #3: 本番環境にCockroachDB追加

**変更内容**:
- `terraform/cockroachdb.tf` 追加
- Serverless cluster（GCP asia-southeast1）
- 無料枠のみ（spend_limit = 0）
- 接続URIを出力

**参考**: [CockroachDB Terraform Provider](https://registry.terraform.io/providers/cockroachdb/cockroach/latest/docs)

## 本番移行手順（PR外作業）

### 1. 検証
- CockroachDB環境でマイグレーション実行
- 単体テスト、パフォーマンステスト
- `EXPLAIN ANALYZE`でインデックス確認

### 2. データ移行
- 段階的移行（テストデータ → 本番データ）
- データ整合性検証
- ロールバック手順準備

### 3. 切り替え
- 環境変数`DATABASE_URL`を変更
- 動作確認

## リスク

### パフォーマンス差異

**要因**: 空間インデックスの実装が異なる（GIST vs GIN）
**対策**: `EXPLAIN ANALYZE`で実測比較

### データ移行失敗

**要因**: ネットワーク障害、データ破損
**対策**: 完全バックアップ、段階的移行、ロールバック手順

## 切り替え方法

### ローカル開発
docker-compose.ymlで両DBを起動し、環境変数で切り替え。

### 本番環境（Cloud Run）
`terraform/backend.tf`の`DATABASE_URL`の`value`を変更し、`terraform apply`。

## 参考資料

- [PostGIS Performance Tips](https://postgis.net/docs/manual-3.0/performance_tips.html)
- [CockroachDB Spatial Functions](https://www.cockroachlabs.com/docs/stable/spatial-indexes)
- [Neon Plans](https://neon.com/docs/introduction/plans)
- [CockroachDB Serverless Pricing](https://www.cockroachlabs.com/pricing/)
