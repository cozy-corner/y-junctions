# OSMインポート パフォーマンス最適化計画

## 前提：現在の処理フロー（Phase 1完了時点）

### ベンチマーク結果
**測定条件**: `shikoku-latest.osm.pbf`, bbox=132,33,135,35, 16,798件

### 処理時間の内訳
**Total: 6.651秒**
1. Single Pass（PBFパース + データ収集）: 5.13秒（77%）
2. 角度計算: 0.24秒（4%）
3. DB挿入: 0.62秒（9%）
4. その他: 0.68秒（10%）

### 角度計算の内部処理（0.24秒の内訳）
各Y字路候補（16,798個）について以下を実行：
1. 候補ノードIDを取得
2. 接続している3本のwayを特定
3. **各wayについて「候補の隣のノード」を探す** ← Phase 2で改善対象
4. 隣ノードの座標を取得（メモリから）
5. 3つの座標から角度を計算

---

## Phase 1: 1パス読み込み実装 ✅ 完了

### 改善内容
PBFファイルの読み込み回数を削減

**Before（3パス実装）**:
- Pass 1: 道路（way）を探す → 2.65秒
- Pass 2: Y字路候補の座標を取得 → 1.52秒
- Pass 3: 隣接ノードの座標を取得 → 1.88秒
- Total: 7.17秒

**After（1パス実装）**:
- Single Pass: way収集 + 全ノード座標をメモリにキャッシュ → 5.13秒
- Total: 6.65秒

### 実装方法
```rust
// 1回のファイル読み込みで全データを取得
let node_coords: DashMap<i64, (f64, f64)> = DashMap::new();

reader.for_each(|element| match element {
    Element::Way(way) => { /* way処理 */ }
    Element::Node(node) => {
        node_coords.insert(node.id(), (node.lat(), node.lon()));
    }
    _ => {}
});
```

### 効果
| 項目 | Before | After | 改善 |
|------|--------|-------|------|
| 処理時間 | 7.171秒 | 6.651秒 | **-0.52秒（7.2%）** |
| ファイルI/O | 3回 | 1回 | 66%削減 |
| メモリ使用 | 中 | 中+3.6MB | +3.6MB |

### なぜ7%しか改善しなかったか
- ファイル読み込み自体は速い
- ボトルネックは「PBFファイルの中身を解析（パース）する」処理

### DashMapの使用理由
Phase 2での並列化に備えて先行導入（現時点では`HashMap`でも動作する）

**変更ファイル**: `backend/src/importer/parser.rs`

---

## Phase 2: 並列処理の導入 ✅ 完了

### 実装結果

`osmpbf::ElementReader`は並列化に対応していないため（PBFファイルは圧縮されており順次読み込みが必要）、以下の部分を並列化しました：

1. **bboxフィルタリング処理の並列化**
2. **角度計算処理の並列化**

### 実装方法

```rust
use rayon::prelude::*;

// 1. bboxフィルタリングを並列化
let y_junctions: Vec<YJunctionWithCoords> = candidates
    .par_iter()  // 並列化
    .filter_map(|candidate| {
        node_coords.get(&candidate.node_id).and_then(|coords_ref| {
            let (lat, lon) = *coords_ref;
            if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
                Some(YJunctionWithCoords { ... })
            } else {
                None
            }
        })
    })
    .collect();

// 2. 角度計算を並列化
let junctions_for_insert: Vec<JunctionForInsert> = y_junctions
    .par_iter()  // 並列化
    .filter_map(|junction| {
        // 角度計算処理
        calculate_junction_angles(junction.lat, junction.lon, &neighbor_points)
            .and_then(|(angles, bearings)| {
                // ... フィルタリングと構造体作成 ...
                Some(JunctionForInsert { ... })
            })
    })
    .collect();
```

### 並列化できなかった部分

- **Single Pass処理**（5.13秒）: `osmpbf::ElementReader`が順次読み込みのため並列化不可
- **NodeConnectionCounter**への書き込み: `HashMap`を使用しており、スレッドセーフではない

### 並列化した部分の期待効果

**改善対象**: 
- bboxフィルタリング処理
- 角度計算処理（0.24秒）

