# 2本Way接続Y字路の検出ロジック

## 背景

### 問題

渋谷の有名なY字路（座標: 35.6653281, 139.7038614）が検索結果に表示されない。

### 調査結果

**該当ノード**: 1522928021 (lat: 35.6652968, lon: 139.7038746)

**接続しているway**:
- Way 520422791: highway=**unclassified** (キャットストリート)
- Way 138913755: highway=**service** (alley)

**現在のシステムの判定**:
- ❌ 検出されない
- 理由: 接続wayが**2本のみ**（システムは「3本way接続」を要求）

### 幾何学的分析

ノード1522928021から見た3方向:

```
         進入元 (74.9°)
              ↗
             /
   Service  /
   (39.6°) /
        ↗ /
         /
        O (ノード1522928021)
         \
          \
           ↘ 退出方向 (236.8°)
```

**計算された分岐角度**:
- angle_1: **35.3°** ← Sharp Y字路に該当
- angle_2: 162.0°
- angle_3: 162.7°

**結論**:
- ✅ 幾何学的には完璧なY字路（最小角度35.3°）
- ✅ Sharpタイプ（30° ≤ angle_1 < 45°）
- ❌ OSM構造上は2本wayのため除外

## 問題の本質

OpenStreetMapでは、視覚的・体感的にY字路であっても、以下の理由で2本wayとして表現されることがある:

1. **連続する道路**: メイン道路が1本のwayとして表現され、途中のノードで曲がる
2. **脇道の接続**: そのノードに別の道路（service, path等）が接続
3. **マッピングの粒度**: 厳密には3方向だが、wayの分割がされていない

## 提案ロジック

### 概要

**「2本way接続 + 幾何学的Y字路判定」** を追加実装する。

### 検出条件

以下の全条件を満たすノードを「2本wayY字路」として検出:

#### 1. 構造的条件
- 接続しているhighway wayが**ちょうど2本**
- **両方のway**が対象highway type（16種類）に該当
- 少なくとも1本のwayが当該ノードを**通過**している（始点・終点ではない）

#### 2. 幾何学的条件
- 3方向ベクトルが形成される:
  - 方向1: 通過wayの進入方向（前ノード → 当該ノード）
  - 方向2: 通過wayの退出方向（当該ノード → 次ノード）
  - 方向3: 接続wayの方向（隣接ノード → 当該ノード または その逆）
- 3方向間の最小角度 **< 60°**（既存のY字路判定条件と同じ）

### アルゴリズム

```
for each node with exactly 2 highway ways:
    way_a, way_b = get_connected_ways(node)

    # highway type チェック
    if not (is_valid_type(way_a) and is_valid_type(way_b)):
        continue

    # 通過wayを特定
    passing_way = None
    connecting_way = None

    if way_a passes through node:
        passing_way = way_a
        connecting_way = way_b
    elif way_b passes through node:
        passing_way = way_b
        connecting_way = way_a
    else:
        continue  # どちらも通過していない（両方が終点）

    # 3方向ベクトルを計算
    prev_node = get_prev_node(passing_way, node)
    next_node = get_next_node(passing_way, node)
    neighbor_node = get_neighbor_node(connecting_way, node)

    direction_1 = bearing(prev_node, node)
    direction_2 = bearing(node, next_node)
    direction_3 = bearing(neighbor_node, node) or bearing(node, neighbor_node)

    # 3つの分岐角度を計算
    angles = calculate_angles([direction_1, direction_2, direction_3])
    min_angle = min(angles)

    # Y字路判定
    if min_angle < 60:
        add_to_y_junction_candidates(node, angles, ...)
```

### エッジケース

#### ケース1: 両wayが終点の場合
```
Way A ────> [Node] <──── Way B
```
- 通過wayが存在しない
- **除外** (2本道路の単純な接続点、Y字路ではない)

#### ケース2: 両wayが通過する場合
```
Way A ────> [Node] ───> (Way A続き)
Way B ────> [Node] ───> (Way B続き)
```
- 理論上は4方向
- **除外** (X字路またはT字路、Y字路ではない)
- ただし、OSM上でこのような構造は稀

#### ケース3: 方向転換が微小な場合
```
Way A ──────> [Node] ──────> (ほぼ直進)
              ↑
           Way B (接続)
```
- 最小角度が60°以上になる
- **除外** (T字路扱い、既存ロジックと同じ)

## データ構造

### 既存のY字路データとの統合

検出されたノードは、既存の3本wayY字路と同じデータ構造で保存:

```rust
struct JunctionForInsert {
    osm_node_id: i64,
    lat: f64,
    lon: f64,
    angle_1: i32,
    angle_2: i32,
    angle_3: i32,
    bearings: Vec<i32>,
    // ... その他のフィールド
    way_1_highway_type: String,
    way_2_highway_type: String,
    way_3_highway_type: String,  // 2本wayの場合も3方向あるため
}
```

### way情報の記録

2本wayの場合:
- `way_1`: 通過wayの進入側
- `way_2`: 通過wayの退出側
- `way_3`: 接続way

注: `way_1`と`way_2`は同じOSM way IDだが、異なる方向として記録

## 期待される効果

### 検出可能になるY字路

1. **都市部の複雑な交差点**
   - メイン道路が緩やかにカーブしつつ、脇道が接続
   - 例: 渋谷のキャットストリート付近

