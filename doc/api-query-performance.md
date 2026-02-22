# API クエリ パフォーマンス最適化

## 背景

CockroachDB 移行後、`/api/junctions` エンドポイントが 3.9〜4.2 秒かかる問題が発生。
PR #175 でCockroachDBの空間インデックスが使われていない問題を修正し、400〜750ms に改善。
しかし「slow statement」アラートは引き続き発生しているため、さらなる改善を検討する。

## 現状の計測値（2026-02-22 時点）

| bbox サイズ | 結果件数 | DB クエリ時間 | レスポンスサイズ |
|------------|---------|-------------|---------------|
| 小（1〜2km四方） | 22〜31件 | 85〜90ms | 14〜20 KiB |
| 中（3〜4km四方） | 124〜134件 | 160〜240ms | 77〜85 KiB |
| 大（10km四方以上） | 400〜500件 | 320〜430ms | 230〜320 KiB |

結果件数が増えるほど線形に遅くなる傾向あり。LIMIT 500 に達するケースも多い。

## リクエストパターンの分析

ユーザーが地図をパン・ズームすると 1 秒おきに複数クエリが発行される。
たとえば 7 秒間に 8 リクエストが連続するケースを観測。

- bboxはリクエストごとに異なる（同一クエリの重複ではない）
- デバウンス 300ms は設定済みだが、1 秒おきのゆっくりしたパンには効かない
- 前のリクエストが処理中でも次のリクエストが発行される

## 現在のクエリ

```sql
SELECT id, osm_node_id,
  ST_Y(location::geometry) as lat,   -- 全行でgeography→geometry変換
  ST_X(location::geometry) as lon,   -- 全行でgeography→geometry変換
  angle_1, angle_2, angle_3, bearings, created_at,
  elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff,
  way_1_highway_type, way_2_highway_type, way_3_highway_type,
  way_1_category, way_2_category, way_3_category
FROM y_junctions
WHERE ST_Intersects(location, ST_MakeEnvelope($1, $2, $3, $4, 4326)::geography)
  AND ((way_1_category = $5 OR way_2_category = $6 OR way_3_category = $7)
    OR (way_1_category = $8 OR way_2_category = $9 OR way_3_category = $10))
LIMIT $11
```

## コスト要因

1. **ST_Y / ST_X の変換コスト**: 結果行数分だけ geography→geometry 変換が走る
2. **ST_Intersects の geography 演算**: geometry より遅い
3. **カテゴリフィルタの OR 条件**: 3 カラムにまたがる OR はインデックスが効きにくい
4. **リクエスト頻度**: パン・ズーム操作で無駄なクエリが多数発行される

---

## 改善案

### ✅ 案1: デバウンス時間の調整（実装済み）

**対象**: フロントエンド `frontend/src/hooks/useJunctions.ts`

**内容**: `debounceMs` のデフォルト値を 300ms → 600ms に変更。

**効果**: クエリ発行回数を削減。DB クエリ自体は速くならないが、
1 秒おきの操作で発行されていたリクエストの半数程度を削減できる。

**実装コスト**: 1 行変更。マイグレーション不要。

**限界**: ゆっくりしたパン（1 秒以上の間隔）には効果なし。

---

### 案2: lat/lon を STORED 列として保存

**対象**: データベーススキーマ + バックエンドクエリ

**内容**:
```sql
-- マイグレーション追加
ALTER TABLE y_junctions
  ADD COLUMN lat FLOAT8 GENERATED ALWAYS AS (ST_Y(location::geometry)) STORED,
  ADD COLUMN lon FLOAT8 GENERATED ALWAYS AS (ST_X(location::geometry)) STORED;

CREATE INDEX idx_y_junctions_lon_lat ON y_junctions (lon, lat);
```

クエリを以下に変更:
```sql
-- Before
WHERE ST_Intersects(location, ST_MakeEnvelope($1, $2, $3, $4, 4326)::geography)
SELECT ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon

-- After
WHERE lon BETWEEN $1 AND $3 AND lat BETWEEN $2 AND $4
SELECT lat, lon  -- 直接読み出し
```

**効果**:
- ST_* 関数呼び出しを排除（結果行数分の変換コストがゼロになる）
- 単純な BETWEEN 比較でインデックスが効く
- CockroachDB の geography 演算を回避できる

**実装コスト**: マイグレーション 1 本 + Rust クエリ修正。
データの再インポートは不要（STORED 列は自動計算される）。

**注意**: 精度が geography（球面計算）から geometry（平面計算）に変わるが、
bbox フィルタの用途では誤差は無視できる範囲（数メートル以内）。

