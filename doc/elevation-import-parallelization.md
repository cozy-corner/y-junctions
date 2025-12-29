# 標高インポート並列化戦略

## 概要

標高インポート処理を並列化して高速化するための戦略ドキュメント。

### 現在のボトルネック

- **処理対象**: 10万件以上のY字路
- **処理時間**: 約90秒（XMLパース処理）
- **CPU利用率**: 12.5%（8コア中1コアのみ使用）
- **ボトルネック**: 3000回のXMLパース処理（各30ms）が逐次実行

### 環境

- **CPU**: Apple M1 Pro (8コア: 6 Performance + 2 Efficiency)
- **期待効果**: 4.5-6倍の高速化（XMLパース: 90秒 → 15-20秒）

---

## 並列化戦略の比較

### 戦略1: 独立キャッシュ（スレッドローカル）

#### 概要

各スレッドが独自のキャッシュを持つ方式。

```rust
use rayon::prelude::*;

let elevation_updates: Vec<ElevationUpdate> = junctions
    .par_chunks(12500)  // 10万件 / 8スレッド = 12,500件/スレッド
    .flat_map(|chunk| {
        // 各スレッドが独立したProviderを作成
        let mut provider = ElevationProvider::new(elevation_dir).unwrap();

        chunk.iter().filter_map(|junction| {
            // スレッド内では逐次処理
            let junction_elev = provider.get_elevation(junction.lat, junction.lon).ok()??;
            let neighbor_elevs = get_neighbor_elevations(&mut provider, junction)?;

            Some(create_elevation_update(junction, junction_elev, neighbor_elevs))
        }).collect::<Vec<_>>()
    })
    .collect();
```

#### データフロー

```
Thread 1: cache { メッシュA, B, C, ... }  ← 独立
Thread 2: cache { メッシュA, D, E, ... }  ← 独立（メッシュAを重複）
Thread 3: cache { メッシュA, F, G, ... }  ← 独立（メッシュAを重複）
...

総メモリ使用量 = 8スレッド × キャッシュサイズ
```

#### メリット

- ✅ **実装が簡単**：既存のコードをほぼ変更不要
- ✅ **ロック不要**：スレッド間の競合なし
- ✅ **デバッグが容易**：各スレッドが独立して動作

#### デメリット

- ❌ **メモリ使用量が多い**：8倍のメモリ消費
- ❌ **重複パース**：同じメッシュを複数スレッドでパース
- ❌ **効率が悪い**：理論値より低い高速化（3-4倍程度）

#### 性能評価

| 項目 | 値 |
|------|-----|
| **実装難易度** | ⭐⭐⭐⭐⭐ 非常に簡単 |
| **速度** | ⭐⭐⭐ 3-4倍（重複パースで効率低下） |
| **メモリ効率** | ⭐⭐ 8倍使用 |
| **保守性** | ⭐⭐⭐⭐⭐ 高い |

#### 推奨度

🟡 **プロトタイプや小規模データセット向け**

---

### 戦略2: 共有キャッシュ（RwLock）

#### 概要

全スレッドが1つのキャッシュを共有。`Arc<RwLock<HashMap>>` で読み書き分離。

**重要**：`parking_lot::RwLock` を使用することで、標準ライブラリのpoisoning問題を回避。

```rust
use parking_lot::RwLock;
use rayon::prelude::*;
use std::sync::Arc;

pub struct ElevationProvider {
    cache: Arc<RwLock<HashMap<String, Arc<GsiTile>>>>,  // Arc<GsiTile>でクローン回避
    mesh_to_file: Arc<HashMap<String, PathBuf>>,
}

impl ElevationProvider {
    pub fn new(data_dir: &str) -> Result<Self> {
        // 初期化...
        Ok(Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            mesh_to_file: Arc::new(mesh_to_file),
        })
    }

    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<Option<f64>> {
        let mesh_code = calculate_mesh_code(lat, lon);

        // ステップ1: 読み取りロックでキャッシュ確認（複数スレッド同時可能）
        {
            let cache = self.cache.read();
            if let Some(tile) = cache.get(&mesh_code) {
                return Ok(tile.get_elevation(lat, lon).filter(|&e| e != -9999.0));
            }
        }  // ← 読み取りロック解放

        // ステップ2: XMLパース（ロックなし = 並列実行可能）
        let xml_path = self.mesh_to_file.get(&mesh_code)
            .ok_or_else(|| anyhow::anyhow!("Mesh code not found: {}", mesh_code))?;
        let tile = Arc::new(Self::parse_xml_file(xml_path)?);
        let elevation = tile.get_elevation(lat, lon);

        // ステップ3: 書き込みロックでキャッシュに保存（排他的）
        {
            let mut cache = self.cache.write();
            // 二重チェック：他スレッドが既に挿入している可能性
            cache.entry(mesh_code).or_insert(tile);
        }  // ← 書き込みロック解放

        Ok(elevation.filter(|&e| e != -9999.0))
    }
}

// 並列処理
let provider = Arc::new(ElevationProvider::new(elevation_dir)?);

let elevation_updates: Vec<ElevationUpdate> = junctions
    .par_iter()
    .filter_map(|junction| {
        // providerはキャプチャされる（Arc::cloneは不要）
        process_junction(&provider, junction).ok()
    })
    .collect();
```

