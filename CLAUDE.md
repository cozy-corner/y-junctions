# 【最優先】セッション開始時の必須チェック

**このセクションを最初に実行すること。他の作業を開始する前に必ず実行する。**

## チェック手順

```bash
# 1. ルートのnode_modulesチェック
ls node_modules/husky 2>/dev/null || echo "❌ ルートのnpm installが必要"

# 2. Frontendのnode_modulesチェック
ls frontend/node_modules 2>/dev/null || echo "❌ Frontendのnpm installが必要"
```

## 対応

上記チェックで ❌ が表示された場合：

```bash
# ルートのnpm installが必要な場合
npm install

# Frontendのnpm installが必要な場合
cd frontend && npm install
```

## 確認

以下をユーザーに報告すること：

- [ ] ルートのnode_modules/huskyが存在することを確認した
- [ ] frontend/node_modulesが存在することを確認した
- [ ] 必要に応じてnpm installを実行した

**注意**: この環境チェックは新規セッション、継続セッション問わず毎回実行する。

---

# 【必須】コミット前のチェック

**コミット前に必ず以下のチェックを実行すること。チェックなしでのコミットは禁止。**

## Backend変更時

```bash
# 1. テスト実行（必須）
cargo test --manifest-path backend/Cargo.toml

# 2. フォーマットチェック（必須）
cargo fmt --manifest-path backend/Cargo.toml --check

# 3. Clippyチェック（必須）
cargo clippy --manifest-path backend/Cargo.toml -- -D warnings
```

## Frontend変更時

```bash
# 1. テスト実行（必須）
cd frontend && npm test

# 2. 型チェック（必須）
npm run type-check

# 3. フォーマットチェック（必須）
npm run format:check

# 4. Lintチェック（必須）
npm run lint
```

## コミット前の確認チェックリスト

以下の全てにチェックが入っていることを確認してからコミットすること：

- [ ] 該当するテストを全て実行し、全て通過した
- [ ] フォーマットチェックを実行し、通過した
- [ ] Clippyチェックを実行し、通過した（Backend）
- [ ] 型チェックを実行し、通過した（Frontend）
- [ ] Lintチェックを実行し、通過した（Frontend）

## CI失敗時の対応プロトコル

CI失敗を検出した場合、以下の手順を実行する：

1. ローカルで上記の全チェックを実行する
2. 失敗したチェックを修正する
3. 再度全チェックを実行し、全て通過することを確認する
4. 修正をコミットしてプッシュする

---

# Phase開発ルール

`doc/todo.md`の各Phaseチェックリストに書かれていることだけを実装する。
それ以外は実装しない。疑問があれば質問する。

## PR作成前の必須作業

**`doc/todo.md`の更新を必ず行う：**
- 完了したPhaseのタスクチェックボックスをすべて `[x]` にする
- Phaseタイトルに ✅ を追加する
- 完了条件に実際の結果を記載する（✅ や ⚠️ を使用）
- 必要に応じて実装メモを追加する（重要な実装詳細や発見した課題）

---

# Worktree開発環境セットアップ

## 新しいworktreeを作成した場合

新しいgit worktreeを作成した際は、以下のセットアップが必須：

```bash
# 1. ルートで依存関係をインストール（husky, lint-staged用）
npm install

# 2. Frontend依存関係をインストール
cd frontend && npm install
```

## 理由

- `node_modules`は`.gitignore`されているため、worktreeごとに独立
- ルートの`npm install`は`husky`と`lint-staged`に必要
- Frontendの`npm install`は開発ツール（vite, eslint, prettier等）に必要
- Backendは`npm install`不要（Rustのみ）

## Pre-commit hookが動作しない場合

症状：`npx: command not found`や`lint-staged: command not found`

解決：上記のセットアップ手順を実行する
