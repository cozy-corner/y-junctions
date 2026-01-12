# Y-Junction データベース正規化計画

## 背景

### 現状の問題

2-way junction検出機能の実装時に、以下の設計上の問題が明らかになった：

1. **非正規化スキーマの制約**
   - 現在のスキーマは way_1, way_2, way_3 の固定3カラムを持つ
   - 2-way junctionは2本のwayしか持たないため、way_3をNULLにする必要がある
   - 論理的に2本しかないのに3本分のカラムを用意するのは不自然

2. **データの重複**
   - 2-way junctionでは、way_1とway_2が同じOSM wayを指す
   - way_1_highway_type == way_2_highway_type のような重複が発生
   - way_1_bridge == way_2_bridge などの冗長なデータ

3. **拡張性の問題**
   - 将来4-way以上のjunctionを扱う場合、さらにカラム追加が必要
   - スキーマ変更のたびに大規模なマイグレーションが必要

## 正規化後の設計

### テーブル構造

#### 1. junctions テーブル（主テーブル）

```sql
CREATE TABLE junctions (
    id SERIAL PRIMARY KEY,
    osm_node_id BIGINT NOT NULL UNIQUE,
    location GEOGRAPHY(POINT, 4326) NOT NULL,

    -- 角度情報（3方向固定）
    angle_1 INTEGER NOT NULL,
    angle_2 INTEGER NOT NULL,
    angle_3 INTEGER NOT NULL,
    bearings REAL[3] NOT NULL,

    -- 標高情報
    elevation REAL,
    neighbor_elevation_1 REAL,
    neighbor_elevation_2 REAL,
    neighbor_elevation_3 REAL,
    elevation_diff_1 REAL,
    elevation_diff_2 REAL,
    elevation_diff_3 REAL,
    min_angle_index SMALLINT,
    min_elevation_diff REAL,
    max_elevation_diff REAL,

    -- メタ情報
    junction_type VARCHAR(10) NOT NULL DEFAULT 'three_way', -- 'three_way' or 'two_way'
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_junctions_location ON junctions USING GIST(location);
CREATE INDEX idx_junctions_type ON junctions(junction_type);
```

#### 2. junction_ways テーブル（正規化されたway情報）

```sql
CREATE TABLE junction_ways (
    id SERIAL PRIMARY KEY,
    junction_id INTEGER NOT NULL REFERENCES junctions(id) ON DELETE CASCADE,

    -- Way情報
    osm_way_id BIGINT NOT NULL,
    direction_index SMALLINT NOT NULL, -- 0, 1, or 2 (bearings配列のインデックスに対応)

    -- Way属性
    highway_type VARCHAR(50) NOT NULL,
    bridge BOOLEAN NOT NULL DEFAULT FALSE,
    tunnel BOOLEAN NOT NULL DEFAULT FALSE,

    -- 生成カラム（カテゴリ）
    category VARCHAR(20) GENERATED ALWAYS AS (
        CASE
            WHEN highway_type IN ('motorway', 'motorway_link', 'trunk', 'trunk_link') THEN 'highway'
            WHEN highway_type IN ('primary', 'primary_link', 'secondary', 'secondary_link', 'tertiary', 'tertiary_link') THEN 'major'
            WHEN highway_type IN ('residential', 'unclassified', 'service', 'living_street') THEN 'local'
            WHEN highway_type IN ('pedestrian', 'footway', 'path', 'steps', 'cycleway') THEN 'pedestrian'
            ELSE 'local'
        END
    ) STORED,

    UNIQUE(junction_id, direction_index)
);

CREATE INDEX idx_junction_ways_junction ON junction_ways(junction_id);
CREATE INDEX idx_junction_ways_highway_type ON junction_ways(highway_type);
CREATE INDEX idx_junction_ways_category ON junction_ways(category);
CREATE INDEX idx_junction_ways_osm_way ON junction_ways(osm_way_id);
```

### データの対応関係

#### 3-way junction の例
```
現在:
  way_1_highway_type = 'primary', way_1_bridge = true
  way_2_highway_type = 'residential', way_2_bridge = false
  way_3_highway_type = 'footway', way_3_bridge = false

正規化後:
  junction_ways: [
    { direction_index: 0, highway_type: 'primary', bridge: true },
    { direction_index: 1, highway_type: 'residential', bridge: false },
    { direction_index: 2, highway_type: 'footway', bridge: false }
  ]
```

#### 2-way junction の例
```
現在（問題あり）:
  way_1_highway_type = 'primary', way_1_bridge = true
  way_2_highway_type = 'primary', way_2_bridge = true  ← 重複
  way_3_highway_type = 'footway', way_3_bridge = false

正規化後（重複解消）:
  junction_ways: [
    { direction_index: 0, osm_way_id: 12345, highway_type: 'primary', bridge: true },
    { direction_index: 1, osm_way_id: 12345, highway_type: 'primary', bridge: true },
    { direction_index: 2, osm_way_id: 67890, highway_type: 'footway', bridge: false }
  ]
```

## パフォーマンス影響の分析

### 1. インポート処理

**現在の実装:**
```rust
// 1000件バッチで、1テーブルに28パラメータ×1000行を一括INSERT
INSERT INTO y_junctions (...28 columns...) VALUES (...), (...), ...
```

**正規化後:**
```rust
// 1000件バッチで、2テーブルに分けて一括INSERT
INSERT INTO junctions (...10 columns...) VALUES (...), (...), ...
INSERT INTO junction_ways (...5 columns...) VALUES (...), (...), ...  // 約2500行
```

