---
description: backend の Dependabot PR を確認し、CIが通れば自動マージ。失敗時は自動修正する。
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, WebFetch
---

# backend の Dependabot PR を自動処理

**目的**: backend (Rust/cargo) 依存を最新化し、CIが通ればマージする。判断はユーザーに求めず自走する。

## 実行上の重要事項

- このプロジェクトは git worktree 前提。各PRを専用worktreeで処理し、既存作業を壊さない。
- **Bash tool の各呼び出しは独立シェル**。変数は引き継がれないため、各 Bash ブロックで変数を必ず再定義する。
- **`gh pr checks --watch`・`cargo build`・`cargo test`・`cargo clippy` を含む Bash ブロックは `timeout: 600000` (10分) を指定**。backend CI は ~2分36秒、ローカル初回ビルドはさらに長い。
- リトライ回数は `/tmp/dependabot-retry-<PR>` に永続化する。
- バックグラウンド実行禁止（`&`, `nohup`）。
- **pre-commit hook (`.husky/pre-commit`)** が存在し、commit時に lint-staged が走る。skill の「コミット前最終チェック」を通過していれば pre-commit も通る想定。
- **前提**: ローカルで `cargo test` を通すには PostgreSQL (with PostGIS) が必要。CI 相当の DB セットアップは CLAUDE.md / README.md 参照。DB 未起動時は test のみスキップし、fmt / clippy だけで判断する（CI で最終判定される）。
- **CI が全部緑でも、それだけではマージして良い根拠にならない**（MSRV が上がる更新は CI を素通りしてマージ後に落ちる）。Step 2-6 のマージ前チェックを必ず通す。

## Step 1: 対象PRを列挙

```bash
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
cd "$REPO"
gh pr list --author "app/dependabot" --state open --json number,title,headRefName,url \
  --jq '.[] | select(.headRefName | startswith("dependabot/cargo/backend/"))'
```

該当PRがなければ「対象なし」と表示して終了。複数ある場合は各PRについて Step 2 を順次実行する。

## Step 2: 各PRを処理

以下の `PR=<PR番号>` を対象PR番号に置換して、ブロックごとに実行する。

### 2-1. worktree 作成・セットアップ

```bash
set -e
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/backend-$PR"
cd "$REPO"

git worktree prune

BRANCH=$(gh pr view $PR --json headRefName --jq .headRefName) || {
  echo "PR $PR の情報取得に失敗（closed/削除済み？）"; exit 0;
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

- `all_pass` → **Step 2-6（マージ）** へ
- `has_fail` → **Step 2-3**（修正）へ
- `pending` → Step 2-5 の `--watch` で完了を待ち、再度 2-2
- `no_checks` / `unknown` → **Step 2-3** へ

### 2-3. ローカルで再現・修正

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/backend-$PR"
cd "$WT"

# 自動修正フェーズ（stderr は残す。エラー情報が必要）
cargo fmt --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --fix --allow-dirty --allow-staged -- -D warnings || true
```

**修正戦略**（エラー種別ごとに対応）:

1. **fmt --check 失敗** → `cargo fmt` を実行
2. **clippy 失敗** → `cargo clippy --fix` で自動修正。残るwarningは該当箇所を Read → Edit で修正
3. **コンパイルエラー（API変更）** → エラーから該当crateを特定:
   - 廃止API → crateのCHANGELOG を `WebFetch`（例: `https://github.com/<org>/<crate>/blob/main/CHANGELOG.md`）
   - シグネチャ変更 → 呼び出し側を新シグネチャに合わせる
   - 型変更 → `From`/`Into` 変換を挟むか、型を合わせる
4. **test 失敗** → テストコード側を新API/挙動に合わせて更新（プロダクションコードは変えない）
5. **breaking change** → リリースノートを `WebFetch` で確認、移行ガイドに沿って修正

**コミット前最終チェック（CLAUDE.md 準拠・すべて check モード = CI と同じ）**:

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/backend-$PR"
cd "$WT"

cargo fmt --manifest-path backend/Cargo.toml --check \
  && cargo clippy --manifest-path backend/Cargo.toml -- -D warnings \
  && cargo test --manifest-path backend/Cargo.toml
