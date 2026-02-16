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

## 実装計画（PR単位）

### PR #1: コード修正（DB抽象化）

**変更内容**: `backend/src/db/repository.rs:88-96` を修正
**検証**: 既存テストが全て通過

### PR #2: ローカル開発環境にCockroachDB追加

**変更内容**:
- `docker-compose.yml`にCockroachDBサービスを追加
- PostgreSQLとCockroachDBを並行稼働
- 環境変数`DATABASE_URL`で切り替え

**検証**:
- 両DBで`cargo test`が通過
- 環境変数切り替えのみで動作確認

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