#### データフロー

```
共有 cache { メッシュA, B, C, D, ... }
    ↑        ↑        ↑        ↑
Thread 1  Thread 2  Thread 3  Thread 4
  (ロック)  (待機)   (ロック)  (待機)
```

#### タイムライン

```
Thread 1: [ロック]確認[解放]---[XMLパース30ms]---[ロック]保存[解放]
Thread 2:      [ロック]確認[解放]---[XMLパース30ms]---[ロック]保存[解放]
Thread 3:           [ロック]確認[解放]---[XMLパース30ms]---[ロック]保存[解放]
                     ↑                  ↑
                 短時間ロック        同時実行（ロックなし）
```

#### メリット

- ✅ **メモリ効率が高い**：キャッシュは1つだけ
- ✅ **読み取り並列化**：複数スレッドが同時に読み取り可能
- ✅ **実装が比較的簡単**：標準的なパターン

#### デメリット

- ⚠️ **ロック競合のリスク**：書き込みロック取得時に全体が待機
- ⚠️ **重複パースの可能性**：同時アクセス時に同じメッシュを複数回パース
- ⚠️ **外部依存**：parking_lotクレートが必要

#### 重複パースの問題

```
時刻 0ms:
  Thread 1: メッシュA確認 → ミス → [ロック解放] → XMLパース開始
  Thread 2: メッシュA確認 → ミス → [ロック解放] → XMLパース開始  ← 重複！

時刻 30ms:
  Thread 1: パース完了 → [ロック]保存[解放]
  Thread 2: パース完了 → [ロック]保存[解放]（上書き、無駄）
```

#### 性能評価

| 項目 | 値 |
|------|-----|
| **実装難易度** | ⭐⭐⭐ 中程度 |
| **速度** | ⭐⭐⭐⭐ 4-5倍（重複パース+ロック競合） |
| **メモリ効率** | ⭐⭐⭐⭐⭐ 最適 |
| **保守性** | ⭐⭐⭐⭐ 高い |

#### 推奨度

🟢 **中規模データセット向け（外部依存を避けたい場合）**

---

### 戦略3: 共有キャッシュ（DashMap）⭐推奨

#### 概要

並行HashMap `DashMap` を使用。内部でシャーディングによりロック競合を最小化。

**重要**：`Arc<GsiTile>` を使用してクローンを回避し、`entry API` で重複パースを完全に防止。

```rust
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use rayon::prelude::*;
use std::sync::Arc;

pub struct ElevationProvider {
    cache: Arc<DashMap<String, Arc<GsiTile>>>,  // Arc<GsiTile>でクローン回避
    mesh_to_file: Arc<HashMap<String, PathBuf>>,
}

impl ElevationProvider {
    pub fn new(data_dir: &str) -> Result<Self> {
        // 初期化...
        Ok(Self {
            cache: Arc::new(DashMap::new()),
            mesh_to_file: Arc::new(mesh_to_file),
        })
    }

    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<Option<f64>> {
        let mesh_code = calculate_mesh_code(lat, lon);

        // 高速パス: キャッシュヒット（ほぼロックフリー）
        if let Some(tile) = self.cache.get(&mesh_code) {
            return Ok(tile.get_elevation(lat, lon).filter(|&e| e != -9999.0));
        }

        // 遅いパス: entry APIで原子的にパース＆挿入（重複パース完全防止）
        let tile_ref = self.cache.entry(mesh_code.clone())
            .or_try_insert_with(|| -> Result<Arc<GsiTile>> {
                let xml_path = self.mesh_to_file.get(&mesh_code)
                    .ok_or_else(|| anyhow::anyhow!("Mesh code not found: {}", mesh_code))?;
                let tile = Self::parse_xml_file(xml_path)?;
                Ok(Arc::new(tile))
            })?;

        Ok(tile_ref.value().get_elevation(lat, lon).filter(|&e| e != -9999.0))
    }
}

// 並列処理
let provider = Arc::new(ElevationProvider::new(elevation_dir)?);

let elevation_updates: Vec<ElevationUpdate> = junctions
    .par_iter()
    .filter_map(|junction| {
        // providerはキャプチャされる（Arc::cloneは不要）
        process_junction(&provider, junction).ok()
    })
    .collect();
```

