---
description: すべての Dependabot PR を確認し、CIが通れば自動マージ。失敗時は自動修正する。
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, WebFetch
---

# すべての Dependabot PR を自動処理

**目的**: Dependabot が作成したすべてのPRを処理し、依存を最新化する。判断はユーザーに求めず自走する。

## 実行上の重要事項

- このプロジェクトは git worktree 前提。各PRを専用worktreeで処理し、既存作業を壊さない。
- **Bash tool の各呼び出しは独立シェル**。変数は引き継がれないため、各 Bash ブロックで変数を必ず再定義する。
- **`gh pr checks --watch` を呼ぶ Bash ブロックは `timeout: 600000` (10分) を指定**。
- frontend/backend の詳細手順は `/dependabot-frontend`・`/dependabot-backend` の skill ファイルを Read して適用する（このファイルには複製しない）。
- バックグラウンド実行禁止（`&`, `nohup`）。

## Step 1: 対象PRを列挙・分類

```bash
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
cd "$REPO"
gh pr list --author "app/dependabot" --state open \
  --json number,title,headRefName,url
```

該当PRがなければ「対象なし」と表示して終了。

ブランチ名の prefix で分類する:

| prefix | 種別 | 処理方法 |
| --- | --- | --- |
| `dependabot/npm_and_yarn/frontend/` | frontend | `/dependabot-frontend` の Step 2 を適用 |
| `dependabot/cargo/backend/` | backend | `/dependabot-backend` の Step 2 を適用 |
| `dependabot/github_actions/` | actions | 下記 Step 3 |
| `dependabot/npm_and_yarn/`（frontend 以外） | devtools | 下記 Step 4 |

## Step 2: frontend / backend PR の処理

- **frontend PR**: `.claude/commands/dependabot-frontend.md` を Read し、その Step 2 の手順をそのまま適用
- **backend PR**: `.claude/commands/dependabot-backend.md` を Read し、その Step 2 の手順をそのまま適用

手順の複製はしない。skill ファイルが更新されたら自動的に追従する。

## Step 3: actions PR の処理

GitHub Actions の更新PR。ローカルで workflow を実行できないため簡略化する。

### 3-1. worktree 作成

```bash
set -e
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/actions-$PR"
cd "$REPO"

git worktree prune

BRANCH=$(gh pr view $PR --json headRefName --jq .headRefName) || {
  echo "PR $PR の情報取得に失敗"; exit 0;
}

git fetch origin "$BRANCH" || {
  gh pr comment $PR --body "branch $BRANCH の fetch に失敗しました。手動確認が必要です。" 2>/dev/null || true
  exit 0
}

mkdir -p "$(dirname "$WT")"

if [ -d "$WT" ]; then
  git -C "$WT" reset --hard "origin/$BRANCH"
else
  git worktree add "$WT" -B "$BRANCH" "origin/$BRANCH"
fi
```

### 3-2. CI判定

```bash
PR=<PR番号>
gh pr checks $PR --json name,state --jq '
  [.[] | select(.name != "update_release_draft")] |
  if length == 0 then "no_checks"
  elif all(.state == "SUCCESS") then "all_pass"
  elif any(.state == "FAILURE" or .state == "CANCELLED" or .state == "TIMED_OUT" or .state == "ACTION_REQUIRED") then "has_fail"
  elif any(.state == "PENDING" or .state == "QUEUED" or .state == "IN_PROGRESS") then "pending"
  else "unknown" end
'
```

- `all_pass` / `no_checks` → **Step 3-4（マージ）** へ
- `pending` → Step 3-3 の `--watch` で待ってから再判定
- `has_fail` → まず failed run を再実行:

```bash
PR=<PR番号>
RUN_ID=$(gh pr checks $PR --json name,link \
  --jq '.[] | select(.name != "update_release_draft") | .link' \
  | head -1 | grep -oE 'runs/[0-9]+' | cut -d/ -f2)
[ -n "$RUN_ID" ] && gh run rerun "$RUN_ID" --failed || true
```

再実行後もダメなら、`.github/workflows/` のdiffを Read し、該当actionのリリースノートを `WebFetch` で確認して workflow ファイルを Edit。修正後に push。それでもダメならスキップ。

### 3-3. CI 完了を待つ

**`timeout: 600000` で呼ぶ**:

```bash
PR=<PR番号>
gh pr checks $PR --watch --interval 30
```

### 3-4. マージ（3段フォールバック）

```bash
PR=<PR番号>
if gh pr merge $PR --squash --delete-branch; then
  echo "MERGED"
elif gh pr merge $PR --squash --delete-branch --auto; then
  echo "AUTO_MERGE_ENABLED"
else
  gh pr comment $PR --body "マージできませんでした（auto-merge 未有効 or branch protection）。手動確認が必要です。"
  echo "MERGE_SKIPPED"
fi
```

### 3-5. worktree クリーンアップ

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/actions-$PR"
cd "$REPO"

BRANCH=$(git worktree list --porcelain | awk -v wt="$WT" '
  $1=="worktree" && $2==wt {found=1; next}
  found && $1=="branch" {sub("refs/heads/", "", $2); print $2; exit}
')
git worktree remove "$WT" --force 2>/dev/null || true
[ -n "$BRANCH" ] && git branch -D "$BRANCH" 2>/dev/null || true
```

## Step 4: devtools PR (root の npm) の処理

ルート `package.json`（husky など）の更新PR。該当するCIは無いが、`npm ci` が通ることを検証する。

```bash
set -e
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/devtools-$PR"
cd "$REPO"

git worktree prune

BRANCH=$(gh pr view $PR --json headRefName --jq .headRefName) || { exit 0; }

git fetch origin "$BRANCH" || {
  gh pr comment $PR --body "branch $BRANCH の fetch に失敗しました。手動確認が必要です。" 2>/dev/null || true
  exit 0
}

mkdir -p "$(dirname "$WT")"

if [ -d "$WT" ]; then
  git -C "$WT" reset --hard "origin/$BRANCH"
else
  git worktree add "$WT" -B "$BRANCH" "origin/$BRANCH"
fi

cd "$WT"
npm ci
```

- `npm ci` 成功 → Step 3-4 と同じマージ処理
- 失敗 → PRコメント残してスキップ

worktree クリーンアップは Step 3-5 と同様（パスの `actions-` を `devtools-` に置き換え）。

## Step 5: サマリ表示

```
=== Dependabot PR 処理結果 ===
✅ マージ完了:
  - #X (frontend): <title>
  - #Y (backend): <title>
⚠️ 手動対応必要:
  - #Z (actions): <title> - 理由: <reason>
```

## 実行時の注意（再掲）

- **ユーザーに判断を求めない**。迷ったら「修正してCIを通す」方向で自走する
- **既存のworktree/ブランチを壊さない**。`gh pr checkout` は使わず `git worktree add` を使う
- **frontend/backendの詳細手順はそれぞれのskillを Read して適用**。このファイルには複製しない
- 修正不能なPRのみ、PRにコメントを残してスキップする