**影響見積もり:**
- **DB挿入処理のみ**: 1.2〜1.3倍遅延（20〜30%増）
- **インポート全体**: 1.05〜1.1倍程度（5〜10%増）
  - PBFパース処理が大部分の時間を占めるため
- **100万件で**: 200秒 → 210〜220秒程度

**遅延要因:**
- 外部キー制約チェック
- 2テーブルへのインデックス更新

**高速化要因:**
- 生成カラム（way_1/2/3_category）が不要（3カラム → 1カラム）

### 2. 検索クエリ

**現在（非正規化）:**
```sql
SELECT * FROM y_junctions
WHERE way_1_highway_type = 'primary'
   OR way_2_highway_type = 'primary'
   OR way_3_highway_type = 'primary';
-- 実行時間: ~0.15ms (インデックス使用不可)
```

**正規化後:**
```sql
SELECT j.* FROM junctions j
JOIN junction_ways jw ON j.id = jw.junction_id
WHERE jw.highway_type = 'primary';
-- 実行時間: ~0.55ms (インデックス使用可能、ただしJOIN必要)
```

**影響見積もり:** 3〜4倍遅延（0.15ms → 0.55ms）
- ただし、実際のアプリケーションではネットワーク遅延が支配的
- ユーザー体感への影響は無視できるレベル

### 3. 単一Junction取得

**影響見積もり:** 約5倍遅延（0.01ms → 0.05ms）
- 絶対時間は依然として非常に小さい

### 4. 統計クエリ

**影響見積もり:** **2倍高速化**（500ms → 200ms）
- GROUP BY が効率的に実行可能
- 集計用インデックスが有効活用される

## マイグレーション計画

### Phase 1: 新スキーマの追加（互換性維持）

1. 新テーブル作成
   ```sql
   CREATE TABLE junctions (...);
   CREATE TABLE junction_ways (...);
   ```

2. データ移行スクリプト作成
   ```sql
   -- y_junctions → junctions へデータコピー
   -- way_1/2/3 → junction_ways へ正規化して挿入
   ```

3. 二重書き込みの実装
   - 新規インポート時に両スキーマに書き込み
   - 旧スキーマとの整合性を保証

### Phase 2: アプリケーション移行

1. Repository層の修正
   - `find_junctions()` を新スキーマ対応に
   - テストの更新

2. API層の動作確認
   - 既存のAPI仕様を維持
   - レスポンス形式は変更なし

3. フロントエンドの動作確認
   - APIレスポンスが変わらないため影響なし

### Phase 3: 旧スキーマの削除

1. 二重書き込みの停止
2. 旧テーブル（y_junctions）の削除
3. マイグレーション完了

### 推定作業期間

- Phase 1: 1〜2日（マイグレーション、テスト）
- Phase 2: 2〜3日（コード修正、テスト）
- Phase 3: 0.5日（削除、確認）
- **合計: 4〜6日**

## メリット・デメリット

### メリット

1. **論理的整合性**
   - 2-way junctionでway_3がNULLになる不自然さの解消
   - データの重複がなくなる

2. **拡張性**
   - 将来4-way以上のjunctionに対応可能
   - カラム追加不要

3. **統計クエリの高速化**
   - highway_type別集計が2倍高速化

4. **保守性**
   - スキーマが理解しやすい
   - データの更新が容易

### デメリット

1. **検索クエリの遅延**
   - 単純なSELECTが3〜4倍遅延
   - ただし絶対時間は小さい（0.15ms → 0.55ms）

2. **インポート処理の遅延**
   - 全体で5〜10%程度の遅延
   - 100万件で10〜20秒の増加

3. **実装コストと移行リスク**
   - マイグレーション作業が必要
   - データ移行時のバグリスク

## 代替案の検討

### 案1: 正規化（本計画）

- **採用理由**: 長期的な保守性と拡張性
- **適用場面**: プロジェクトが長期運用される場合

### 案2: way_3のみNULL許可（最小変更）

```sql
ALTER TABLE y_junctions ALTER COLUMN way_3_highway_type DROP NOT NULL;
ALTER TABLE y_junctions ALTER COLUMN way_3_bridge TYPE BOOLEAN,
    ALTER COLUMN way_3_bridge DROP DEFAULT;
-- 同様にway_3_tunnel, way_3_categoryも変更
```

- **メリット**: 実装が簡単、パフォーマンス影響なし
- **デメリット**: way_1とway_2の重複は解消されない、4-way非対応
- **適用場面**: 短期的な対応、プロトタイプ段階

### 案3: 2-way検出を実装しない

- **メリット**: 現状維持、リスクゼロ
- **デメリット**: Shibuya Y-junctionのような重要な地点が検出されない
- **適用場面**: 2-way junctionの重要性が低い場合

## 推奨事項

### 短期的対応（Phase 1実装中）

**案2（way_3のみNULL許可）を採用**

理由:
- 2-way junction検出機能をすぐに使える
- 実装リスクが低い
- パフォーマンス影響なし
- 正規化は後からでも可能

### 長期的対応（Phase 2以降）

**案1（正規化）への移行を検討**

判断基準:
- ✅ 4-way以上のjunction対応が必要になった場合
- ✅ 統計クエリが頻繁に実行され、パフォーマンスが問題になった場合
- ✅ データの保守・更新が複雑になってきた場合
- ❌ 現状のパフォーマンスで十分な場合は見送り

## 結論

**現時点での推奨**: 案2（way_3のみNULL許可）

1. まず最小変更でway_3をNULL許可にする
2. 2-way junction検出を実装・リリース
3. 実運用でのデータとパフォーマンスを観察
4. 必要に応じて正規化を検討

この段階的アプローチにより、リスクを最小化しながら機能を提供できる。