```

一つでも失敗したら修正戦略に戻って再修正。全て通ったら **Step 2-4** へ。
DB 未起動で `cargo test` のみ失敗する場合は、`cargo fmt --check` と `clippy` が通った時点で Step 2-4 に進んで CI に判定を委ねる（その旨をコミットメッセージに記載）。

### 2-4. コミット・プッシュ（差分なしなら CI 再実行）

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/backend-$PR"
cd "$WT"

git add -A

if git diff --cached --quiet; then
  RUN_ID=$(gh pr checks $PR --json name,link \
    --jq '.[] | select(.name != "update_release_draft") | .link' \
    | head -1 | grep -oE 'runs/[0-9]+' | cut -d/ -f2)
  if [ -n "$RUN_ID" ]; then
    gh run rerun "$RUN_ID" --failed || true
  fi
else
  # commit 失敗時 (pre-commit hook 等) は push しない
  BRANCH=$(git rev-parse --abbrev-ref HEAD)
  git commit -m "fix(deps): CIを通すための修正" \
    && git push origin "HEAD:$BRANCH" \
    && sleep 10
fi

RETRY=$(cat "/tmp/dependabot-retry-$PR" 2>/dev/null || echo 0)
echo $((RETRY + 1)) > "/tmp/dependabot-retry-$PR"
```

### 2-5. CI 完了を待つ

**この Bash ブロックは `timeout: 600000` で呼ぶ**:

```bash
PR=<PR番号>
gh pr checks $PR --watch --interval 30
```

終了後、Step 2-2 で再判定:

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
- `SKIP` → **Step 2-7** へ

### 2-6. マージ（3段フォールバック）

**マージ前チェック: Rust バージョンの整合**。依存更新が要求 rustc を上げても CI は緑のまま通る。`backend-ci.yml` は `toolchain: '1.94.0'` を明示して新しい側でビルドし、かつ CI には `docker build` が無いため。古いままなのは Dockerfile の `FROM rust:X.Y` だけで、それはマージ後の `Deploy to Production` で初めて落ちる。Rust バージョンは複数ファイルに分散しているので、ずれていないか確認する:

```bash
grep -rn 'rust:1\|RUST_VERSION:\|^rust = ' .mise.toml .github/workflows/*.yml backend/Dockerfile pipeline/Dockerfile
```

ずれていたら Dockerfile 2 つを揃える（`FROM rust:X.Y` は `.github/dependabot.yml` に docker ecosystem が無いため自動更新されない）。直したら実ビルドまで確認してからマージする:

```bash
docker build -t verify-backend ./backend
docker build -f pipeline/Dockerfile -t verify-pipeline .   # context はリポジトリルート
```

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

### 2-7. worktree クリーンアップ

```bash
PR=<PR番号>
REPO=$(cd "$(git rev-parse --path-format=absolute --git-common-dir)/.." && pwd)
WT="$REPO/.claude/worktrees/dependabot/backend-$PR"
cd "$REPO"

BRANCH=$(git worktree list --porcelain | awk -v wt="$WT" '
  $1=="worktree" && $2==wt {found=1; next}
  found && $1=="branch" {sub("refs/heads/", "", $2); print $2; exit}
')
git worktree remove "$WT" --force 2>/dev/null || true
[ -n "$BRANCH" ] && git branch -D "$BRANCH" 2>/dev/null || true
rm -f "/tmp/dependabot-retry-$PR"
```

## Step 3: 全PR処理後

```
=== backend Dependabot PR 処理結果 ===
✅ マージ完了: #X, #Y
⚠️ 手動対応必要: #Z
```

## 実行時の注意（再掲）

- **ユーザーに判断を求めない**。迷ったら「修正してCIを通す」方向で自走する
- **既存のworktree/ブランチを壊さない**。`gh pr checkout` は使わず `git worktree add` を使う
- **コミット前チェック**は CLAUDE.md の通り（fmt --check / clippy / test すべて check モードで通す）
- DB 未起動で test のみ失敗する場合は fmt/clippy 通過後 CI に判定を委ねる
- 修正不能と判断したPRのみ、PRにコメントを残してスキップする
