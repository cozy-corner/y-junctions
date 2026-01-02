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

**測定条件**: `shikoku-latest.osm.pbf`, bbox=132,33,135,35, 16,798件

| 処理 | Phase 1（並列化前） | Phase 2（並列化後） | 改善 |
|------|-------------------|-------------------|------|
| Single Pass | 5.13秒 | 5.27秒 | +0.14秒 |
| bboxフィルタリング | - | 0.009秒 | - |
| **角度計算** | **0.24秒** | **0.028秒** | **-0.21秒（88%改善）** |
| DB挿入 | 0.62秒 | 0.35秒 | -0.27秒（44%改善） |
| その他 | 0.68秒 | 0.35秒 | -0.33秒 |
| **Total** | **6.651秒** | **6.01秒** | **-0.64秒（9.6%改善）** |

**結果分析**:
- ✅ 角度計算の並列化が非常に効果的（88%改善）
- ✅ 全体で約10%の高速化を達成
- ✅ Single Pass処理は並列化できなかったが、他の部分で大幅改善

**期待値との比較**:
- 期待: 角度計算 0.24秒 → 0.06秒（-0.18秒）
- 実測: 角度計算 0.24秒 → 0.028秒（-0.21秒）
- **期待を上回る改善を達成！**

---

## Phase 3: Single Pass処理の並列化（未実装）

### 発見: osmpbfは並列処理に対応していた

Phase 2の実装後、osmpbfクレートが**`par_map_reduce`**メソッドで並列処理に対応していることが判明しました。

**現在の実装（Phase 2）:**
```rust
reader.for_each(|element| {
    // 順次処理（並列化不可）
    match element {
        Element::Way(way) => counter.add_way(...),
        Element::Node(node) => node_coords.insert(...),
    }
})?;
```

**目標の実装（Phase 3）:**
```rust
let results = reader.par_map_reduce(
    |element| {
        // 各blobを並列処理
        match element {
            Element::Way(way) => extract_way_data(way),
            Element::Node(node) => extract_node_data(node),
            _ => None,
        }
    },
    || LocalState::new(),  // 各スレッドの初期状態
    |state1, state2| state1.merge(state2)  // 結果をマージ
)?;
```

### 必要な変更

#### 1. NodeConnectionCounterのスレッドセーフ化

**現在の実装（スレッド非対応）:**
```rust
pub struct NodeConnectionCounter {
    node_to_ways: HashMap<i64, HashSet<i64>>,  // ← HashMap
    way_nodes: HashMap<i64, Vec<i64>>,
    // ...
}
```

**必要な実装（スレッド対応）:**
```rust
pub struct NodeConnectionCounter {
    node_to_ways: DashMap<i64, HashSet<i64>>,  // ← DashMap
    way_nodes: DashMap<i64, Vec<i64>>,
    // ...
}
```

#### 2. Map-Reduceパターンへの移行

**課題:**
- 現在の実装は「副作用型」（状態を変更する）
- par_map_reduceは「純粋関数型」（データを返す）が前提
- 設計パターンの根本的な変更が必要

**アプローチ:**
1. Map: 各要素から必要なデータを抽出
2. Reduce: 抽出したデータをマージして最終状態を構築

### 期待される効果

**改善対象**: Single Pass処理 5.27秒（全体の88%）

**理論値**（4コアCPUの場合）:
- 5.27秒 ÷ 4 = 1.32秒

**現実的な値**（並列化オーバーヘッド30%を考慮）:
- 5.27秒 ÷ (4 × 0.7) = 1.9秒
- **改善: 5.27秒 → 1.9秒（-3.4秒）**

**Total処理時間**:
- 6.01秒 → 2.6秒（**-3.4秒、57%改善**）

### 必要な作業量

| タスク | 工数 | 難易度 |
|--------|------|--------|
| NodeConnectionCounterをDashMap化 | 2-3時間 | 中 |
| detector.rsのテスト修正 | 1-2時間 | 低 |
| par_map_reduceの実装 | 2-3時間 | 中 |
| Map-Reduceパターンの設計 | 1-2時間 | 高 |
| 動作確認・デバッグ | 1-2時間 | 中 |
| **合計** | **7-12時間** | **中〜高** |

### 技術的課題

1. **状態管理の複雑さ**
   - 複数のHashMapを同時にマージする必要がある
   - データ競合を避けるための設計が必要

2. **メモリ効率**
   - 各スレッドが独立した状態を持つため、メモリ使用量が増加
   - 最終的なマージ時にメモリが倍増する可能性

3. **テストの複雑化**
   - 並列処理特有のバグ（レースコンディション等）のテストが必要

### 前提条件

- 4コアCPU以上を前提
- メモリ: 現在の1.5〜2倍程度必要（一時的）
- osmpbf 0.3以上

**変更ファイル**: 
- `backend/src/importer/parser.rs`
- `backend/src/importer/detector.rs`

### 今後の改善案（Phase 3以降）

Phase 3を実装しない場合の代替案：

1. **別のPBFパーサーへの移行**: より効率的なライブラリを使用
2. **メモリ効率の改善**: 不要なデータのキャッシュを削減
3. **データベース挿入の最適化**: COPY文やバルクインサートの改善

---

## まとめ

| Phase | 改善対象 | 処理時間 | 改善幅 | 状態 |
|-------|---------|---------|--------|------|
| Phase 0（初期） | 3パス実装 | 7.17秒 | - | - |
| Phase 1 | ファイルI/O削減 | 6.65秒 | -0.52秒（7.2%） | ✅ 完了 |
| Phase 2 | 部分並列処理 | 6.01秒 | -0.64秒（9.6%） | ✅ 完了 |
| Phase 3 | 全体並列処理 | 2.6秒（期待値） | -3.4秒（57%） | 📋 計画中 |
| **累積（Phase 1+2）** | - | **6.01秒** | **-1.16秒（16.2%）** | - |
| **累積（Phase 1+2+3）** | - | **2.6秒（期待値）** | **-4.6秒（64%）** | - |

### Phase別の成果

**Phase 1（完了）:**
- ✅ 3パス → 1パス読み込みに変更
- ✅ ファイルI/O: 3回 → 1回（66%削減）
- ✅ DashMap先行導入（Phase 3への準備）

**Phase 2（完了）:**
- ✅ 角度計算の並列化: 0.24秒 → 0.028秒（88%改善）
- ✅ bboxフィルタリングの並列化
- ✅ 全体処理時間: 6.65秒 → 6.01秒（9.6%改善）
- ⚠️ Single Pass処理は並列化できず（誤った方法を使用）

**Phase 3（未実装）:**
- 📋 `par_map_reduce`を使用したSingle Pass処理の並列化
- 📋 NodeConnectionCounterのDashMap化
- 📋 Map-Reduceパターンへの設計変更
- 📋 工数: 7-12時間
- 📋 期待される改善: 6.01秒 → 2.6秒（-3.4秒、57%改善）

**次のステップ**: Phase 3の実装判断（費用対効果の検討）
