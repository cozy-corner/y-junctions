# issue #257: enrich-elevation Job 対応概要

ブランチ: `chore/257-enrich-elevation`
worktree: `/Users/sasakitakashinanji/code/y-junctions-worktrees/chore-257-enrich-elevation`

OSM インポートを Cloud Run Jobs 化する meta issue #220 の最後の主要 step。
月次パイプラインに DEM 標高エンリッチを組み込み、load 時点で `y_junctions.elevation` 列が埋まるようにする。

---

## 1. 設計方針（要点）

- **DAG 内に enrich-elevation を組み込む**（Baidu #258 の post-DAG パターンは採用しない — 月次パイプラインの強整合性を保つ）
- **side-table 設計** — extracted と elevation は別 Parquet で持つ（PBF 月次更新と DEM 年次更新のライフサイクル分離）
- **stage = bucket、内容は path で区別** — 既存 yj-raw / yj-extracted / yj-serving の規約を踏襲し、新規 `yj-enriched/elevations/...` を追加
- **将来 enricher 追加時** — `yj-enriched/baidu-panoids/...` のように同バケット内で path 分離（傘語ではなく "enrich stage" の意味）
- **欠損 sentinel: `-9999.0`**（既存 ElevationProvider 慣行と一致、parquet_derive v58 の `Option<T>` 非対応回避）
- **region 判定** — catalog の `region` フィールドで Workflows YAML が switch、`japan` 以外は enrich を skip（catalog 駆動、座標ベース判定なし）

---

## 2. バケット / path レイアウト

```
gs://...-yj-raw/                          (download / 投入 stage)
  osm/{date}/{dataset}.osm.pbf            既存（lifecycle 90d delete）
  dem/{YYYYMMDD}/*.xml.gz                 新規 path — operator 年次手動投入（lifecycle 対象外）

gs://...-yj-extracted/                    (extract stage、既存変更なし)
  three-way/{date}/{dataset}.parquet
  two-way/{date}/{dataset}.parquet

gs://...-yj-enriched/                     (enrich stage、新規)
  elevations/three-way/{date}/{dataset}.parquet
  elevations/two-way/{date}/{dataset}.parquet

gs://...-yj-serving/                      (serving stage、書く schema が変わる)
  {date}/{dataset}.parquet
```

`yj-raw` は Storage Class = `COLDLINE`、lifecycle delete は `matches_prefix = ["osm/"]` で `osm/` のみ scope。

---

## 3. Parquet record 3 種（`backend/src/pipeline/parquet_io.rs`）

| 既存/新規 | struct | 用途 |
|---|---|---|
| 既存 | `JunctionParquetRecord` | extracted stage、enrichment 列なし、変更なし |
| **新規** | `ElevationParquetRecord` | enrich stage、`osm_node_id` + elevation 10 列（薄い side table、欠損行は出さないので全 non-nullable で OK）|
| **新規** | `ServingJunctionParquetRecord` | serving stage、extracted 18 列 + elevation 10 列。**`-9999.0` を欠損 sentinel**、`min_angle_index` は `-1` |

**変換**
- `From<ServingJunctionParquetRecord> for JunctionForInsert` で sentinel → `None` mapping
- `write_parquet_bytes` / `read_parquet_bytes` を `T: ParquetRecordWriter/Reader` で generic 化

---

## 4. Binary 変更

| binary | 改修 |
|---|---|
| `pipeline-extract-three-way` / `-two-way` | **変更なし** |
| `pipeline-enrich-elevation`（**新規** `backend/src/bin/pipeline_enrich_elevation.rs`） | `--input` extracted parquet / `--output` enriched parquet / `--dem-dir` (FUSE mount path)。 `resolve_dem_dir` で `^\d{8}$` 形式の subdir を優先、無ければ direct layout fallback。 `compute_elevation_enrichment` を rayon par_iter で適用、成功行のみ `ElevationParquetRecord` で write |
| `pipeline-prepare-serving` | concat → osm_node_id LEFT JOIN に改修。引数を `--input` → `--extracted` + 任意の `--enrichment`（複数可、空文字列は filter）に再設計 |
| `pipeline-load-to-cockroach` | 読込 record 型を `ServingJunctionParquetRecord` に変更、`From` 実装が sentinel → None mapping を担う |

`Cargo.toml` に `pipeline-enrich-elevation` の `[[bin]]` と `flate2` dependency を追加。

---

## 5. 共通ロジック（`backend/src/importer/`）

