# 道路種別（highway_type）とカテゴリ分類 追加機能 実装計画

## 概要

Y字路データに各道路の種別（highway_type）とカテゴリを追加し、APIで検索できるようにする。

**実装方針**: 3つのPRに分割
- PR1: データ保存機能（Backend）
- PR2: 検索機能（Backend）
- PR3: UI実装（Frontend）

## 要件

- **highway_type**: 3つのカラム（way_1_highway_type, way_2_highway_type, way_3_highway_type）
- **category**: 3つのGenerated Column（PostgreSQLが自動計算、STORED）
- **カテゴリ分類**: highway / major / local / pedestrian の4段階
  - highway: motorway, trunk, motorway_link, trunk_link
  - major: primary, secondary, tertiary, primary_link, secondary_link, tertiary_link
  - local: residential, unclassified, service
  - pedestrian: steps, pedestrian, path
- **API検索**: highway_typeとcategoryでフィルタリング可能（OR条件）
- **UI検索**: チェックボックスで選択したカテゴリを1本でも含むY字路を表示

---

# PR1: データ保存機能

## ゴール
highway_typeをDBに保存できるようにする。categoryはPostgreSQLが自動生成する。

## 成果物

### 1. Migration (新規作成)
**ファイル**: `backend/migrations/005_add_highway_type_and_category.sql`

- 3つのhighway_typeカラム追加（VARCHAR(50)）
- 3つのcategoryカラム追加（VARCHAR(20) GENERATED ALWAYS AS ... STORED）
- インデックス作成

```sql
ALTER TABLE y_junctions
ADD COLUMN way_1_highway_type VARCHAR(50),
ADD COLUMN way_2_highway_type VARCHAR(50),
ADD COLUMN way_3_highway_type VARCHAR(50);

ADD COLUMN way_1_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_1_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_1_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_1_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_1_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED,
ADD COLUMN way_2_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_2_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_2_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_2_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_2_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED,
ADD COLUMN way_3_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_3_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_3_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_3_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_3_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED;

CREATE INDEX idx_y_junctions_highway_types ON y_junctions (way_1_highway_type, way_2_highway_type, way_3_highway_type);
CREATE INDEX idx_y_junctions_categories ON y_junctions (way_1_category, way_2_category, way_3_category);
```

### 2. Detector - データ構造拡張
**ファイル**: `backend/src/importer/detector.rs`

**WayTagInfo拡張** (Line 4-9):
```rust
pub struct WayTagInfo {
    pub bridge: bool,
    pub tunnel: bool,
    pub highway_type: String,  // NEW
}
```

**JunctionForInsert拡張** (Line 29-61):
```rust
pub struct JunctionForInsert {
    // ... existing fields ...
    pub way_1_highway_type: String,  // NEW
    pub way_2_highway_type: String,  // NEW
    pub way_3_highway_type: String,  // NEW
}
```

**NodeConnectionCounter修正** (Line 172-192):
- `add_way()`: WayTagInfoにhighway_typeを含める

### 3. Parser - highway_type抽出
**ファイル**: `backend/src/importer/parser.rs`

**JunctionForInsert生成時に追加** (Line 226-252):
```rust
let way_1_highway_type = way_tags[0].highway_type.clone();
let way_2_highway_type = way_tags[1].highway_type.clone();
let way_3_highway_type = way_tags[2].highway_type.clone();
```

### 4. Inserter - INSERT文拡張
**ファイル**: `backend/src/importer/inserter.rs`

- PARAMS_PER_ROW: 25 → 28
- INSERT文に3カラム追加
- バインディング追加

## 完了条件
- マイグレーション実行成功
- データインポート後、highway_typeとcategoryが保存されている
- categoryがhighway_typeから自動生成されている
- 全テスト合格（cargo test, fmt, clippy）

---

# PR2: 検索機能

## ゴール
highway_typeとcategoryでY字路を検索できるようにする。

## 前提条件
PR1がマージ済みで、DBにhighway_typeとcategoryが保存されている状態。

## 成果物

### 1. Repository - フィルタ機能追加
**ファイル**: `backend/src/db/repository.rs`

**FilterParams拡張** (Line 24-33):
```rust
pub struct FilterParams {
    pub angle_types: Option<Vec<AngleType>>,
    pub exclude_bridge_tunnel: bool,
    pub highway_types: Option<Vec<String>>,  // NEW
    pub categories: Option<Vec<String>>,     // NEW
}
```

**フィルタ関数追加** (Line 179以降):
```rust
fn add_highway_type_filter(builder: &mut QueryBuilder<Postgres>, highway_types: &[String]) {
    // OR条件: いずれか1本でもhighway_typeが一致すればヒット
}

fn add_category_filter(builder: &mut QueryBuilder<Postgres>, categories: &[String]) {
    // OR条件: いずれか1本でもcategoryが一致すればヒット
}
```

