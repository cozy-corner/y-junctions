# 本番環境白画面バグ調査レポート

## 概要

2026-03-03 に本番環境（GCS）で白画面が発生。

- 発生URL: https://storage.googleapis.com/y-junctions-prod-frontend/index.html
- 原因コミット: `a44e709 feat: Y字路マーカーへのURLリンク機能を追加する`

## 根本原因

`frontend/vite.config.ts` の `base` 設定が変更されたことで、ビルド成果物のアセットパスが壊れた。

```diff
- base: './'
+ base: '/'
```

| 設定 | 生成されるアセットパス | GCS での動作 |
|---|---|---|
| `base: './'` | `./assets/index-xxx.js`（相対パス） | ✅ 正常 |
| `base: '/'` | `/assets/index-xxx.js`（絶対パス） | ❌ 404 → 白画面 |

GCS はバケット名を含むサブパス（`/y-junctions-prod-frontend/`）で配信しているため、絶対パス `/assets/...` はバケットルートを指してしまい 404 になる。

## なぜその変更がされたか

コミットメッセージには「SPAルーティングを正常化」と記載されていた。URL直リンク機能（`/node/:osm_node_id`）の導入にともない、絶対パスへの変更が必要だと判断されたと思われる。

## その変更は本当に必要だったか

**不要だった。**

実装（`MapView.tsx:164-167`）を確認すると、React Router は一切使っておらず、`window.location.pathname` を直接読んでいるだけ：

```ts
const urlOsmNodeId = useMemo(() => {
  const match = window.location.pathname.match(/^\/node\/(\d+)$/);
  return match?.[1] ?? null;
}, []);
```

Vite の `base` 設定はアセットパスの生成にのみ影響し、`window.location.pathname` の読み取りとは無関係。

加えて、`base: '/'` に変えたところで URL 直リンク（`/node/12345` を直接開く）は GCS では動かない。GCS にはそのパスのファイルが存在しないため 404 になるからで、`base` 設定とは無関係の問題。

| | `base: './'` | `base: '/'` |
|---|---|---|
| GCS でのアセット読み込み | ✅ 動く | ❌ 白画面 |
| `/node/12345` を直接開く | ❌ GCSが404 | ❌ GCSが404 |

どちらにしても URL 直リンクの直接アクセスは GCS では動かないため、`base` を変更した意味がなかった。

## 暫定対応

`base: './'` に戻す。白画面は即座に解消される。

URL 直リンクの動作は変更前後で変わらない（GCS では不可）。

## 恒久対応

URL 直リンク（`/node/:osm_node_id` への直接アクセス）を正しく動かすには、サーバー側で全パスを `index.html` にフォールバックする仕組みが必要。選択肢：

1. **Firebase Hosting**（推奨）: `firebase.json` の `rewrites` 1行で対応。最小工数。
2. **Cloud CDN + Load Balancer**: GCS のまま対応可能だがインフラが複雑化。
3. **Cloud Run（Nginx）**: `try_files` でSPAルーティング対応。バックエンドと統一できる。
