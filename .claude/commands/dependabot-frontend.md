---
description: frontend の Dependabot PR を確認し、CIが通れば自動マージ。失敗時は自動修正する。
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, WebFetch
---

# frontend の Dependabot PR を自動処理

**目的**: frontend 依存を最新化し、CIが通ればマージする。判断はユーザーに求めず自走する。

## 実行上の重要事項

- このプロジェクトは git worktree 前提。各PRを専用worktreeで処理し、既存作業を壊さない。
- **Bash tool の各呼び出しは独立シェル**。変数は次の呼び出しに引き継がれないため、各 Bash ブロックで変数を必ず再定義する。
- **`gh pr checks --watch`・`npm ci`・`npm test` を含む Bash ブロックは `timeout: 600000` (10分) を指定**。デフォルトの120秒では足りない可能性がある。
- リトライ回数は `/tmp/dependabot-retry-<PR>` に永続化する。
- バックグラウンド実行禁止（`&`, `nohup`）。
- **pre-commit hook (`.husky/pre-commit`)** が存在し、commit時に lint-staged が走る。skill の「コミット前最終チェック」を通過していれば pre-commit も通る想定。

## Step 1: 対象PRを列挙

```bash
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
cd "$REPO"
gh pr list --author "app/dependabot" --state open --json number,title,headRefName,url \
  --jq '.[] | select(.headRefName | startswith("dependabot/npm_and_yarn/frontend/"))'
```

該当PRがなければ「対象なし」と表示して終了。複数ある場合は各PRについて Step 2 を順次実行する。

## Step 2: 各PRを処理

以下の `PR=<PR番号>` を対象PR番号に置換して、ブロックごとに実行する。

### 2-1. worktree 作成・セットアップ

```bash
set -e
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/frontend-$PR"
cd "$REPO"

# stale worktree 管理情報をクリーンアップ
git worktree prune

# PR情報取得（closed/削除済みなら即スキップ）
BRANCH=$(gh pr view $PR --json headRefName --jq .headRefName) || {
  echo "PR $PR の情報取得に失敗（closed/削除済み？）"; exit 0;
}

# fetch 失敗時はスキップ
git fetch origin "$BRANCH" || {
  gh pr comment $PR --body "branch $BRANCH の fetch に失敗しました。手動確認が必要です。" 2>/dev/null || true
  exit 0
}

# 親ディレクトリを作成（git worktree add は親を作らない）
mkdir -p "$(dirname "$WT")"

# worktree 作成 or 最新化
if [ -d "$WT" ]; then
  git -C "$WT" reset --hard "origin/$BRANCH"
else
  git worktree add "$WT" -B "$BRANCH" "origin/$BRANCH"
fi

# リトライカウンタ初期化
echo 0 > "/tmp/dependabot-retry-$PR"

echo "WT ready: $WT"
```

### 2-2. CI 状態を判定（JSON）

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

判定結果に応じて:

- `all_pass` → **Step 2-6（マージ）** へ
- `has_fail` → **Step 2-3**（修正）へ
- `pending` → Step 2-5 の `--watch` で完了を待ち、再度 2-2 を実行
- `no_checks` / `unknown` → **Step 2-3** へ（念のため自動修正フロー）

### 2-3. ローカルで再現・修正

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/frontend-$PR"
cd "$WT/frontend"

npm ci

# 自動修正フェーズ
npm run format
npm run lint:fix || true
```

**修正戦略**（エラー種別ごとに対応）:

1. **format:check 失敗** → `npm run format` を実行
2. **lint 失敗** → `npm run lint:fix`。残ったエラーは該当箇所を Read → Edit で修正
3. **typecheck 失敗** → エラーから該当ファイル/行を特定:
   - 廃止API → 新APIに置換。必要に応じて CHANGELOG を `WebFetch`（例: `https://github.com/<org>/<repo>/releases`）
   - 型の厳格化 → 型注釈・`as`・`satisfies` で対応
   - `@types/*` 更新 → 呼び出し側を新シグネチャに合わせる
4. **test 失敗** → テストコード側を新API/挙動に合わせて更新（プロダクションコードは変えない）
5. **breaking change** → リリースノートを `WebFetch` で確認、移行ガイドに沿って修正

