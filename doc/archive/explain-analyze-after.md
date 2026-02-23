# EXPLAIN ANALYZE アフター（STORED lat/lon + B-tree インデックス適用後）

**計測日**: 2026-02-23
**データ件数**: 743,318件
**計測条件**: コールドキャッシュ（docker restart 後の初回実行）
**クエリ**: カテゴリフィルタあり（local OR pedestrian）、LIMIT 500
**適用変更**:
- `migration 006_add_lon_lat_stored.sql`: `lat`, `lon` を STORED 生成カラムとして追加
- `idx_y_junctions_lon_lat` (lon, lat) B-tree インデックス作成
- `add_bbox_filter`: ST_Intersects → lon/lat BETWEEN に変更

---

## 小bbox（約2km × 2km）

**座標**: `lon BETWEEN 134.04 AND 134.06 AND lat BETWEEN 34.33 AND 34.35`

```
planning time: 5ms
execution time: 13ms
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 576 (49 KiB, 576 KVs, 5 gRPC calls)
cumulative time spent in KV: 11ms
maximum memory usage: 5.0 MiB
sql cpu time: <1ms

• limit (count: 500)
└── • filter
    │ actual row count: 51
    │ execution time: 175µs
    │ estimated row count: 27
    │ filter: (category conditions)
    └── • index join (streamer)
        │ actual row count: 55
        │ KV time: 10ms
        │ KV rows decoded: 55
        │ KV gRPC calls: 4
        │ table: y_junctions@y_junctions_pkey
        └── • filter
            │ actual row count: 55
            │ filter: (lat >= 34.33) AND (lat <= 34.35)
            └── • scan
                  actual row count: 56
                  KV time: 1ms
                  KV gRPC calls: 1
                  table: y_junctions@idx_y_junctions_lon_lat
                  spans: [/134.04/34.33 - /134.06/34.35]
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| B-tree index scan (lon, lat) | 56行 | KV 1ms |
| lat filter | → 55行 | <1ms |
| index join (pkey fetch) | 55行 | KV 10ms |
| category filter | → 51行 | <1ms |

---

## 中bbox（約8km × 11km）

**座標**: `lon BETWEEN 134.00 AND 134.08 AND lat BETWEEN 34.30 AND 34.40`

```
planning time: 4ms
execution time: 23ms
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 1,748 (152 KiB, 1,748 KVs, 5 gRPC calls)
cumulative time spent in KV: 19ms
maximum memory usage: 5.0 MiB
sql cpu time: 1ms

• limit (count: 500)
└── • filter
    │ actual row count: 500
    │ execution time: 271µs
    │ estimated row count: 313
    │ filter: (category conditions)
    └── • index join (streamer)
        │ actual row count: 780
        │ KV time: 18ms
        │ KV rows decoded: 780
        │ KV gRPC calls: 4
        │ table: y_junctions@y_junctions_pkey
        └── • filter
            │ actual row count: 780
            │ filter: (lat >= 34.30) AND (lat <= 34.40)
            └── • scan
                  actual row count: 802
                  KV time: 1ms
                  KV gRPC calls: 1
                  table: y_junctions@idx_y_junctions_lon_lat
                  spans: [/134.00/34.30 - /134.08/34.40]
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| B-tree index scan (lon, lat) | 802行 | KV 1ms |
| lat filter | → 780行 | <1ms |
| index join (pkey fetch) | 780行 | KV 18ms |
| category filter | → 500行 | <1ms |

---

## 大bbox（約28km × 44km）

**座標**: `lon BETWEEN 133.90 AND 134.20 AND lat BETWEEN 34.20 AND 34.60`

```
planning time: 5ms
execution time: 94ms
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 11,257 (981 KiB, 11,749 KVs, 5 gRPC calls)
cumulative time spent in KV: 79ms
maximum memory usage: 5.0 MiB
sql cpu time: 6ms

• limit (count: 500)
└── • filter
    │ actual row count: 500
    │ execution time: 163µs
    │ estimated row count: 575
    │ filter: (category conditions)
    └── • index join (streamer)
        │ actual row count: 1,534
        │ KV time: 73ms
        │ KV rows decoded: 1,534
        │ KV bytes read: 478 KiB
        │ KV gRPC calls: 3
        │ table: y_junctions@y_junctions_pkey
        └── • filter
            │ actual row count: 4,288
            │ filter: (lat >= 34.2) AND (lat <= 34.6)
            └── • scan
                  actual row count: 9,723
                  KV time: 6ms
                  KV rows decoded: 9,723
                  KV bytes read: 503 KiB
                  KV gRPC calls: 2
                  estimated row count: 8,051 - 9,256 (1.2% of the table)
                  table: y_junctions@idx_y_junctions_lon_lat
                  spans: [/133.9/34.2 - /134.2/34.6]
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| B-tree index scan (lon, lat) | 9,723行 | KV 6ms |
| lat filter | → 4,288行 | <1ms |
| index join (pkey fetch) | 1,534行 | KV 73ms |
| category filter | → 500行 | <1ms |

---

## ベースラインとの比較

| bbox | ベースライン | アフター | 改善率 | プラン変化 |
|------|------------|---------|-------|-----------|
| 小（2km） | 12ms | 13ms | ≒同等 | 空間インデックス → B-tree (lon, lat) |
| 中（10km） | 24ms | 23ms | ≒同等 | 空間インデックス → B-tree (lon, lat) |
| **大（30km）** | **1,200ms** | **94ms** | **13x 高速化** | **FULL SCAN → B-tree (lon, lat)** |

### 改善の要点

**大bbox**:
- ベースライン: 266,732行をディスクから読み取り（primary key の ID 順フルスキャン）
- アフター: 9,723行のみ読み取り（lon 範囲でインデックスをポイントスキャン）
- 読み取り量: 81 MiB → 981 KiB（約85分の1）

**小〜中bbox**:
- ベースラインでも空間インデックスが使用されており大きな差はなし
- index join ボトルネックは引き続き存在（pkey 2段目ルックアップ）

### 残課題

- index join による2段目ルックアップが小〜中bboxのボトルネック
  - `(lon, lat)` インデックスは座標しか持たないため、他カラム取得に primary key lookup が必要
  - COVERING INDEX（全カラムをインデックスに含める）で解消可能だが、インデックスサイズが大幅増加
- 大bboxではカテゴリフィルタ後に 1,534行 → 500行なので、さらなる絞り込みは困難