**`compute_elevation_enrichment`**（`mod.rs`）— 純関数として新規 pub。引数 `(provider, lat, lon, bearings, angles)`、戻り値 `Option<ElevationEnrichment>`。
- `try_get_elevation` ヘルパで実エラー（I/O / parse / gzip 失敗）は `tracing::warn!`、 `Ok(None)`（out-of-coverage）は silent
- legacy `import_elevation_data` もこの helper を共有するように refactor

**ElevationProvider 拡張**（`elevation.rs`）
- `.xml.gz` 透過読み（`flate2::read::GzDecoder` で拡張子判定 → 自動 decode）
- glob を 4 pattern（`xml/*.xml`, `xml/*.xml.gz`, `*.xml`, `*.xml.gz`）で flat / xml/ 両 layout 対応
- `get_elevation` で "mesh not in mesh_to_file" を Err → **`Ok(None)`** に変更（out-of-coverage を first-class に）

---

## 6. Terraform（`terraform/pipeline.tf`）

**バケット / Storage Class**
- `yj_enriched` バケット新規（lifecycle 90d、yj_extracted と同形、Standard）
- 既存 `yj_raw` に `storage_class = "COLDLINE"` 追加
- `yj_raw` の lifecycle_rule に `matches_prefix = ["osm/"]` 追加 — **DEM (`dem/` prefix) は自動削除されない**

**SA / IAM**
- `sa-pipeline-enrich-elevation` 新規
- IAM 3 件: read yj-extracted / read yj-raw / write yj-enriched
- prepare-serving SA に read yj-enriched 追加
- workflow_acts_as_enrich_elevation 追加

**Cloud Run Job**
- `pipeline-enrich-elevation`（mem 8Gi、cpu 2、timeout 30min、max_retries 1）
- `execution_environment = "EXECUTION_ENVIRONMENT_GEN2"` 明示
- `volumes.gcs { bucket = yj_raw, read_only = true }` を `/mnt/dem` にマウント（FUSE）

**Workflows YAML**（インライン HEREDOC）
- `init` step で `region`（missing / 空文字列 → `unknown` フォールバック）、 `enriched_*_uri`、 `prepare_serving_args` (no-enrichment 形のデフォルト) を宣言
- `extract` の後に `enrich:` switch step を挿入：
  - `condition: region == "japan"` → 3-way / 2-way 並列で `pipeline-enrich-elevation` 起動、`prepare_serving_args` を `--enrichment` 付きに再代入
  - `condition: true` → no-op、`prepare_serving_args` を `--extracted` のみに再代入
- `prepare_serving` step は `args: ${prepare_serving_args}` で動的に list 受け取り

---

## 7. catalog（`pipeline/datasets.json`）

shikoku-latest / chugoku-latest の 2 entry に `"region": "japan"` 追加。dispatcher は entry 全体を `json.encode_to_string(ds)` で workflow に渡しているので dispatcher 側改修不要。

---

## 8. operator手順（`README.md` に追記）

年次 DEM 更新フロー：
```bash
# 1. 国土地理院から DEM5A 取得し ~/y-junctions-data/gsi/xml/ に展開

# 2. gzip 圧縮（-k で元 .xml を残す → upload リトライ時 / ローカル CLI 用）
gzip -k ~/y-junctions-data/gsi/xml/*.xml

# 3. 日付 prefix にアップロード
gsutil cp ~/y-junctions-data/gsi/xml/*.xml.gz \
  gs://${PROJECT_ID}-yj-raw/dem/$(date +%Y%m%d)/

# 4. refresh したい時は手動キック（real な値は pipeline/datasets.json から）
gcloud workflows execute yj-pipeline --location=asia-northeast1 \
  --data='{"dataset":"shikoku-latest","geofabrik_url":"https://download.geofabrik.de/asia/japan/shikoku-latest.osm.pbf","bbox":"134.0,34.3,134.1,34.4","region":"japan"}'

# 古い DEM 掃除（operator が新 DEM upload 時に手動）
gsutil -m rm -r gs://${PROJECT_ID}-yj-raw/dem/20250515/
```

---

## 9. Dockerfile（`pipeline/Dockerfile`）

`pipeline-enrich-elevation` を `cargo build --bin` リストと container COPY 双方に追加。

---

## 10. Code Review 結果と対応

extra-high 効ort review で 15 件 surface。

### 直したもの（quick wins）