**コミット前最終チェック（CLAUDE.md 準拠・すべて check モード = CI と同じ）**:

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/frontend-$PR"
cd "$WT/frontend"

npm run format:check && npm run lint && npm run typecheck && npm test
```

一つでも失敗したら修正戦略に戻って再修正。全て通ったら **Step 2-4** へ。

### 2-4. コミット・プッシュ（差分なしなら CI 再実行）

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/frontend-$PR"
cd "$WT"

git add -A

if git diff --cached --quiet; then
  # 差分なし = ローカルで再現しない（flaky test等）。failed job のみ再実行
  RUN_ID=$(gh pr checks $PR --json name,link \
    --jq '.[] | select(.name != "update_release_draft") | .link' \
    | head -1 | grep -oE 'runs/[0-9]+' | cut -d/ -f2)
  if [ -n "$RUN_ID" ]; then
    gh run rerun "$RUN_ID" --failed || true
  fi
else
  # commit 失敗 (pre-commit hook の lint-staged 失敗など) なら push しない
  BRANCH=$(git rev-parse --abbrev-ref HEAD)
  git commit -m "fix(deps): CIを通すための修正" \
    && git push origin "HEAD:$BRANCH" \
    && sleep 10  # Actions が新 run を生成するまでのラグ
fi

# リトライカウンタ +1
RETRY=$(cat "/tmp/dependabot-retry-$PR" 2>/dev/null || echo 0)
echo $((RETRY + 1)) > "/tmp/dependabot-retry-$PR"
```

### 2-5. CI 完了を待つ

**この Bash ブロックは `timeout: 600000` で呼ぶ**:

```bash
PR=<PR番号>
gh pr checks $PR --watch --interval 30
```

終了後、**Step 2-2** で再判定:

- `all_pass` → **Step 2-6** へ
- `has_fail` → リトライ判定:

```bash
PR=<PR番号>
RETRY=$(cat "/tmp/dependabot-retry-$PR" 2>/dev/null || echo 0)
if [ "$RETRY" -ge 3 ]; then
  gh pr comment $PR --body "自動修正を3回試行しましたがCIが通りませんでした。手動確認が必要です。"
  echo "SKIP"
else
  echo "RETRY (count: $RETRY)"
fi
```

- `RETRY` → **Step 2-3** に戻る
- `SKIP` → **Step 2-7**（クリーンアップ）へ

### 2-6. マージ（3段フォールバック）

```bash
PR=<PR番号>
# 1段目: 即時マージ
if gh pr merge $PR --squash --delete-branch; then
  echo "MERGED"
# 2段目: auto-merge にフォールバック
elif gh pr merge $PR --squash --delete-branch --auto; then
  echo "AUTO_MERGE_ENABLED"
# 3段目: 諦めてコメント残し
else
  gh pr comment $PR --body "マージできませんでした（auto-merge 未有効 or branch protection）。手動確認が必要です。"
  echo "MERGE_SKIPPED"
fi
```

### 2-7. worktree クリーンアップ

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/frontend-$PR"
cd "$REPO"

# worktree に対応するブランチ名を取得してから worktree 削除
BRANCH=$(git worktree list --porcelain | awk -v wt="$WT" '
  $1=="worktree" && $2==wt {found=1; next}
  found && $1=="branch" {sub("refs/heads/", "", $2); print $2; exit}
')
git worktree remove "$WT" --force 2>/dev/null || true
[ -n "$BRANCH" ] && git branch -D "$BRANCH" 2>/dev/null || true
rm -f "/tmp/dependabot-retry-$PR"
```

## Step 3: 全PR処理後

残りの frontend Dependabot PR がなくなるまで Step 2 を繰り返す。終わったらサマリ表示:

```
=== frontend Dependabot PR 処理結果 ===
✅ マージ完了: #X, #Y
⚠️ 手動対応必要: #Z
```

## 実行時の注意（再掲）

- **ユーザーに判断を求めない**。迷ったら「修正してCIを通す」方向で自走する
- **既存のworktree/ブランチを壊さない**。`gh pr checkout` は使わず `git worktree add` を使う
- **コミット前チェック**は CLAUDE.md の通り（format:check / lint / typecheck / test すべて check モードで通す）
- 修正不能と判断したPRのみ、PRにコメントを残してスキップする
