# SonarQube Cloud によるリポジトリ品質評価

## 目的

リポジトリ全体（backend: Rust / frontend: TypeScript）の品質を SonarQube Cloud で継続的に可視化する。
PR ごとのゲートではなく、**リポジトリ単位の品質スナップショットとその推移**を見ることが目的。

## 前提調査（公式ドキュメントで確認済み）

| 事項 | 結論 | 出典 |
| --- | --- | --- |
| リポジトリ可視性 | public → SonarQube Cloud 無料枠が使える | `gh repo view` |
| Automatic Analysis の Rust 対応 | **非対応**（Objective-C, Dart, Rust を除く全言語が対象） | [automatic-analysis](https://docs.sonarsource.com/sonarqube-cloud/analyzing-source-code/automatic-analysis.md) |
| Automatic Analysis のカバレッジ対応 | **非対応** | 同上 |
| Automatic Analysis の monorepo 対応 | **非対応** | 同上 |
| Rust 解析の前提条件 | 解析マシンの PATH に `cargo` と `clippy` が必要。`sonar.rust.clippy.enabled` はデフォルト有効 | [languages/rust](https://docs.sonarsource.com/sonarqube-cloud/analyzing-source-code/languages/rust.md) |
| Rust カバレッジ取り込み | `sonar.rust.lcov.reportPaths` / `sonar.rust.cobertura.reportPaths` | [test-coverage-parameters](https://docs.sonarsource.com/sonarqube-cloud/analyzing-source-code/test-coverage/test-coverage-parameters.md) |
| JS/TS カバレッジ取り込み | `sonar.javascript.lcov.reportPaths` | 同上 |
| SonarScanner CLI の宛先 | scanner 6.0 以降は `sonar.host.url` 不要（SonarQube Cloud が既定）。`SONAR_TOKEN` と `sonar.organization` / `sonar.projectKey` があればよい | [scanners/sonarscanner-cli](https://docs.sonarsource.com/sonarqube-cloud/analyzing-source-code/scanners/sonarscanner-cli.md) |

→ Rust を対象に含める以上、Automatic Analysis は使えず **SonarScanner を明示的に走らせる必要がある**。
それを CI で回すか手元で回すかは別問題で、本件では後者を選ぶ（後述）。

## 決定事項

| 項目 | 決定 | 理由 |
| --- | --- | --- |
| 解析対象 | backend + frontend の両方 | リポジトリ全体の品質を 1 画面で見たい |
| プロジェクト構成 | **単一 Sonar プロジェクト**（monorepo 構成は採らない） | 目的に対して monorepo 分割は過剰。プロジェクト手動作成と CI 二重化のコストに見合わない |
| カバレッジ | backend / frontend とも取り込む | カバレッジなしでは品質評価の主要指標が欠ける |
| Quality Gate | 非ブロッキング | 既存負債の量が不明なうちは止めない。CI で回さないのでマージも妨げない |
| 実行方法 | **ローカルから手動実行**（CI ワークフローは作らない） | 後述「CI 化しない理由」 |
| 既存 CI | 変更しない | PR のブロッキング条件を今回の変更で動かさない |

### CI 化しない理由

検討の過程で週次 cron の GitHub Actions ワークフローを一度実装したが、以下の理由で取り下げた。

- **手作業は減らない。** 解析結果の保存先は SonarQube Cloud なので、org 作成・プロジェクト追加・
  token 発行はローカル実行でも等しく必要。CI 化で減るのは「自分で実行する手間」だけ
- **トレンドが平らになる。** グラフが意味を持つのはメトリクスが動いたときだが、main への変更は
  ほぼ Dependabot のマージで `backend/src` / `frontend/src` は動かない。週次で回しても同じ値が並ぶだけ
- **劣化検知として機能しない。** Quality Gate 非ブロッキングかつ通知未設定では、結局ダッシュボードを
  自分で見に行かない限り気づかない
- **コストが重複していた。** ワークフロー約 100 行のうち postgis サービス・テスト DB 作成は
  `backend-ci.yml` の test job とほぼ同じで、週 1 で backend 全テスト（実測 51 秒 + ビルド）が回る

トレンド蓄積が効くのは、複数人が継続的にコードを足していて、知らないうちの品質低下を検知したい場合。
本リポジトリはその状況にない。必要になった時点でワークフローを足せばよく、
`sonar-project.properties` とカバレッジ設定はそのまま流用できる。

## 構成

### 追加・変更するファイル

```
sonar-project.properties          (新規・リポジトリルート)
frontend/vitest.config.ts         (coverage reporter を追加)
frontend/package.json             (test:coverage スクリプト、@vitest/coverage-v8)
frontend/eslint.config.js         (coverage を ignores に追加)
.gitignore                        (カバレッジ生成物を除外)
README.md                         (セットアップと実行手順を追記)
```

`.github/workflows/backend-ci.yml` と `frontend-ci.yml` は変更しない。

### sonar-project.properties

```properties
sonar.projectKey=cozy-corner_y-junctions
sonar.organization=cozy-corner

sonar.sources=.
sonar.tests=backend/tests
sonar.exclusions=backend/tests/**,backend/target/**,backend/lcov.info,\
  frontend/node_modules/**,frontend/dist/**,frontend/coverage/**,frontend/src/test/**,data/**

sonar.rust.cargo.manifestPaths=backend/Cargo.toml
sonar.rust.lcov.reportPaths=backend/lcov.info
sonar.javascript.lcov.reportPaths=frontend/coverage/lcov.info
```

#### sonar.sources をリポジトリ全体にする理由

当初 `sonar.sources=backend/src,frontend/src` と書いていたが誤り。
既に走っている Automatic Analysis の結果を API で確認したところ、未解決 107 件の内訳は:

| 件数 | 場所 |
| --- | --- |
| 57 | `.github/workflows`（secret の展開、action の SHA 未固定 など） |
| 20 | `backend/migrations` |
| 18 | `terraform`（GCS の logging/versioning 未設定、IAM の過剰権限） |
| 7 | `frontend/src` |
| 5 | その他 |
| 0 | `backend/src`（Rust は Automatic Analysis 非対応のため未解析） |

Sonar は解析のたびにプロジェクトの状態を上書きするため、範囲を `backend/src,frontend/src` に絞ると
`.github/workflows` / `backend/migrations` / `terraform` は「解析されていない = 問題なし」扱いになり、
95 件が修正されないままダッシュボードから消える。Automatic Analysis を OFF にする前提なので戻らない。

`sonar.sources=.` にすることで、現状の 107 件を維持したまま Rust（`backend/src` 7,169 行）と
カバレッジが純粋に上積みされる。

`sonar.test.inclusions` は使わない。この設定は `sonar.tests` に指定した全ディレクトリに効くため、
frontend のテストパターンを書くと `backend/tests/api_tests.rs` まで弾かれる。
frontend にはテストが 1 件も無いので、現状は `sonar.tests=backend/tests` のみで足りる。

`sonar.rust.clippy.enabled` は既定で有効なので明示しない。スキャナが自前で clippy を実行する。

### 実行手順

必要なツール: `sonar-scanner`（`brew install sonar-scanner`）と `cargo-llvm-cov`。
Rust の toolchain は `.mise.toml` で固定する。

手順は `.mise.toml` のタスクにまとめる。README に生コマンドを並べると、
`cargo +1.94.0` のようなバージョン回避策が手順書に固定化されてしまうため。

```toml
[tasks."sonar:coverage:backend"]   # cargo llvm-cov --all-features --lcov --output-path lcov.info
[tasks."sonar:coverage:frontend"]  # npm run test:coverage
[tasks."sonar"]                    # 上記 2 つに depends して sonar-scanner
```

```bash
mise run sonar
```

`SONAR_TOKEN` は `[env] _.file = ".env"` で `.env` から読ませる。
`.env` が存在しない場合もエラーにならないことを確認済みなので、Sonar を使わない環境や
他の worktree には影響しない。トークンをリポジトリ内の平文ファイルに置く形になるが、
root の `.gitignore` に `.env` が入っているためコミットされない。

#### `.mise.toml` の Rust バージョンを 1.94.0 に修正した

作業中に判明した既存の不具合。`.mise.toml` は `rust = "1.90.0"` を指していたが、
この版では**ビルド自体が通らない**。

```
$ cargo +1.90.0 check
error: rustc 1.90.0 is not supported by the following packages:
  sqlx@0.9.0 requires rustc 1.94.0
```

CI (`backend-ci.yml`) は 1.94.0 を指定しているため CI は通り、ローカルだけが壊れている状態だった。
当初 README に `cargo +1.94.0` と書いていたのはこの回避策にすぎないので、`.mise.toml` 側を直した。

- スキャナは `sonar-project.properties` を自動で読む
- Rust 解析はスキャナが `cargo` と `clippy` を PATH から呼ぶため、toolchain が入っていることが前提
- `sonar.host.url` は指定不要。scanner 6.0 以降は SonarQube Cloud を既定の宛先とする
  （brew 版は 8.1.0）

実行時の注意:

- **結果はローカルに残らない。** スキャナは解析後にレポートを SonarQube Cloud へアップロードし、
  プロジェクトの状態を**上書き**する。前の結果は履歴にしか残らない
- **git の状態ではなくファイルシステムを見る。** 未コミットの変更もそのまま解析対象になるので、
  クリーンな作業ツリーで実行する

### frontend のカバレッジ設定

`@vitest/coverage-v8` を devDependencies に追加し、`vitest.config.ts` に以下を足す:

```ts
coverage: {
  provider: 'v8',
  reporter: ['text', 'lcovonly'],
  reportsDirectory: './coverage',
},
```

`lcov` ではなく `lcovonly` を使う。`lcov` は HTML レポート（`coverage/lcov-report/`）も生成し、
その中の JS ファイルが `eslint . --ext .ts,.tsx` に拾われて `--max-warnings=0` を落とすため。
併せて `eslint.config.js` の `ignores` に `coverage` を追加し、`.gitignore` に
`frontend/coverage/` と `backend/lcov.info` を追加する。

`package.json` に `"test:coverage": "vitest run --passWithNoTests --coverage"` を追加する。
既存の `test` スクリプトは変更しない（PR CI の挙動を変えないため）。

#### 判明した前提: frontend にテストが 1 件も存在しない

vitest の設定（`vitest.config.ts` / `src/test/setup.ts`）はあるが、`*.test.ts(x)` は 1 つもない。
そのため導入直後の frontend カバレッジは **0%**、`coverage/lcov.info` は空ファイルになる。

これは設定不備ではなく現状の事実であり、Sonar はそれをそのまま報告する（それ自体が有用な情報）。
Quality Gate を非ブロッキングにしているため実害はない。テストを書くかどうかは本件のスコープ外とし、
別途判断する。

### backend のカバレッジ設定

`cargo-llvm-cov` は解析時にしか使わないため `Cargo.toml` は変更せず、
`cargo install cargo-llvm-cov --locked` で各自の環境に入れる。

## SonarQube Cloud 側の設定（実施済み・再現手順ではない）

以下は 2026-08-01 に実施済み。日常的に踏む手順ではないので README には載せない。
プロジェクトを作り直す場合や、設定の理由を追う場合の記録として残す。

| 項目 | 設定値 |
| --- | --- |
| organization key | `cozy-corner`（表示名は「Koji Sasaki」） |
| project key | `cozy-corner_y-junctions` |
| プラン | Free（public リポジトリのため） |
| Automatic Analysis | **OFF** |
| token 種別 | Personal Access Token（My Account > Security で発行） |

`Automatic Analysis` を OFF にした理由は 2 つ。

- Rust もカバレッジも取り込めないため、そのままでは目的を果たせない
- ON のままだと手動解析が `You are running manual analysis while Automatic Analysis is enabled`
  で拒否される。両者は排他

その副作用として、**PR に付いていた `SonarCloud Code Analysis` チェックは出なくなる**。
PR ごとの評価は行わない方針なので想定内だが、チェックが消えたことに後から驚かないよう記録しておく。

Scoped Organization Token（プロジェクト単位で解析実行権限のみを持つトークン）は Team プラン以上の
機能なので使えない。Personal Access Token はアカウント権限をそのまま持つ点に注意する。

## 検証方法

- ローカルでの事前確認（実施済み）:
  - `mise run sonar:coverage:backend`
    → 148 テスト全通過、199KB の `lcov.info` を生成（既定 toolchain が 1.90.0 だと
    `rustc 1.90.0 is not supported` で落ちるため 1.94.0 の明示が必要）
  - `cd frontend && npm run test:coverage` → `frontend/coverage/lcov.info` を生成（テストが無いため空）
  - `npm run typecheck` / `npm run lint` / `npm run format:check` が全て通ること
- **初回解析（実施済み・2026-08-01）**: 設定は意図通りに機能した。

| 指標 | 導入前（Automatic Analysis） | 初回解析後 |
| --- | --- | --- |
| 解析行数 | 3,707 | **9,001** |
| Rust | 0 行（非対応） | **5,497 行** |
| カバレッジ | 計測なし | **62.4%** |
| 指摘件数 | 107 | 113 |

既存 107 件は 1 件も落ちず（`.github/workflows` 57 / `backend/migrations` 20 / `terraform` 18 を維持）、
そこに Rust の 6 件が上積みされた。`sonar.sources=.` の判断が正しかったことが確認できた。

### 初回解析でわかった Rust 解析の実力

Rust 5,497 行に対して指摘は 6 件のみ、しかも**全て同一ルール `rust:S3776`（Cognitive Complexity）**だった。
理由は 2 つある。

- **ルール数が少ない。** Quality Profile の有効ルール数は Rust 78 / TypeScript 484 / Terraform 51。
  Sonar の Rust アナライザは 2025-04 に追加された新しいもので、まだ規模が小さい
- **clippy 由来の指摘がゼロ。** Rust ルールの多くは `sysTags: ["clippy"]` が付いた clippy の lint だが、
  既存 CI が `cargo clippy --all-targets --all-features -- -D warnings` を強制しているため既に潰れている

つまり Rust について Sonar が既存 CI に上積みする価値は、現状「関数の複雑度の可視化」と
「カバレッジの計測」の 2 点にほぼ限られる。指摘の大半（113 件中 95 件）は今も
GitHub Actions / SQL / Terraform 由来で、これは Automatic Analysis でも取れていた範囲。
導入効果を過大評価しないよう記録しておく。
  - clippy 由来の指摘が Issues に現れること

## スコープ外

- PR デコレーション（PR ごとの新規コード評価）— 今回の目的はリポジトリ単位の評価
- Quality Gate を required check にすること — まず負債量を見てから判断する
- monorepo 構成でのプロジェクト分割
- CI ワークフローによる自動実行 — 「CI 化しない理由」参照。必要になった時点で足す
- frontend のテスト追加 — 別途判断する
