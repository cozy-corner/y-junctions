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