---

### 案3: カテゴリフィルタの最適化

**対象**: データベーススキーマ + バックエンドクエリ

**内容**: カテゴリをビットマスクで表現し、単一カラムのインデックスで絞り込む。

```sql
-- category_flags の各ビットの意味
-- bit 0: way_1 が 'highway'
-- bit 1: way_1 が 'major'
-- bit 2: way_1 が 'local'
-- bit 3: way_1 が 'pedestrian'
-- bit 4: way_2 が 'highway'
-- ...（以下同様）

ALTER TABLE y_junctions
  ADD COLUMN category_flags SMALLINT GENERATED ALWAYS AS (
    (CASE way_1_category
      WHEN 'highway'    THEN 1
      WHEN 'major'      THEN 2
      WHEN 'local'      THEN 4
      WHEN 'pedestrian' THEN 8
      ELSE 0 END) |
    (CASE way_2_category
      WHEN 'highway'    THEN 16
      WHEN 'major'      THEN 32
      WHEN 'local'      THEN 64
      WHEN 'pedestrian' THEN 128
      ELSE 0 END) |
    (CASE way_3_category
      WHEN 'highway'    THEN 256
      WHEN 'major'      THEN 512
      WHEN 'local'      THEN 1024
      WHEN 'pedestrian' THEN 2048
      ELSE 0 END)
  ) STORED;

CREATE INDEX idx_y_junctions_category_flags ON y_junctions (category_flags);
```

クエリ変更例（local または pedestrian を含む場合）:
```sql
-- マスク: local(4+64+1024) | pedestrian(8+128+2048) = 3276
WHERE category_flags & 3276 != 0
```

**効果**: 3 カラムの OR 条件が単一カラムのビット演算に変わる。
インデックスが効きやすくなる。

**実装コスト**: マイグレーション + Rust 側でビットマスク計算ロジックの追加。

---

### 案4: アプリケーションレベルのインメモリキャッシュ

**対象**: バックエンド（Rust）

**内容**: bbox + フィルタ条件をキーとして、クエリ結果を短時間キャッシュする。

```rust
// 概念的なイメージ
use moka::future::Cache;

let cache: Cache<CacheKey, Arc<Vec<Junction>>> = Cache::builder()
    .max_capacity(200)
    .time_to_live(Duration::from_secs(30))
    .build();
```

**効果**: 同一 bbox への繰り返しリクエストで DB を叩かずに返せる。
データが変わらない限り何度でも使い回せる。

**限界**: bbox が毎回わずかに異なる（パン操作）ため、完全一致でのキャッシュヒット率は低い。
bbox を固定グリッドに丸め込む工夫が必要。

**実装コスト**: 中（moka などの依存追加 + キャッシュ層の実装）。

---

### 案5: bbox の丸め込みキャッシュ

**内容**: リクエストの bbox を固定グリッド（例: 0.005° 刻み）に切り上げてからクエリ。
グリッド単位でキャッシュすることでヒット率を上げる。

```
実際の bbox:  139.5378..., 35.6297...
丸め込み後:   139.535,   35.625      ← グリッド境界に合わせる
```

**効果**: パン操作で bbox がわずかに変わっても同じキャッシュキーにヒットしやすくなる。
やや多めのデータを返すが、クライアント側でフィルタリングすれば問題ない。

**実装コスト**: 中〜高（フロントエンドとバックエンド両方の変更が必要）。

---

## 優先順位まとめ

| 案 | 効果 | 実装コスト | 優先度 |
|----|------|----------|-------|
| 案1: デバウンス調整 | リクエスト数削減 | 低（1行） | ✅ 実施済み |
| 案2: lat/lon STORED列 | クエリ 30〜50% 高速化 | 低〜中 | ⭐ 次の候補 |
| 案3: カテゴリ bitmask | カテゴリフィルタ改善 | 中 | 中期 |
| 案4: インメモリキャッシュ | DB 負荷大幅削減 | 中 | 中期 |
| 案5: bbox 丸め込みキャッシュ | キャッシュヒット率向上 | 中〜高 | 長期 |

## 参考: AbortController について

フロントエンドで AbortController を使うと前のリクエストの HTTP 接続を切断できるが、
**CockroachDB のクエリ自体はキャンセルされない**。SQLx は Future が drop されても
PostgreSQL の CancelRequest を送らないため、DB 側の処理は最後まで続く。
AbortController の主な効果は「古いレスポンスで UI が上書きされない」ことであり、
サーバー負荷の削減効果はほとんど期待できない。