**代替実装（DashMap v5以前の場合）**:

```rust
pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<Option<f64>> {
    let mesh_code = calculate_mesh_code(lat, lon);

    // キャッシュ確認
    if let Some(tile) = self.cache.get(&mesh_code) {
        return Ok(tile.get_elevation(lat, lon).filter(|&e| e != -9999.0));
    }

    // entry APIで原子的に処理
    let entry = self.cache.entry(mesh_code.clone());
    let tile_ref = match entry {
        Entry::Occupied(e) => e.into_ref(),
        Entry::Vacant(e) => {
            let xml_path = self.mesh_to_file.get(&mesh_code)
                .ok_or_else(|| anyhow::anyhow!("Mesh code not found: {}", mesh_code))?;
            let tile = Self::parse_xml_file(xml_path)?;
            e.insert(Arc::new(tile))
        }
    };

    Ok(tile_ref.value().get_elevation(lat, lon).filter(|&e| e != -9999.0))
}
```

#### DashMap の内部構造

```
DashMap {
    シャード 0: RwLock<HashMap> { メッシュA, B, C }  ← Thread 1, 2がアクセス
    シャード 1: RwLock<HashMap> { メッシュD, E, F }  ← Thread 3, 4がアクセス
    シャード 2: RwLock<HashMap> { メッシュG, H, I }  ← Thread 5, 6がアクセス
    シャード 3: RwLock<HashMap> { メッシュJ, K, L }  ← Thread 7, 8がアクセス
    ...
}

異なるシャードへのアクセスは競合しない → 並列度が高い
```

#### タイムライン

```
Thread 1: [読込シャード0]---[XMLパース]---[書込シャード0]
Thread 2:    [読込シャード1]---[XMLパース]---[書込シャード1]  ← 競合なし！
Thread 3:       [読込シャード2]---[XMLパース]---[書込シャード2]  ← 競合なし！
Thread 4:          [読込シャード3]---[XMLパース]---[書込シャード3]  ← 競合なし！

ロック待ちがほぼ発生しない
```

#### メリット

- ✅ **メモリ効率が高い**：キャッシュは1つだけ
- ✅ **ロック競合が最小**：シャーディングで並列度向上
- ✅ **最高の高速化**：4.5-6倍（実測予想）
- ✅ **重複パース完全防止**：`or_try_insert_with()` で原子的操作

#### デメリット

- ⚠️ **外部依存追加**：`dashmap` クレートが必要（v6.1以降推奨）
- ⚠️ **やや複雑**：並行データ構造の理解が必要

#### 性能評価

| 項目 | 値 |
|------|-----|
| **実装難易度** | ⭐⭐⭐⭐ やや難しい |
| **速度** | ⭐⭐⭐⭐⭐ 4.5-6倍（最高） |
| **メモリ効率** | ⭐⭐⭐⭐⭐ 最適 |
| **保守性** | ⭐⭐⭐⭐ 高い |

#### 推奨度

🟢🟢 **大規模データセット向け（最も推奨）**

---

## 総合比較

| 戦略 | 実装難易度 | 速度 | メモリ効率 | 推奨度 |
|------|-----------|------|-----------|--------|
| **独立キャッシュ** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐ (3-4倍) | ⭐⭐ | 🟡 プロトタイプ |
| **RwLock共有** | ⭐⭐⭐ | ⭐⭐⭐⭐ (4-5倍) | ⭐⭐⭐⭐⭐ | 🟢 中規模 |
| **DashMap共有** | ⭐⭐⭐⭐ | ⭐⭐⭐⭐⭐ (4.5-6倍) | ⭐⭐⭐⭐⭐ | 🟢🟢 **推奨** |

---

## 実装推奨事項

### ステップ1: 依存関係の追加

