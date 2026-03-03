# バグ修正計画: GCS 上でのマーカー URL 直リンク

## 問題の概要

マーカーポップアップの「URL をコピー」ボタンで生成される URL が不正で、
本番環境（GCS ホスティング）でナビゲートするとエラーになる。

### 再現手順

1. 本番サイトを開く（`https://storage.googleapis.com/BUCKET_NAME/index.html`）
2. 任意のマーカーをクリック
3. ポップアップの URL コピーボタンを押す
4. コピーされた URL に遷移する

### 実際の挙動

```
https://storage.googleapis.com/node/581715516
```

GCS が `node` というバケットへのアクセスと解釈し、XML エラーを返す。

### 期待する挙動

```
https://storage.googleapis.com/BUCKET_NAME/index.html#node=581715516
```

に遷移し、マーカーにフォーカスされた状態でアプリが表示される。

---

## 根本原因

### 原因 1: URL 生成が `window.location.origin` のみを使用（`JunctionPopup.tsx`）

```js
// 現在（バケット名が落ちる）
const url = `${window.location.origin}/node/${osm_node_id}`;
// → https://storage.googleapis.com/node/581715516  ❌
```

`window.location.origin` は `https://storage.googleapis.com` を返すため、
バケット名（パスの第1セグメント）が失われる。

### 原因 2: パスベースのルーティングは GCS と相性が悪い

`/node/581715516` のようなパスベース URL にアクセスすると GCS はそのオブジェクトを探しに行く。
存在しない場合は 404 エラーが返る。

`gsutil web set -e index.html` でフォールバックを設定しても、`vite.config.ts` の
`base: './'`（相対パス）により、`index.html` が異なるパス深度で返された場合に
アセットの解決が壊れる（PR #191 と同じ白画面バグが再発する）。

```
URL: /BUCKET_NAME/node/581715516
./assets/xxx.js → /BUCKET_NAME/node/assets/xxx.js  ❌（存在しない）
正しくは      → /BUCKET_NAME/assets/xxx.js
```

### 原因 2 の対策: ハッシュを使う

ハッシュ（`#`）はブラウザ内だけで処理され、GCS へのリクエストは常に
`index.html` へのアクセスになる。pathname が変わらないためアセットの解決も壊れない。

意味的にも「このページ内の特定リソースを指定する」用途であり、ハッシュの慣習に合致する
（クエリパラメータは検索・フィルタ用途）。

---

## 修正内容

### 1. `frontend/src/components/JunctionPopup.tsx`

URL 形式をパスベース（`/node/{id}`）からハッシュ（`#node={id}`）に変更する。

```js
// 修正前
const url = `${window.location.origin}/node/${osm_node_id}`;

// 修正後（pathname を含めて現在のページ URL にハッシュを付与する）
const url = `${window.location.origin}${window.location.pathname}#node=${osm_node_id}`;
```

**動作確認：**

| 環境 | origin + pathname | 生成される URL |
|------|----------|---------------|
| ローカル | `http://localhost:3000/` | `http://localhost:3000/#node=123` ✅ |
| GCS | `https://storage.googleapis.com/BUCKET_NAME/index.html` | `https://storage.googleapis.com/BUCKET_NAME/index.html#node=123` ✅ |
| カスタムドメイン | `https://example.com/` | `https://example.com/#node=123` ✅ |

### 2. `frontend/src/components/MapView.tsx`

`window.location.pathname` によるパース（`/node/{id}`）を
`window.location.hash` によるパース（`#node={id}`）に変更する。

```js
// 修正前
const urlOsmNodeId = useMemo(() => {
  const match = window.location.pathname.match(/^\/node\/(\d+)$/);
  return match?.[1] ?? null;
}, []);

// 修正後
const urlOsmNodeId = useMemo(() => {
  const match = window.location.hash.match(/^#node=(\d+)$/);
  return match?.[1] ?? null;
}, []);
```

**動作確認：**

| 環境 | hash | マッチ結果 |
|------|------|-----------|
| ローカル | `#node=123` | `123` ✅ |
| GCS | `#node=123` | `123` ✅ |
| 通常ページ（ハッシュなし） | `` | `null` ✅ |

### 3. `deploy.yml` の変更

**不要。** GCS への設定変更は一切不要。

---

## 修正ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `frontend/src/components/JunctionPopup.tsx` | URL 生成を `origin + pathname + #node={id}` に変更 |
| `frontend/src/components/MapView.tsx` | `pathname` ベースのパースを `hash` ベースに変更 |

---

## 検証方法

### ローカル検証

```bash
cd frontend && npm run dev
# http://localhost:3000/#node=581715516 に直接アクセスして動作確認
```

### 本番検証

1. main にマージ・デプロイ
2. `https://storage.googleapis.com/BUCKET_NAME/index.html#node=581715516` に直接アクセス
3. マーカーにフォーカスされた状態でアプリが表示されることを確認
