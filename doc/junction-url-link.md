# 設計: Y字路マーカーへのURLリンク機能

## ユーザー体験の流れ

```
【共有する側】
マーカーをクリック
  → ポップアップが開く（Leaflet既存動作）
  → URLが /node/XXX に変わる（pushState）
  → 「URLをコピー」ボタンでクリップボードにコピー

【受け取る側】
/node/XXX のURLを開く
  → GET /api/junctions/node/:osm_node_id でY字路の座標を取得
  → マップをその座標に移動（flyTo）
  → データロード後、対象マーカーのポップアップを自動で開く
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
| `frontend/src/App.tsx` | pathname から osm_node_id を取得し `MapView` に渡す |
| `frontend/src/components/MapView.tsx` | `InitialBounds` / `MapFlyTo` の追加、マーカークリック時URL更新、ポップアップ自動オープン |
| `frontend/src/components/JunctionPopup.tsx` | 「URLをコピー」ボタンを追加 |

---

## 各コンポーネントの設計

### バックエンド

**新エンドポイント**
```
GET /api/junctions/node/:osm_node_id
→ osm_node_id で DB検索（UNIQUE制約あり）
→ 既存の Junction レスポンス形式で返す
→ 存在しない場合は 404
```

### App.tsx

```
window.location.pathname を /^\/node\/(\d+)$/ でパース
→ マッチすれば osm_node_id を文字列として取得
→ MapView に selectedOsmNodeId?: string として渡す
```

### MapView.tsx に追加する要素

**InitialBounds（新コンポーネント）**
```
useMap() でマウント時の現在boundsを取得
→ handleBoundsChange を呼んで bounds を初期化
```

**MapFlyTo（新コンポーネント）**
```
selectedOsmNodeId があれば
  → fetchJunctionByOsmNodeId(id) でY字路の座標を取得
  → map.flyTo([lat, lon], 16) でマップを移動
```

**各マーカーの eventHandlers**
```
click 時に window.history.pushState で /node/XXX をURLに反映
※ Leafletのポップアップ表示は既存動作のまま
```

**selectedOsmNodeId と一致するマーカー**
```
初回レンダリング時に useEffect で markerRef.current.openPopup() を呼ぶ
（MapFlyTo でマップ移動 → データロード完了と同時にポップアップが開く）
```

### JunctionPopup.tsx

```
「URLをコピー」ボタン
  → origin + /node/${osm_node_id} でURLを生成
  → navigator.clipboard.writeText でコピー
  → 2秒間「コピーしました！」と表示
```

---

## 検証方法

1. マーカーをクリック → URLが `/node/XXX` に変わることを確認
2. そのURLを別タブで開く → 同じマーカーにジャンプしポップアップが開くことを確認
3. 「URLをコピー」でクリップボードにURLが入ることを確認
