# GitHubラベルのセットアップ

リリースノート自動生成には、以下のラベルが必要です。

## 必要なラベル

| ラベル名 | 色 | 説明 |
|---------|-----|------|
| 🗺️ 地域拡大 | `#0E8A16` (緑) | データ追加による地域拡大 |
| ✨ 新機能 | `#7057FF` (紫) | ユーザー向け新機能 |
| 🐛 バグ修正 | `#D73A4A` (赤) | バグ修正 |
| internal | `#EDEDED` (グレー) | 内部改善（リリースノートに含めない） |

## セットアップ方法

### 方法1: GitHub CLI（推奨）

```bash
# リポジトリのルートディレクトリで実行

# 🗺️ 地域拡大
gh label create "🗺️ 地域拡大" \
  --color "0E8A16" \
  --description "データ追加による地域拡大"

# ✨ 新機能
gh label create "✨ 新機能" \
  --color "7057FF" \
  --description "ユーザー向け新機能"

# 🐛 バグ修正
gh label create "🐛 バグ修正" \
  --color "D73A4A" \
  --description "バグ修正"

# internal
gh label create "internal" \
  --color "EDEDED" \
  --description "内部改善（リリースノートに含めない）"
```

### 方法2: GitHub Web UI

1. https://github.com/cozy-corner/y-junctions/labels にアクセス
2. 「New label」をクリック
3. 各ラベルを以下の情報で作成：

#### 🗺️ 地域拡大
- Label name: `🗺️ 地域拡大`
- Description: `データ追加による地域拡大`
- Color: `#0E8A16`

#### ✨ 新機能
- Label name: `✨ 新機能`
- Description: `ユーザー向け新機能`
- Color: `#7057FF`

#### 🐛 バグ修正
- Label name: `🐛 バグ修正`
- Description: `バグ修正`
- Color: `#D73A4A`

#### internal
- Label name: `internal`
- Description: `内部改善（リリースノートに含めない）`
- Color: `#EDEDED`

## セットアップ確認

```bash
# ラベル一覧を表示
gh label list
```

以下のラベルが表示されればOK：
- 🗺️ 地域拡大
- ✨ 新機能
- 🐛 バグ修正
- internal

## 次のステップ

ラベルのセットアップが完了したら、`doc/release-workflow.md` を参照して運用を開始してください。