2. **歩行者道路の分岐**
   - pedestrian, path, steps等の組み合わせ
   - 既存の対象highway typeに含まれる

3. **service道路の接続**
   - 幹線道路にservice/alleyが接続
   - 視覚的には明確な分岐

### データ増加の見込み

- 既存: 3本way接続のY字路のみ
- 追加: 2本way接続だが幾何学的にY字路
- 予想増加率: 10-30%程度（要検証）

## 実装上の注意点

### パフォーマンス

#### 問題の規模（実測値）

**関東データでの計測結果**:
- 全ノード（valid highway接続）: 9,379,094
- 2本way接続: 1,766,441 (18.8%)
  - 両方が通過（交差点）: 140,033 (8%)
  - 少なくとも1本終端: 1,626,408 (92%)
    - **ちょうど1本終端: 1,349,541 (76%)** ← 処理対象
    - 両方が終端: 276,867 (16%)
- 3本way接続: 330,060

**処理対象の比率**:
- 2本way（ちょうど1本終端）: 1,349,541
- 3本way: 330,060
- **比率: 4.1倍**

**全国スケールでの予測**:
- 2本way処理対象: 推定600万～1000万ノード
- 処理時間: 3本wayの4～5倍

#### 効果的なフィルタリング戦略

コストの低い順に実行:

**1. highway typeチェック（安価）**
```rust
if !is_valid_type(way_a) || !is_valid_type(way_b) {
    continue;  // 対象外のhighway typeを除外
}
```

**2. 終端チェック（中程度、最重要）**
```rust
let way_a_terminates = is_start_or_end(way_a, node);
let way_b_terminates = is_start_or_end(way_b, node);

// ちょうど1本だけが終端
if way_a_terminates == way_b_terminates {
    continue;  // 両方通過（交差点）or 両方終端を除外
}
```
- **効果**: 76% → 24%に削減（関東実測値）
- 除外ケース1: 両方通過 = 交差点（X字路など）
- 除外ケース2: 両方終端 = 2本道路の接続点
- 残るケース: 1本通過 + 1本終端 = Y字路またはT字路の候補

**3. 幾何計算と角度チェック（高コスト）**
```rust
let angles = calculate_3way_angles(...);
if min(angles) >= 60° {
    continue;  // T字路として除外
}
```
- **問題**: この段階まで134.9万ノードが残る
- T字路とY字路の区別には角度計算が必須
- これ以上の事前フィルタは存在しない

#### 処理量削減の限界

**事前フィルタで試した手法**:
- ✅ highway typeチェック
- ✅ 終端チェック（ちょうど1本）
- ❌ 同じway ID除外 - 効果なし（ほぼゼロ件）
- ❌ 前後ノード間距離 - 無意味（Y字路判定と無関係）

**結論**:
- 軽量な事前フィルタでは134.9万ノード（4.1倍）が限界
- Y字路/T字路の区別には角度計算が必須
- 全国で600万～1000万の角度計算が必要

### 既存機能への影響

- 3本way検出ロジックは**変更なし**
- 2本way検出は**追加**のみ
- データベーススキーマ変更**不要**

### テストケース

1. **渋谷ノード1522928021**: 最小角度35.3°、Sharp Y字路
2. **T字路の除外**: 最小角度>=60°の場合は除外されること
3. **両端点の除外**: 通過wayがない場合は除外されること

## 実装方針

### Phase 1: 3本way検出のみ（現状維持）

**デフォルトの動作**:
- 3本way接続ノードのみを検出
- 高速・安定
- 既存の全機能を維持

### Phase 2: 2本way検出（拡張機能）

**オプションフラグで有効化**:
```bash
# デフォルト（3本wayのみ）
import --input kanto.pbf --bbox ...

# 拡張（2本wayも検出）
import --input kanto.pbf --bbox ... --enable-two-way-junctions
```

**処理量の実測値（関東データ）**:
- 全ノード（valid highway接続）: 9,379,094
- 3本way接続: 330,060
- 2本way接続（ちょうど1本終端）: 1,349,541
- **比率: 4.1倍**

**全国スケールでの影響**:
- 2本way候補: 推定600万～1000万ノード
- 角度計算を全てに実行する必要がある
- 処理時間: 3本wayの4～5倍（推定）

**なぜ拡張機能なのか**:
1. **処理量が膨大** - 全国データで4～5倍の計算量
2. **T字路も含まれる** - 角度計算して初めてY字路/T字路を区別可能
3. **事前フィルタの限界** - これ以上の軽量な絞り込み方法がない
4. **必要性は限定的** - 多くのY字路は3本wayで検出できる

**期待される効果**:
- 渋谷キャットストリート（ノード1522928021）のような重要なY字路を検出
- OSMマッピングの粒度に依存しないロバストな検出

## 次のステップ

### Phase 1（既存機能の維持）
1. ✅ 調査完了・方針決定
2. ドキュメント整備

### Phase 2（2本way検出の実装）
1. `backend/src/importer/detector.rs` の実装確認
2. `--enable-two-way-junctions` フラグの追加
3. 2本way検出ロジックの実装
4. テストコード追加（渋谷ノード1522928021を含む）
5. 関東データでのベンチマーク
6. 全国データでの動作確認と効果測定