| # | 内容 | 修正 |
|---|------|------|
| #1 | Dockerfile に新 binary 漏れ | `--bin pipeline-enrich-elevation` + COPY 追加 |
| #2 | yj-raw 90 日 lifecycle が DEM を巻き込む | `matches_prefix = ["osm/"]` で osm/ のみ scope |
| #3 + #10 | resolve_dem_dir が非日付 subdir / xml/ short-circuit | `^\d{8}$` regex 検証 + date-subdir 優先、unit tests 6 件追加 |
| #4 | Workflow `prepare_serving_args` の switch-out scope 依存 | `init` で default 配列宣言（no-enrichment 形） |
| #5 | region 空文字列が "japan" 比較で silent skip | `if(region_raw == "", "unknown", region_raw)` |
| #6 | README `gzip` destructive | `gzip -k` 推奨に変更 |
| #7 | README workflow execute 例が literal `...` | datasets.json の real 値に置換 + 古い DEM 掃除手順追加 |
| #8 | compute_elevation_enrichment が Err と Ok(None) を区別なく drop | `try_get_elevation` ヘルパで Err のみ warn、ElevationProvider で mesh-not-found を Ok(None) 化 |

### 残し（PR description / follow-up issue）

- **#9 旧 schema serving Parquet の再 load 不可** — apply 後の transient 期間のみ影響、recovery には extract から再走
- **#11 Coldline + PBF 同 path 上書き early-deletion fee** — cost 注意、operator 認知のみ
- **#13 DEM 不在時の fail-loud** — 設計上正しい挙動として受容、alert 整備は別 issue
- **#14 空 Parquet write/read 未検証** — 実用上発生しにくい edge case
- **#15 ELEVATION_SENTINEL 衝突の latent footgun** — 将来別 DEM source 統合時に再評価、コメント済み

---

## 11. apply 前の確認事項

- **DEM volume mount の terraform schema 検証** — `google_cloud_run_v2_job` の volumes/volume_mounts ブロック（gen2 FUSE）の正確な書式
- **Workflows DSL の `map.get` / `default` / `if` 関数の実在確認** — apply 前に公式 reference で確認（[[feedback_verify_dsl_before_apply]]）
- **既存 yj-raw 上の無圧縮 XML / Standard クラスオブジェクト** — apply 後、operator が `gsutil rewrite` または次回 DEM 更新時に gzip 上書きで順次反映

---

## 12. Verification

- **cargo test**: 92 lib + 6 binary（resolve_dem_dir）+ 47 integration = **145 PASS**
- **cargo fmt --check**: PASS
- **cargo clippy -D warnings**: PASS
- **terraform fmt -check**: PASS

---

## 13. 変更ファイル一覧

```
M  README.md                                       (operator 手順 + 古い DEM 掃除)
M  backend/Cargo.lock
M  backend/Cargo.toml                              (+ flate2, + [[bin]])
M  backend/src/bin/pipeline_load_to_cockroach.rs   (型差し替え + sentinel mapping)
M  backend/src/bin/pipeline_prepare_serving.rs     (LEFT JOIN + --extracted/--enrichment)
M  backend/src/importer/elevation.rs               (.xml.gz, mesh-not-found → Ok(None), glob 4 pattern)
M  backend/src/importer/mod.rs                     (compute_elevation_enrichment + try_get_elevation)
M  backend/src/pipeline/parquet_io.rs              (2 新規 struct + generic + From impl + tests)
M  pipeline/Dockerfile                             (pipeline-enrich-elevation を build + COPY に追加)
M  pipeline/datasets.json                          ("region": "japan")
M  terraform/backend.tf                            (fmt のみ)
M  terraform/pipeline.tf                           (yj-raw COLDLINE+matches_prefix / yj-enriched
                                                    バケット / SA / IAM / Cloud Run Job (FUSE) /
                                                    Workflows YAML region switch)
?? backend/src/bin/pipeline_enrich_elevation.rs    (新規、resolve_dem_dir 含む)
?? doc/enrich-elevation.md                         (本ドキュメント)
```

---

## 14. issue 本文との差分

issue #257 本文の "却下した選択肢" / "リスク" を、設計討論の過程で以下のように decide:

| 論点 | 本文時点 | 確定 |
|------|---------|------|
| Parquet schema | append 列 or 検討中 | **side table** (`ElevationParquetRecord` + `ServingJunctionParquetRecord`) |
| DEM access | FUSE / on-demand fetch / pre-download | **gzip + Coldline + GCS FUSE volume mount** |
| Sentinel | NaN / -9999.0 | **-9999.0** (ElevationProvider 慣行と一致) |
| 失敗時 | abort / 行欠損 | **行欠損許容** + 実 I/O エラーは warn |
| Region 判定 | 座標ベース vs catalog | **catalog 駆動**（dispatcher が `json.encode_to_string` で entry 全体を渡すので追加コスト無し） |
| 月額 cost | issue 試算 ~$1 未満 | gzip + Coldline で月 **~$0.18** (Standard比 ~95% 削減) |