### 2. API Handler - クエリパラメータ追加
**ファイル**: `backend/src/api/handlers.rs`

**JunctionsQuery拡張** (Line 51-61):
```rust
pub struct JunctionsQuery {
    // ... existing fields ...
    pub highway_type: Option<String>,  // カンマ区切り
    pub category: Option<String>,      // カンマ区切り
}
```

**to_filter_params修正** (Line 106-143):
- highway_typeとcategoryをパース（カンマ区切り→Vec）
- FilterParamsに含める

## 完了条件
- API: `?category=highway,major` で検索可能
- API: `?highway_type=motorway` で検索可能
- OR条件で動作（3本のうち1本でも該当すればヒット）
- 全テスト合格

---

# PR3: UI実装

## ゴール
UIから道路カテゴリでY字路を検索できるようにする。

## 前提条件
PR2がマージ済みで、APIがhighway_typeとcategoryパラメータに対応している状態。

## 成果物

### 1. Types - 型定義拡張
**ファイル**: `frontend/src/types/index.ts`

```typescript
export type RoadCategory = 'highway' | 'major' | 'local' | 'pedestrian';

export interface FilterParams {
    // ... existing fields ...
    category?: RoadCategory[];
}
```

### 2. Filters Hook - 状態管理
**ファイル**: `frontend/src/hooks/useFilters.ts`

```typescript
export interface FilterState {
    angleTypes: AngleType[];
    minAngleRange: [number, number];
    elevationDiffRange: [number, number];
    categories: RoadCategory[];  // NEW
}

const [categories, setCategories] = useState<RoadCategory[]>(['highway', 'major', 'local', 'pedestrian']);

const toggleCategory = useCallback((category: RoadCategory) => { /* ... */ }, []);
```

### 3. Filter Panel - UI追加
**ファイル**: `frontend/src/components/FilterPanel.tsx`

```tsx
const CATEGORY_LABELS: Record<RoadCategory, string> = {
  highway: '高速道路級',
  major: '主要道路',
  local: '生活道路',
  pedestrian: '歩道',
};

// 道路カテゴリフィルタセクション
<div className="filter-section">
  <h3>道路カテゴリ</h3>
  <p style={{ fontSize: 12, color: '#666' }}>
    選択したカテゴリを1本でも含むY字路を表示
  </p>
  <div className="category-options">
    {['highway', 'major', 'local', 'pedestrian'].map(category => (
      <label key={category}>
        <input type="checkbox" checked={categories.includes(category)} onChange={() => onToggleCategory(category)} />
        <span className="category-indicator" style={{ backgroundColor: CATEGORY_COLORS[category] }} />
        <span>{CATEGORY_LABELS[category]}</span>
      </label>
    ))}
  </div>
</div>
```

### 4. API Client - パラメータ送信
**ファイル**: `frontend/src/api/client.ts`

```typescript
if (filters?.category && filters.category.length > 0) {
  params.append('category', filters.category.join(','));
}
```

## 完了条件
- UIに道路カテゴリフィルタが表示される
- チェックボックスで選択/解除できる
- 地図上のY字路が適切にフィルタリングされる
- 全テスト合格（npm test, typecheck, lint）

---

# 実装順序

## Phase 1: PR1 - データ保存（Backend）
1. Migration作成・実行
2. WayTagInfo拡張
3. JunctionForInsert拡張
4. Parser修正
5. Inserter修正
6. テスト・PR作成

## Phase 2: データ再インポート（任意）
- 既存データにhighway_typeを追加したい場合は全データ再インポート

## Phase 3: PR2 - 検索機能（Backend）
1. FilterParams拡張
2. フィルタ関数追加
3. API Handler修正
4. テスト・PR作成

## Phase 4: PR3 - UI実装（Frontend）
1. Types拡張
2. useFilters Hook拡張
3. FilterPanel UI追加
4. API Client修正
5. テスト・PR作成

---

# PR間の依存関係

```
PR1 (Backend: データ保存)
  ↓
データ再インポート（任意）
  ↓
PR2 (Backend: 検索機能)
  ↓
PR3 (Frontend: UI実装)
```

---

# 注意事項

## Generated Columnについて
- カテゴリは**PostgreSQLが自動計算**する（Rustコードでは計算しない）
- INSERT時にcategoryカラムは指定しない（自動で入る）
- highway_typeを更新すると、categoryも自動で更新される

## OR条件の動作
- チェックON: そのカテゴリを1本でも含むY字路を表示
- 全部ON: 全Y字路を表示（フィルタなし）
- 一部OFF: ONにしたカテゴリのみで絞り込み
- 実用上、極端に異なるカテゴリの混在は稀なため、単純なOR条件で自然に動作する