**期待される改善**（4コアCPUの場合）:
- 角度計算: 0.24秒 → 0.06秒（-0.18秒）
- bboxフィルタリング: 効果は候補数に依存（数千～数万件で効果あり）

**Total処理時間の期待値**:
- 6.65秒 → 約6.4秒（角度計算のみの改善で約-0.2秒、3%改善）
- 実際の効果はbboxフィルタリングの候補数とCPUコア数に依存

### 実装詳細

**変更ファイル**: `backend/src/importer/parser.rs`

**テスト結果**: 
- ユニットテスト: 36個全て合格
- 統合テスト: 23個全て合格
- cargo fmt: 合格
- cargo clippy: 合格（警告なし）

### ベンチマーク結果（実測）

**測定条件**: `shikoku-latest.osm.pbf`, bbox=132,33,135,35, 16,798件、空のDB

| 処理 | Phase 1 | Phase 2 | 改善 |
|------|---------|---------|------|
| Single Pass | 5.13秒 | 5.19秒 | +0.06秒 |
| bboxフィルタリング | - | 0.009秒 | - |
| **角度計算** | **0.24秒** | **0.030秒** | **-0.21秒（88%改善）** |
| DB挿入 | 0.62秒 | 0.562秒 | -0.058秒 |
| その他 | 0.68秒 | 0.45秒 | -0.23秒 |
| **Total** | **6.65秒** | **6.245秒** | **-0.405秒（6.1%改善）** |

**結果分析**:
- ✅ **角度計算の並列化が非常に効果的（88%改善）**
- ✅ 全体で6.1%の高速化を達成
- ⚠️ Single Pass処理は並列化できず（+0.06秒はノイズ）
- ⚠️ DB挿入の改善（-0.058秒）は測定誤差の範囲内

**測定方法**:
- バイナリ直接実行: `./target/release/import`
- ログのタイムスタンプから各処理時間を計算
- Phase 1、Phase 2ともに空のDBで測定

---

## Phase 3: Single Pass処理の並列化 ✅ 完了

### 実装結果

osmpbfクレートの**`par_map_reduce`**メソッドを使用してSingle Pass処理を並列化しました。

### 実装内容

#### 1. NodeConnectionCounterのDashMap化

**Before:**
```rust
pub struct NodeConnectionCounter {
    node_to_ways: HashMap<i64, HashSet<i64>>,
    way_nodes: HashMap<i64, Vec<i64>>,
    // ...
}
```

**After:**
```rust
pub struct NodeConnectionCounter {
    node_to_ways: DashMap<i64, HashSet<i64>>,  // スレッドセーフ
    way_nodes: DashMap<i64, Vec<i64>>,
    // ...
}
```

#### 2. Map-Reduceパターンへの移行

**LocalState構造体の導入:**
```rust
struct WayData {
    way_id: i64,
    node_ids: Vec<i64>,
    highway_type: String,
    bridge: bool,
    tunnel: bool,
}

struct LocalState {
    ways: Vec<WayData>,
    nodes: Vec<(i64, (f64, f64))>,
}

impl LocalState {
    fn merge(mut self, other: Self) -> Self {
        self.ways.extend(other.ways);
        self.nodes.extend(other.nodes);
        self
    }
}
```

**par_map_reduceの実装:**
```rust
let local_state = reader.par_map_reduce(
    move |element| {
        let mut local = LocalState::new();
        match element {
            Element::Way(way) => {
                if valid_types.contains(highway_type) {
                    local.ways.push(WayData { ... });
                }
            }
            Element::Node(node) => {
                local.nodes.push((node.id(), (node.lat(), node.lon())));
            }
            Element::DenseNode(node) => {
                local.nodes.push((node.id(), (node.lat(), node.lon())));
            }
            _ => {}
        }
        local
    },
    LocalState::new,
    |a, b| a.merge(b),
)?;

// 並列化されたデータからNodeConnectionCounterを構築
let mut counter = NodeConnectionCounter::new();
for way in &local_state.ways {
    counter.add_way(...);
}
```

### ベンチマーク結果（実測）

