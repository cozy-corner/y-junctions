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
Phase 3での並列化に備えて先行導入（現時点では`HashMap`でも動作する）

**変更ファイル**: `backend/src/importer/parser.rs`

---

## Phase 2: 並列処理の導入（未実装）

### 前提：何が遅いか

**現在の実装**:
```rust
// 1つのCPUコアで順次処理
reader.for_each(|element| {
    process(element)  // 直列処理
})
```

**ボトルネック**: Single Pass処理が5.13秒（全体の77%）

### 改善内容

**実装方法**:
```rust
use rayon::prelude::*;

// 4つのCPUコアで並列処理
reader.par_bridge().for_each(|element| {
    match element {
        Element::Way(way) => { /* way処理 */ }
        Element::Node(node) => {
            // DashMapでスレッドセーフに書き込み
            node_coords.insert(node.id(), (node.lat(), node.lon()));
        }
        _ => {}
    }
});

// 角度計算も並列化
let junctions: Vec<_> = y_junctions
    .par_iter()
    .filter_map(|junction| {
        calculate_junction_angles(...)
    })
    .collect();
```

### 何が早くなるか

**改善対象**: Single Pass処理 5.13秒

**理論値**（4コアCPUの場合）:
- 5.13秒 ÷ 4 = 1.28秒

**現実的な値**（並列化オーバーヘッド30%を考慮）:
- 5.13秒 ÷ (4 × 0.7) = 1.8秒
- **改善: 5.13秒 → 1.8秒（-3.3秒）**

**Total処理時間**:
- 6.65秒 → 3.3秒（**-3.3秒、50%改善**）

### 前提条件
- `osmpbf::ElementReader`が並列化（`par_bridge()`）に対応していること（要確認）
- 4コアCPUを前提
- DashMapによるスレッド同期オーバーヘッド含む

**変更ファイル**: `backend/src/importer/parser.rs`

---

## まとめ

| Phase | 改善対象 | 処理時間 | 改善幅 |
|-------|---------|---------|--------|
| Phase 1（完了） | ファイルI/O削減 | 6.65秒 | -0.52秒（7.2%） |
| Phase 2 | 並列処理 | 3.3秒 | -3.3秒（50%） |

**次のステップ**: Phase 2（並列処理）の実装
