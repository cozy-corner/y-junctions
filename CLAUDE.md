# Phase開発ルール

`doc/todo.md`の各Phaseチェックリストに書かれていることだけを実装する。
それ以外は実装しない。疑問があれば質問する。

# 起動時チェック

Claude Codeが起動した際、以下を自動的に確認・実行する：

1. **Worktree判定**: `.git`がファイルかどうかで判定
2. **依存関係チェック**:
   - `node_modules/`の存在確認
   - `frontend/node_modules/`の存在確認
3. **自動セットアップ**:
   - いずれかが存在しない場合、ユーザーに確認して`npm install`を実行
   - または、理由を説明した上で自動実行

**目的**: `git gtr new ai`などで新しいworktreeを作成した直後でも、自動的にセットアップが完了し、すぐに開発を開始できるようにする。

# Worktree開発環境セットアップ

## 新しいworktreeを作成した場合

新しいgit worktreeを作成した際は、以下のセットアップが**必須**：

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
