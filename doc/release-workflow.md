# リリースノート自動生成ワークフロー

## 概要

このプロジェクトでは、PRのマージ時に自動でリリースノート（GitHub Releases）を生成します。
ブランチ命名規則とPRラベルを使用して、ユーザー向けの変更のみをリリースノートに含めます。

## ブランチ命名規則

適切なブランチ名を使うことで、PRラベルが自動的に付与されます。

| ブランチプレフィックス | 説明 | 自動ラベル | リリースノートに含む |
|-------------------|------|-----------|-----------------|
| `data/` | データ追加（地域拡大） | 🗺️ 地域拡大 | ✅ はい |
| `region/` | データ追加（地域拡大） | 🗺️ 地域拡大 | ✅ はい |
| `import/` | データ追加（地域拡大） | 🗺️ 地域拡大 | ✅ はい |
| `feature/` | 新機能追加 | ✨ 新機能 | ✅ はい |
| `fix/` | バグ修正 | 🐛 バグ修正 | ✅ はい |
| `bugfix/` | バグ修正 | 🐛 バグ修正 | ✅ はい |
| `refactor/` | リファクタリング | internal | ❌ いいえ |
| `chore/` | 雑務・設定変更 | internal | ❌ いいえ |
| `perf/` | パフォーマンス改善 | internal | ❌ いいえ |
| `style/` | コードスタイル修正 | internal | ❌ いいえ |

## 運用フロー

### 1. データ追加時（地域拡大）

```bash
# 1. ブランチ作成
git checkout -b data/osaka-prefecture

# 2. データ更新履歴を記録
# doc/data-updates.md に以下を追加：
# - 2025-01-15: 大阪府（約1,200箇所）

# 3. コミット
git add doc/data-updates.md
git commit -m "大阪府のデータを追加"

# 4. PR作成
gh pr create --title "大阪府のY字路データを追加（1,200箇所）"
```

**自動処理:**
- ブランチ名 `data/osaka-prefecture` から自動的に「🗺️ 地域拡大」ラベルが付与される
- マージ後、GitHub Releases のドラフトに自動追加される

---

### 2. 新機能追加時

```bash
# 1. ブランチ作成
git checkout -b feature/distance-measurement

# 2. コード実装
# backend/src/ や frontend/src/ でコード変更

# 3. コミット
git add .
git commit -m "距離測定機能を実装"

# 4. PR作成
gh pr create --title "距離測定機能を追加"
```

**自動処理:**
- ブランチ名 `feature/distance-measurement` から自動的に「✨ 新機能」ラベルが付与される
- マージ後、GitHub Releases のドラフトに自動追加される

---

### 3. バグ修正時

```bash
# 1. ブランチ作成
git checkout -b fix/search-result-display

# 2. バグ修正
# 該当箇所を修正

# 3. コミット
git add .
git commit -m "検索結果が表示されない問題を修正"

# 4. PR作成
gh pr create --title "検索結果が表示されない問題を修正"
```

**自動処理:**
- ブランチ名 `fix/search-result-display` から自動的に「🐛 バグ修正」ラベルが付与される
- マージ後、GitHub Releases のドラフトに自動追加される

---

### 4. リファクタリング・技術的改善時

```bash
# 1. ブランチ作成
git checkout -b refactor/query-optimization

# 2. コード改善
# リファクタリングやパフォーマンス改善

# 3. コミット & PR作成
git add .
git commit -m "クエリを最適化"
gh pr create --title "データベースクエリを最適化"
```

**自動処理:**
- ブランチ名 `refactor/query-optimization` から自動的に「internal」ラベルが付与される
- マージ後も **GitHub Releases には含まれない**（ユーザーには関係ない変更のため）

---

## リリースノートの公開

### ドラフトの確認

PRがマージされると、自動的に GitHub Releases のドラフトが更新されます。

1. https://github.com/cozy-corner/y-junctions/releases にアクセス
2. 「Draft」となっているリリースを確認
3. 内容を確認・編集（必要に応じて）

### リリースの公開

1. ドラフトリリースの「Edit draft」をクリック
2. 内容を最終確認
3. 「Publish release」をクリック

公開されたリリースノートは、ユーザーがアプリ内のリンクから閲覧できます。

---

## ラベルの自動付与ルール

release-drafter が以下の条件でラベルを自動付与します：

### 🗺️ 地域拡大
- ブランチ名が `data/`, `region/`, `import/` で始まる
- `doc/data-updates.md` が変更されている
- PRタイトルに「地域」「データ」「インポート」「追加」「道」「県」「府」が含まれる

### ✨ 新機能
- ブランチ名が `feature/` で始まる
- PRタイトルに「feat」「feature」「新機能」「機能追加」が含まれる

### 🐛 バグ修正
- ブランチ名が `fix/`, `bugfix/` で始まる
- PRタイトルに「fix」「bug」「修正」「バグ」が含まれる

### internal
- ブランチ名が `refactor/`, `chore/`, `perf/`, `style/` で始まる
- PRタイトルに「refactor」「chore」「perf」「style」「リファクタ」「チューニング」「最適化」が含まれる

---

## トラブルシューティング

### ラベルが自動で付かない

- ブランチ名が規則に従っているか確認
- PRタイトルに適切なキーワードが含まれているか確認
- 手動でラベルを追加することも可能

### リリースノートに技術的な変更が含まれている

- 該当PRに「internal」ラベルを追加
- release-drafter が次回更新時に除外します

### リリースノートから特定のPRを除外したい

1. 該当PRに「internal」ラベルを追加
2. GitHub Releases のドラフトを再生成（新しいPRをマージすると自動で更新される）
