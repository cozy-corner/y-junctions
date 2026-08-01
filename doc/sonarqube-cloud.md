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

sonar.sources=backend/src,frontend/src
sonar.exclusions=frontend/src/**/*.test.ts,frontend/src/**/*.test.tsx,frontend/src/test/**
sonar.tests=backend/tests,frontend/src
sonar.test.inclusions=frontend/src/**/*.test.ts,frontend/src/**/*.test.tsx

sonar.rust.cargo.manifestPaths=backend/Cargo.toml
sonar.rust.lcov.reportPaths=backend/lcov.info
sonar.javascript.lcov.reportPaths=frontend/coverage/lcov.info
```

frontend のテストは `src/` 配下に同居しているため、`sonar.exclusions` で source 側から除き
`sonar.test.inclusions` で test 側に寄せる（同一ファイルを source と test の両方に割り当てるとエラーになる）。

`sonar.rust.clippy.enabled` は既定で有効なので明示しない。スキャナが自前で clippy を実行する。

### 実行手順

必要なツール: `sonar-scanner`（`brew install sonar-scanner`）、`cargo-llvm-cov`、および
Rust 1.94.0 の toolchain（`clippy` component 込み）。

```bash
(cd backend && cargo +1.94.0 llvm-cov --all-features --lcov --output-path lcov.info)
(cd frontend && npm run test:coverage)
SONAR_TOKEN=<token> sonar-scanner   # リポジトリルートで実行
```

- スキャナは `sonar-project.properties` を自動で読む
- Rust 解析はスキャナが `cargo` と `clippy` を PATH から呼ぶため、toolchain が入っていることが前提
- `sonar.host.url` は指定不要。scanner 6.0 以降は SonarQube Cloud を既定の宛先とする
  （brew 版は 8.1.0）

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

## 手作業が必要なセットアップ

コードでは完結しない。以下はリポジトリオーナーが SonarQube Cloud 上で行う（1 回だけ）。

1. https://sonarcloud.io に GitHub アカウントでサインアップ
2. organization として `cozy-corner` をインポート
3. プロジェクトとして `y-junctions` を追加（project key は `cozy-corner_y-junctions` になる想定。
   実際に払い出された key が異なる場合は `sonar-project.properties` を合わせる）
4. プロジェクト設定で **Automatic Analysis を OFF** にする（Rust 非対応のため）
5. token を生成して手元に控える（`SONAR_TOKEN` として渡す）

この手順は README.md にも記載する。

## 検証方法

- ローカルでの事前確認（実施済み）:
  - `cd backend && cargo +1.94.0 llvm-cov --all-features --lcov --output-path lcov.info`
    → 148 テスト全通過、199KB の `lcov.info` を生成（既定 toolchain が 1.90.0 だと
    `rustc 1.90.0 is not supported` で落ちるため 1.94.0 の明示が必要）
  - `cd frontend && npm run test:coverage` → `frontend/coverage/lcov.info` を生成（テストが無いため空）
  - `npm run typecheck` / `npm run lint` / `npm run format:check` が全て通ること
- **未実施**: `sonar-scanner` の実行そのもの。SonarQube Cloud のアカウントと token が無いため、
  解析はまだ 1 度も走っていない。上記セットアップ完了後に初回実行し、以下を確認する:
  - Rust と TypeScript の両方が Languages に出ていること
  - backend の Coverage が 0% ではないこと（lcov のパス指定が効いている証拠）
  - clippy 由来の指摘が Issues に現れること

## スコープ外

- PR デコレーション（PR ごとの新規コード評価）— 今回の目的はリポジトリ単位の評価
- Quality Gate を required check にすること — まず負債量を見てから判断する
- monorepo 構成でのプロジェクト分割
- CI ワークフローによる自動実行 — 「CI 化しない理由」参照。必要になった時点で足す
- frontend のテスト追加 — 別途判断する