`backend/Cargo.toml`:
```toml
[dependencies]
dashmap = "6.1"        # 並行HashMap（or_try_insert_withサポート）
rayon = "1.10"         # データ並列処理
parking_lot = "0.12"   # 高性能なRwLock（RwLock戦略の場合）
```

**注意**: DashMap v6.0以前を使用する場合は、`or_try_insert_with` の代わりに `Entry` パターンマッチを使用してください。

### ステップ2: `ElevationProvider` の修正

`backend/src/importer/elevation.rs`:
```rust
use dashmap::DashMap;
use std::sync::Arc;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct ElevationProvider {
    cache: Arc<DashMap<String, Arc<GsiTile>>>,  // Arc<GsiTile>でクローン回避
    mesh_to_file: Arc<HashMap<String, PathBuf>>,
}

impl ElevationProvider {
    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<Option<f64>> {
        let mesh_code = calculate_mesh_code(lat, lon);

        // キャッシュヒット
        if let Some(tile) = self.cache.get(&mesh_code) {
            return Ok(tile.get_elevation(lat, lon).filter(|&e| e != -9999.0));
        }

        // 原子的にパース＆挿入
        let tile_ref = self.cache.entry(mesh_code.clone())
            .or_try_insert_with(|| -> Result<Arc<GsiTile>> {
                let xml_path = self.mesh_to_file.get(&mesh_code)
                    .ok_or_else(|| anyhow::anyhow!("Mesh not found"))?;
                Ok(Arc::new(Self::parse_xml_file(xml_path)?))
            })?;

        Ok(tile_ref.value().get_elevation(lat, lon).filter(|&e| e != -9999.0))
    }
}
```

### ステップ3: 並列処理の導入

`backend/src/importer/mod.rs`:
```rust
use rayon::prelude::*;

let provider = Arc::new(elevation_provider);

let elevation_updates: Vec<ElevationUpdate> = junctions
    .par_iter()  // ← 並列イテレータ
    .filter_map(|junction| {
        process_junction_parallel(&provider, junction).ok()
    })
    .collect();
```

---

## パフォーマンス予測

### 現在（逐次処理）

```
処理時間: 90秒
  - XMLパース: 3000回 × 30ms = 90秒
  - キャッシュヒット: 97,000回 × 0.01ms ≈ 1秒
CPU利用率: 12.5% (1/8コア)
```

### DashMap並列化後

```
理想的な処理時間: 11-12秒
  - XMLパース: 3000回 × 30ms / 8 = 11.25秒（完全並列化）
  - キャッシュヒット: 97,000回 × 0.01ms / 8 ≈ 0.12秒

実際の処理時間: 15-20秒（オーバーヘッド考慮）
  - ロック競合: ~5%
  - スレッド管理: ~10%
  - キャッシュミス: ~15%
  - Efficiencyコアの性能差: ~10%
  - その他: ~10%

CPU利用率: 100% (8/8コア)

現実的な効果: 4.5-6倍高速化（90秒 → 15-20秒）
```

### 全体フローにおける効果（DB処理を含む）

**重要**: 上記は**XMLパース処理のみ**の予測です。実際の`import_elevation_data()`には他の処理も含まれます。

```
全体フロー:
1. DB読み込み（find_without_elevation）     : 2-5秒
2. 標高取得ループ（XMLパース + 計算）       : 90秒 → 15-20秒（並列化）
3. DBバッチ更新（bulk_update_elevations）  : 10-15秒

現在の総処理時間: 102-110秒
並列化後の総処理時間: 27-40秒

全体効果: 2.5-4倍の高速化
```

**注意**: Amdahlの法則により、並列化できない部分（DB読み込み、バッチ更新など）が全体の性能に影響します。XMLパース部分は4.5-6倍高速化しますが、全体では2.5-4倍になります。

---

## 次のステップ

1. `dashmap` と `rayon` を `Cargo.toml` に追加
2. `ElevationProvider` を修正してスレッドセーフにする
3. `import_elevation_data()` に並列処理を導入
4. ベンチマークを実行して効果を測定
5. 必要に応じてチューニング（チャンクサイズ、スレッド数など）

---

## 参考資料

- [rayon: データ並列処理ライブラリ](https://docs.rs/rayon/)
- [dashmap: 並行HashMap](https://docs.rs/dashmap/)
- [Rustの並行プログラミング](https://doc.rust-lang.org/book/ch16-00-concurrency.html)
