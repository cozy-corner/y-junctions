# 設計: Y字路マーカーへのURLリンク機能

## ユーザー体験の流れ

```
【共有する側】
マーカーをクリック
  → ポップアップが開く（Leaflet既存動作）
  → 「URLをコピー」ボタンでクリップボードにコピー
  ※ pushState は不要。ポップアップにコピーボタンがあれば十分。

【受け取る側】
/node/XXX のURLを開く
  → GET /api/junctions/node/:osm_node_id でY字路の座標を取得
  → MapContainerの center に直接設定（flyToアニメーションなし）
  → データロード後、対象マーカーのポップアップを自動で開く
```

---

## 正しい設計

```
App.tsx         変更なし（selectedOsmNodeId を持たない）

MapView.tsx     window.location.pathname から osm_node_id を const で取得
                ↓
                osm_node_id があれば fetchJunctionByOsmNodeId で座標取得
                → initialCenter として MapContainer を描画（東京経由なし）
                ↓
                各 JunctionMarker に isSelected を渡す
                （feature.properties.osm_node_id === url の osm_node_id）

JunctionMarker  isSelected=true のとき useEffect で openPopup()
                handleClick は不要（pushState なし、onSelect なし）

JunctionPopup   URLコピーボタン（type="button"、aria-label あり）

Backend         GET /api/junctions/node/:osm_node_id
```

---

## 設計上の決定

### URL形式はパス形式

```
/              → 通常のY字路マップ
/node/123456   → 特定のY字路を指定して開く
```

ルーティングライブラリは使わず、`window.location.pathname` を直接パースする。

```ts
const match = window.location.pathname.match(/^\/node\/(\d+)$/);
const osmNodeId = match?.[1]; // 文字列として取得
```

### osm_node_id は文字列として扱う

`osm_node_id` は `BIGINT`。JavaScriptの `Number` は53bitまでしか精度を保証しないため、
URLの読み取り・APIパスへの受け渡しは **文字列のまま** 行う。

### boundsの初期化問題を合わせて修正する

LeafletはDOMセットアップ中（`useEffect` より前）に `moveend` を発火するため、
`useMapEvents` がそれを拾えず `bounds` が `null` のまま残る問題がある。

`InitialBounds` コンポーネントをマップ内に置き、マウント時に
`useMap().getBounds()` で初期boundsを設定することで解消する。

---

## 変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `backend/src/api/handlers.rs` | `GET /api/junctions/node/:osm_node_id` エンドポイントを追加 |
| `backend/src/api/routes.rs` | 上記ルートを登録 |
| `backend/src/db/repository.rs` | `find_by_osm_node_id()` を追加 |
| `frontend/src/api/client.ts` | `fetchJunctionByOsmNodeId(osmNodeId: string)` を追加 |
| `frontend/src/components/MapView.tsx` | `InitialBounds` / `JunctionMarker` の追加、ポップアップ自動オープン |
| `frontend/src/components/JunctionPopup.tsx` | 「URLをコピー」ボタンを追加 |

---

## PR #187 の失敗から学んだこと（絶対に忘れるな）

### 根本原因

**`pushState` という不要なコードを追加したことが起点。**

ポップアップに「URLをコピー」ボタンがあれば十分なのに、
マーカークリック時にアドレスバーも更新しようとした。

### 連鎖の構造

```
pushState を追加
  ↓
「クリック後に selectedOsmNodeId が更新されない」問題が生まれる
  ↓
onSelectOsmNodeId コールバックチェーンを追加（App.tsx → MapView → JunctionMarker）
  ↓
「同一マーカーを複数回クリックすると重複履歴」問題が生まれる
  ↓
pathname 比較ガードを追加
  ↓
pushState 自体が不要と判明して削除
  ↓
コールバックチェーンが残ったまま（削除し忘れ）
```

### 設計上の誤り

**`selectedOsmNodeId` を App.tsx で `useState` として持ったこと。**

- `selectedOsmNodeId` は「URLから読んだ初期値」であり、マウント後に変化しない
- それなのに `useState` にしたせいで「クリックで更新しなければならない」という誤った要件が生まれた
- App.tsx が知る必要はなく、MapView 内で `const` として完結できる

### 正しい考え方

| 問い | 答え |
|------|------|
| マーカークリック時にURLを更新すべきか？ | **不要**。コピーボタンがある |
| selectedOsmNodeId はどこで持つか？ | **MapView 内の const**。マウント後に変化しない |
| クリックで selectedOsmNodeId を更新すべきか？ | **不要**。URL直アクセスの初期値でしかない |
| handleClick / onSelect は必要か？ | **不要**。pushState がないなら何もしなくていい |

### 実装前に自問すること

- **これは設計に書いてあるか？** → 書いていないなら追加しない
- **このstateはマウント後に変化するか？** → 変化しないなら const で十分
- **このコールバックは何のために存在するか？** → 答えられなければ不要
