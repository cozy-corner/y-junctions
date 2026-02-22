# CockroachDB 移行で発見した問題

作業日: 2026-02-19

---

## 1. CHECK 制約の自動命名がPostgreSQLと異なる（対処済み）

### 現象

`001_create_y_junctions.sql` で制約名を明示せずに CHECK 制約を定義した場合、PostgreSQL と CockroachDB で自動付与される名前が異なる。

| DB | 自動付与される名前 |
|---|---|
| PostgreSQL | `y_junctions_angle_1_check` |
| CockroachDB | `check_angle_1` |

`002_...` で `DROP CONSTRAINT IF EXISTS y_junctions_angle_1_check` を実行しても、CockroachDB では該当制約が存在しないため何も消えず、古い制約が残り続ける。

### 対処

`001_create_y_junctions.sql` で制約名を明示するよう修正した。

```sql
angle_1 SMALLINT NOT NULL CONSTRAINT y_junctions_angle_1_check CHECK (angle_1 BETWEEN 0 AND 180),
angle_2 SMALLINT NOT NULL CONSTRAINT y_junctions_angle_2_check CHECK (angle_2 BETWEEN 0 AND 180),
angle_3 SMALLINT NOT NULL CONSTRAINT y_junctions_angle_3_check CHECK (angle_3 BETWEEN 0 AND 360),
```

マイグレーションファイルを変更したため、Neon と CockroachDB 両方の `_sqlx_migrations` テーブルのチェックサムを手動で更新した。

```sql
UPDATE _sqlx_migrations
SET checksum = '\x178b965aba79fe550ca350afe3aaeb888aea0a0a7e9ba2dcbbbe7ddc3e1bfc855c924d6d7b11bbd34435870157fe02aa'
WHERE version = 1;
```

---

## 2. `id` が JavaScript の安全な整数範囲を超える（未マージ）

### 現象

CockroachDB の `BIGSERIAL`（内部実装: `unique_rowid()`）は大きな 64-bit 整数を生成する（例: `-9223336893040868695`）。

JavaScript の `Number.MAX_SAFE_INTEGER = 9007199254740991` を超えるため、フロントエンドで精度が失われ、異なる `id` が同じ数値に丸められる。

`MapView.tsx` で `id` を React の `key` に使っていたため、重複 key が発生してマーカーのポップアップが空白になった。

```
Warning: Encountered two children with the same key, `-9223336893040481000`.
Keys should be unique so that components maintain their identity across updates.
```

### 対処

`osm_node_id`（OSM ノード ID）を React の `key` に使う。`osm_node_id` は `Number.MAX_SAFE_INTEGER` 以内に収まり、UNIQUE 制約がある。

```tsx
// 修正前
const { id, angle_type } = feature.properties;
<Marker key={id} ...>

// 修正後
const { osm_node_id, angle_type } = feature.properties;
<Marker key={osm_node_id} ...>
```

修正は `MapView.tsx` に適用済みだが、未コミット。別 PR でマージ予定。

---

## 3. `::geometry` キャストで空間インデックスが無効化される（対処済み）

作業日: 2026-02-22

### 現象

`/api/junctions` エンドポイントで 3〜4 秒のスロークエリが頻発。CockroachDB 移行後から発生。

```
slow statement: execution time exceeded alert threshold
GET 2008.45 KB  3.9s  /api/junctions?bbox=...&angle_type=verysharp,sharp&category=local&category=pedestrian
```

### 原因

`repository.rs` の bbox フィルタが `location` カラムを `::geometry` にキャストしていた。

```sql
-- 修正前
WHERE ST_Intersects(location::geometry, ST_MakeEnvelope($1, $2, $3, $4, 4326))
```

`location` カラムは `GEOGRAPHY` 型で、空間インデックスも `GEOGRAPHY` 型に対して作成されている。

```sql
idx_y_junctions_location gin (location)
```

`location::geometry` と書くと、オプティマイザはこれを「`location` カラムへの参照」ではなく「キャスト式」として扱う。インデックスは `location` に貼られているため、式が一致せずインデックスが使用されない。

また、角度フィルタでも `LEAST(angle_1, angle_2, angle_3)` という関数式を使っていたため、`idx_y_junctions_angle_1` も使用されていなかった。角度カラムは「小さい順にソート済み」（`angle_1 ≤ angle_2 ≤ angle_3`）であるため、`LEAST()` は常に `angle_1` と等しく冗長だった。

### EXPLAIN ANALYZE による裏付け

**修正前**

```
execution time: 4s
rows decoded from KV: 743,318 (219 MiB, 23 gRPC calls)
actual row count: 14
table: y_junctions@y_junctions_pkey
spans: FULL SCAN (SOFT LIMIT)
```

743,318 行（テーブル全体）をフルスキャンして 14 行しか返していない。

**修正後**

```
execution time: 10ms
rows decoded from KV: 463 (42 KiB, 4 gRPC calls)
actual row count: 6
table: y_junctions@idx_y_junctions_location
spans: 20 spans
└── inverted filter
    └── index join (streamer)
```

空間 GIN インデックス（inverted filter）が使用され、372 行のみスキャンして 6 行を返している。

| 指標 | 修正前 | 修正後 | 改善率 |
|------|--------|--------|--------|
| 実行時間 | 4s | 10ms | **400倍** |
| KV 読み込み行数 | 743,318 行 | 372 行 | **2000分の1** |
| KV 読み込みデータ量 | 219 MiB | 42 KiB | **5000分の1** |
| 消費 RU | 4,630 | 22 | **210分の1** |
| 使用インデックス | なし（フルスキャン） | `idx_y_junctions_location` | - |

### 対処

**`add_bbox_filter`**: キャストをカラム側からエンベロープ側に移した。

```rust
// 修正前
"WHERE ST_Intersects(location::geometry, ST_MakeEnvelope($1, $2, $3, $4, 4326))"

// 修正後
"WHERE ST_Intersects(location, ST_MakeEnvelope($1, $2, $3, $4, 4326)::geography)"
```

**`add_angle_type_filter` / `add_min_angle_filters`**: `LEAST()` を `angle_1` に置換。

```rust
// 修正前
"LEAST(angle_1, angle_2, angle_3) < 30"

// 修正後
"angle_1 < 30"
```

### なぜ PostgreSQL では問題が起きなかったか

PostgreSQL + PostGIS では、`GEOGRAPHY` 型と `GEOMETRY` 型の間に暗黙的な変換パスが存在し、オプティマイザが `location::geometry` に対して `location` の GIST インデックスを使用できると判断するケースがある。PostGIS の GIST インデックスは geography/geometry を統合的に扱う演算子クラスを持っており、型キャストをまたいだインデックス利用が可能。

CockroachDB の空間インデックスは inverted GIN インデックスとして実装されており、インデックスの対象カラムが**変換なしでそのまま述語に現れる**ことを厳密に要求する。`location::geometry` という式は `location` カラムとは別の式として扱われるため、インデックスが適用されない。

| | PostgreSQL + PostGIS | CockroachDB |
|---|---|---|
| 空間インデックス種別 | GIST | inverted GIN（S2セルベース） |
| 型キャスト越しのインデックス利用 | 演算子クラスに依存して可能 | 不可（カラム参照が一致しない） |
| geography / geometry 統合 | PostGIS が透過的に処理 | 明示的な型一致が必要 |

### 教訓

CockroachDB で空間インデックスを使用するには、**インデックスが貼られているカラムをキャストせずにそのまま `ST_Intersects` の第一引数に渡す**こと。比較対象（エンベロープ等）をカラムの型に合わせてキャストする。
