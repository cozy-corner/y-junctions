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
| scan action の実行環境 | ホストランナー上で動く（Docker コンテナではない）ため、事前ステップで入れた cargo/clippy が PATH に載る | [SonarSource/sonarqube-scan-action](https://github.com/SonarSource/sonarqube-scan-action) |

→ Rust を対象に含める以上、**CI ベース解析が必須**。Automatic Analysis は使わない。

## 決定事項

| 項目 | 決定 | 理由 |
| --- | --- | --- |
| 解析対象 | backend + frontend の両方 | リポジトリ全体の品質を 1 画面で見たい |
| プロジェクト構成 | **単一 Sonar プロジェクト**（monorepo 構成は採らない） | 目的に対して monorepo 分割は過剰。プロジェクト手動作成と CI 二重化のコストに見合わない |
| カバレッジ | backend / frontend とも取り込む | カバレッジなしでは品質評価の主要指標が欠ける |
| Quality Gate | 非ブロッキング | 既存負債の量が不明なうちは止めない。定期実行なのでそもそもマージを妨げない |
| 実行タイミング | **週次 cron + `workflow_dispatch`** | main への push はほぼ Dependabot のマージでソースが変わらない。push 契機は無駄が大きい |
| 既存 CI | 変更しない | PR のブロッキング条件を今回の変更で動かさない |

## 構成

### 追加・変更するファイル

```
sonar-project.properties          (新規・リポジトリルート)
.github/workflows/sonar.yml       (新規)
frontend/vitest.config.ts         (coverage reporter に lcov を追加)
frontend/package.json             (test:coverage スクリプト、@vitest/coverage-v8)
README.md                         (セットアップ手順を追記)
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

### .github/workflows/sonar.yml

トリガー:

```yaml
on:
  schedule:
    - cron: '0 0 * * 1'   # 毎週月曜 09:00 JST
  workflow_dispatch:
```

単一 job の流れ:

1. `actions/checkout@v7` を `fetch-depth: 0` で実行
   （Sonar が git blame で新旧コードを判定するため浅いクローンでは不正確になる）
2. `dtolnay/rust-toolchain@stable` で 1.94.0 + `clippy` をセットアップ
   （Rust 解析の前提条件。同時に scan action からも clippy が見える）
3. postgis サービスを起動し、`backend-ci.yml` の test job と同じ手順でテスト DB を作成
4. `cargo llvm-cov --all-features --lcov --output-path lcov.info` で backend のテスト実行 + カバレッジ生成
5. `actions/setup-node@v6` (23.11.0) → `npm ci` → `npm run test:coverage` で frontend の lcov 生成
6. `SonarSource/sonarqube-scan-action` を実行（`env: SONAR_TOKEN`）

Quality Gate の待機・失敗判定（`sonarqube-quality-gate-action`）は入れない。

`schedule` トリガーは、リポジトリが 60 日間非アクティブだと GitHub 側で自動停止される点に注意。
本リポジトリは Dependabot が定期的に PR を作るため実質的に問題にならない。

### frontend のカバレッジ設定

`@vitest/coverage-v8` を devDependencies に追加し、`vitest.config.ts` に以下を足す:

```ts
coverage: {
  provider: 'v8',
  reporter: ['text', 'lcov'],
  reportsDirectory: './coverage',
},
```

`package.json` に `"test:coverage": "vitest run --coverage"` を追加する。
既存の `test` スクリプトは変更しない（PR CI の挙動を変えないため）。

### backend のカバレッジ設定

`cargo-llvm-cov` は CI でのみ使うため `Cargo.toml` は変更しない。
ワークフロー内で `taiki-e/install-action@cargo-llvm-cov` によりインストールする。

## 手作業が必要なセットアップ

コードでは完結しない。以下はリポジトリオーナーが SonarQube Cloud 上で行う。

1. https://sonarcloud.io に GitHub アカウントでサインアップ
2. organization として `cozy-corner` をインポート
3. プロジェクトとして `y-junctions` を追加（project key は `cozy-corner_y-junctions` になる想定。
   実際に払い出された key が異なる場合は `sonar-project.properties` を合わせる）
4. プロジェクト設定で **Automatic Analysis を OFF** にする（Rust 非対応のため CI ベースへ切り替え）
5. token を生成し、GitHub リポジトリの secret `SONAR_TOKEN` として登録
6. ワークフローを `workflow_dispatch` で 1 回手動実行し、ダッシュボードに結果が出ることを確認

この手順は README.md にも記載する。

## 検証方法

- `sonar-project.properties` と workflow を追加した PR をマージ後、`workflow_dispatch` で手動実行
- SonarQube Cloud のプロジェクト概要で以下を確認する:
  - Rust と TypeScript の両方が Languages に出ていること
  - Coverage が 0% ではないこと（lcov のパス指定が効いている証拠）
  - clippy 由来の指摘が Issues に現れること
- ローカルでの事前確認: `cd frontend && npm run test:coverage` で `frontend/coverage/lcov.info` が生成されること、
  `cd backend && cargo llvm-cov --all-features --lcov --output-path lcov.info` が成功すること

## スコープ外

- PR デコレーション（PR ごとの新規コード評価）— 今回の目的はリポジトリ単位の評価
- Quality Gate を required check にすること — まず負債量を見てから判断する
- monorepo 構成でのプロジェクト分割
