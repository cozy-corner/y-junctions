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

