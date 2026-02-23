# EXPLAIN ANALYZE ベースライン（改善前）

**計測日**: 2026-02-23
**データ件数**: 743,318件
**計測条件**: コールドキャッシュ（docker restart 後の初回実行）
**クエリ**: カテゴリフィルタあり（local OR pedestrian）、LIMIT 500

---

## 小bbox（約2km × 2km）

**座標**: `ST_MakeEnvelope(134.04, 34.33, 134.06, 34.35, 4326)`

```
planning time: 91ms
execution time: 12ms
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 435 (39 KiB, 435 KVs, 6 gRPC calls)
cumulative time spent in KV: 10ms
maximum memory usage: 230 KiB
sql cpu time: 1ms

• render
└── • limit (count: 500)
    └── • filter
        │ actual row count: 51
        │ execution time: 675µs
        │ estimated row count: 81,022  ← 統計が古く見積もりが大幅にずれ
        │ filter: st_intersects(...) AND (category conditions)
        └── • index join (streamer)
            │ actual row count: 55
            │ KV time: 9ms
            │ KV rows decoded: 55
            │ KV gRPC calls: 5
            │ table: y_junctions@y_junctions_pkey
            └── • inverted filter
                │ actual row count: 55
                └── • scan
                      actual row count: 380
                      KV time: 2ms
                      KV gRPC calls: 1
                      table: y_junctions@idx_y_junctions_location
                      WARNING: row count estimate is inaccurate, consider running ANALYZE
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| 空間インデックス scan | 380 S2セル | KV 2ms |
| inverted filter | → 55行 | <1ms |
| index join (pkey fetch) | 55行 | KV 9ms ← ボトルネック |
| filter (ST_Intersects + category) | → 51行 | <1ms |

---

## 中bbox（約8km × 11km）

**座標**: `ST_MakeEnvelope(134.00, 34.30, 134.08, 34.40, 4326)`

```
planning time: 4ms
execution time: 24ms
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 1,782 (249 KiB, 1,782 KVs, 7 gRPC calls)
cumulative time spent in KV: 15ms
maximum memory usage: 1.1 MiB
sql cpu time: 10ms

• render
└── • limit (count: 500)
    └── • filter
        │ actual row count: 500
        │ execution time: 5ms
        │ estimated row count: 81,022
        │ filter: st_intersects(...) AND (category conditions)
        └── • index join (streamer)
            │ actual row count: 606
            │ KV time: 14ms
            │ KV rows decoded: 606
            │ KV gRPC calls: 6
            │ table: y_junctions@y_junctions_pkey
            └── • inverted filter
                │ actual row count: 606
                └── • scan
                      actual row count: 1,176
                      KV time: 1ms
                      KV gRPC calls: 1
                      table: y_junctions@idx_y_junctions_location
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| 空間インデックス scan | 1,176 S2セル | KV 1ms |
| inverted filter | → 606行 | 4ms |
| index join (pkey fetch) | 606行 | KV 14ms ← ボトルネック |
| filter (ST_Intersects + category) | → 500行 | 5ms |

---

## 大bbox（約28km × 44km）

**座標**: `ST_MakeEnvelope(133.90, 34.20, 134.20, 34.60, 4326)`

```
planning time: 45ms
execution time: 1.2s
distribution: local
vectorized: true
plan type: custom
rows decoded from KV: 266,732 (81 MiB, 270,365 KVs, 9 gRPC calls)
cumulative time spent in KV: 917ms
maximum memory usage: 11 MiB
sql cpu time: 693ms

• render
└── • limit (count: 500)
    └── • filter
        │ actual row count: 500
        │ execution time: 250ms
        │ estimated row count: 81,022
        │ filter: st_intersects(...) AND (category conditions)
        └── • scan
              actual row count: 266,732
              KV time: 917ms  ← 支配的
              KV rows decoded: 266,732
              KV bytes read: 81 MiB
              KV gRPC calls: 9
              estimated row count: 4,588 - 743,318 (100% of the table)
              table: y_junctions@y_junctions_pkey
              spans: FULL SCAN (SOFT LIMIT)  ← 空間インデックス不使用
```

**コスト内訳**:
| ステップ | 行数 | 時間 |
|---------|------|------|
| **フルスキャン (pkey)** | **266,732行** | **KV 917ms** |
| filter (ST_Intersects + category) | → 500行 | 250ms |

---

## 総括

| bbox | 実行時間 | プラン | 問題点 |
|------|---------|-------|-------|
| 小（2km） | 12ms | 空間インデックス → index join | index join (9ms) がボトルネック |
| 中（10km） | 24ms | 空間インデックス → index join | index join (14ms) がボトルネック |
| **大（30km）** | **1.2s** | **FULL SCAN** | S2 covering が広すぎて空間インデックス不使用、81MB読み取り |

### 根本的な問題
- **大bbox**: S2セルの covering がテーブル大半をカバーする規模になるとオプティマイザが空間インデックスを諦め、プライマリキーのフルスキャンに切り替える。266,732行を読んで500行を返す（533:1のオーバーヘッド）。
- **小〜中bbox**: 空間インデックスは使われているが、inverted index が `(id, location_inverted_key)` しか持たないため、他のカラム取得に index join（プライマリキーへの2段目ルックアップ）が必要になる。

### 統計の問題
- `estimated row count: 81,022` は実際の 51〜606 行に対して大幅にずれている
- `ANALYZE y_junctions` が推奨されている（stats collected 2 days ago）