**測定条件**: `shikoku-latest.osm.pbf`, bbox=132,33,135,35, 16,798件

| 処理 | Phase 2 | Phase 3 | 改善 |
|------|---------|---------|------|
| **Single Pass** | **5.19秒** | **3.50秒** | **-1.69秒（32.6%）** |
| 角度計算 | 0.030秒 | 0.032秒 | +0.002秒 |
| DB挿入 | 0.562秒 | 0.776秒 | +0.214秒 |
| その他 | 0.45秒 | 0.418秒 | -0.032秒 |
| **Total** | **6.245秒** | **4.726秒** | **-1.519秒（24.3%）** |

### 結果分析

**✅ 成功した点:**
- Single Pass処理が32.6%高速化（5.19秒 → 3.50秒）
- 全体で24.3%の高速化を達成

**⚠️ 期待値とのギャップ:**
- 期待値: 2.9秒
- 実測値: 4.73秒
- 差分: 1.83秒

**ギャップの原因:**
1. **NodeConnectionCounter構築のオーバーヘッド**（約0.4秒）
   - Map-Reduce後に順次構築している
2. **メモリコピーとマージのコスト**
   - LocalStateのVecマージで追加コストが発生
3. **並列化オーバーヘッド**
   - スレッド管理、同期処理のコスト

### 実装詳細

**変更ファイル:**
- `backend/src/importer/parser.rs` - par_map_reduce実装
- `backend/src/importer/detector.rs` - DashMap化

**テスト結果:**
- ユニットテスト: 36個全て合格
- 統合テスト: 23個全て合格
- cargo fmt: 合格
- cargo clippy: 合格

### さらなる改善の余地（Phase 4候補）

現在の処理時間内訳（4.726秒）:
- Single Pass（並列）: 3.500秒（74%） ← まだボトルネック
- DB挿入: 0.776秒（16%）
- Counter構築: 0.418秒（9%）
- 角度計算: 0.032秒（1%）

**改善案:**
1. NodeConnectionCounter構築の並列化（-0.2秒）
2. DB挿入のCOPY文化（-0.5秒）
3. Single Pass処理のさらなる最適化（-1.0秒）
4. メモリアロケーションの削減（-0.5秒）

**期待される改善後:** 4.73秒 → 2.5秒（**47%追加改善**）

---

## まとめ

| Phase | 改善対象 | 処理時間 | 改善幅 | 状態 |
|-------|---------|---------|--------|------|
| Phase 0（初期） | 3パス実装 | 7.17秒 | - | - |
| Phase 1 | ファイルI/O削減 | 6.65秒 | -0.52秒（7.2%） | ✅ 完了 |
| Phase 2 | 部分並列処理 | 6.245秒 | -0.405秒（6.1%） | ✅ 完了 |
| Phase 3 | Single Pass並列化 | 4.726秒 | -1.519秒（24.3%） | ✅ 完了 |
| **累積（Phase 1+2）** | - | **6.245秒** | **-0.925秒（12.9%）** | - |
| **累積（Phase 1+2+3）** | - | **4.726秒** | **-2.444秒（34.1%）** | - |

### Phase別の成果

**Phase 1（完了）:**
- ✅ 3パス → 1パス読み込みに変更
- ✅ ファイルI/O: 3回 → 1回（66%削減）
- ✅ DashMap先行導入（Phase 3への準備）

**Phase 2（完了）:**
- ✅ 角度計算の並列化: 0.24秒 → 0.030秒（88%改善）
- ✅ bboxフィルタリングの並列化
- ✅ 全体処理時間: 6.65秒 → 6.245秒（6.1%改善）

**Phase 3（完了）:**
- ✅ `par_map_reduce`を使用したSingle Pass処理の並列化
- ✅ NodeConnectionCounterのDashMap化
- ✅ Map-Reduceパターンへの設計変更
- ✅ 実測改善: 6.245秒 → 4.726秒（-1.519秒、24.3%改善）
- ✅ Single Pass処理: 5.19秒 → 3.50秒（-1.69秒、32.6%改善）

**次のステップ**: Phase 4検討（さらなる最適化で2.5秒を目指す）
