# 主要都市ジャンプ機能 設計

作成日: 2026-08-08
ブランチ: `feature/city-jump`

## 目的

地図上で主要都市へワンアクションでジャンプできる機能を追加する。ユーザーが
サイドバーのドロップダウンから都市を選ぶと、地図がその都市中心へ滑らかに移動し、
既存の bounds 連動ロジックにより周辺の Y 字路が自動でロードされる。

## スコープ

- 対象: フロントエンドのみ（バックエンド変更なし）
- 都市リストは**手書きの固定リスト**。データから動的生成はしない。
- 収録都市は下記の 12 都市（各都市圏の代表 1 つ。近接都市は統合済み）。

## 収録都市

座標は各都市中心。全都市共通ズーム `14`。全 12 エントリ（日本 7 + 韓国 2 + 台湾 2 + 香港 1）。

| 国/地域 | 都市 | lon | lat |
|---|---|---|---|
| 日本 | 東京 | 139.767 | 35.681 |
| 日本 | 名古屋 | 136.906 | 35.170 |
| 日本 | 大阪 | 135.500 | 34.694 |
| 日本 | 広島 | 132.460 | 34.390 |
| 日本 | 福岡 | 130.401 | 33.590 |
| 日本 | 仙台 | 140.872 | 38.268 |
| 日本 | 札幌 | 141.347 | 43.062 |
| 韓国 | ソウル | 126.978 | 37.567 |
| 韓国 | 釜山 | 129.076 | 35.180 |
| 台湾 | 台北 | 121.565 | 25.033 |
| 台湾 | 高雄 | 120.301 | 22.627 |
| 香港 | 香港 | 114.170 | 22.320 |

除外したもの: 横浜(→東京), 京都・神戸(→大阪), 台中(台北/高雄で南北代表), マカオ(香港に近接), 上海(実験段階のため)。

いずれの収録都市もローカル DB で実データの存在を確認済み（各中心 ±0.15 度に
数千件規模の Y 字路あり）。

## アーキテクチャ

### 新規ファイル

**`frontend/src/data/cities.ts`** — 都市データ

```ts
export interface City {
  name: string;    // 表示名（例: "東京"）
  country: string; // optgroup ラベル（例: "日本"）
  lat: number;
  lon: number;
}

export const CITIES: City[] = [ /* 上表の 12 エントリ */ ];
```

**`frontend/src/components/CityJumpSelect.tsx`** — サイドバーの都市選択 `<select>`

- `country` ごとに `<optgroup>` でグルーピング。
- 先頭にプレースホルダ option（例: 「都市を選択…」, value=""）。
- `onSelect(city: City)` を親に上げる。
- 選択後は `<select>` の値をプレースホルダ("")に戻す（controlled, value="" 固定）。
  これにより**同じ都市を連続選択しても** onChange が発火し再ジャンプできる。

```tsx
interface CityJumpSelectProps {
  onSelect: (city: City) => void;
}
```

### 変更ファイル

**`frontend/src/App.tsx`**

- ジャンプ先 state を追加:
  ```ts
  const [jumpTarget, setJumpTarget] = useState<{ lat: number; lon: number; seq: number } | null>(null);
  ```
- `CityJumpSelect` をサイドバー上部（`StatsDisplay` の上）に配置。
  `onSelect={(c) => setJumpTarget(t => ({ lat: c.lat, lon: c.lon, seq: (t?.seq ?? 0) + 1 }))}`
- `MapView` に `jumpTarget={jumpTarget}` を渡す。

`seq` は「同じ都市の連続選択」でも参照が変わり `useEffect` が再発火するためのカウンタ。

**`frontend/src/components/MapView.tsx`**

- 新 prop `jumpTarget?: { lat: number; lon: number; seq: number } | null` を受ける。
- `MapContainer` 内に `CityFlyTo` コンポーネントを追加（`useMap` を使用）:
  ```tsx
  const CITY_JUMP_ZOOM = 14;

  const CityFlyTo = memo(function CityFlyTo({ target }: { target: JumpTarget | null }) {
    const map = useMap();
    useEffect(() => {
      if (!target) return;
      map.flyTo([target.lat, target.lon], CITY_JUMP_ZOOM, { animate: true, duration: 2 });
    }, [map, target]);
    return null;
  });
  ```
  `useEffect` の依存は `target`（seq が変わるたびに新オブジェクト）。

## データフロー

```
ユーザーが都市を選択
  → CityJumpSelect onChange
  → App: setJumpTarget({lat, lon, seq++})
  → MapView 再レンダー、CityFlyTo の target が変化
  → map.flyTo(...) で滑らかに移動
  → 移動完了で既存の moveend ハンドラ発火
  → bounds 更新 → useJunctions が新 bounds のデータ取得
  → マーカー再描画
```

追加のデータ取得ロジックは不要（既存の bounds 連動をそのまま利用）。

## エッジケース

- **URL フォーカス（#node=...）と競合**: URL 指定時は `FocusAnimation` が初期 flyTo を行う。
  都市ジャンプはユーザー操作後のみ `jumpTarget` が非 null になるため、初期表示時は
  `jumpTarget=null` で発火しない。両者が同時に走ることはない。
- **同一都市の連続選択**: `seq` カウンタで参照が変わるため再発火する。
- **プレースホルダへ戻す**: `<select>` は controlled で常に value="" とし、選択値は state に持たない。

## テスト方針

既存フロントには `.test.tsx` は無いが vitest + @testing-library/react は導入済み。
本機能で最初のフロントテストを追加する。

- **`cities.test.ts`**: `CITIES` が非空 / 各 `lat` が -90..90・`lon` が -180..180 の範囲内 /
  `name`・`country` が非空文字列。
- **`CityJumpSelect.test.tsx`**: レンダー後に都市 option を選択すると、対応する `City`
  オブジェクトで `onSelect` が呼ばれる。プレースホルダ選択では呼ばれない。
- **`flyTo` の実挙動**: Leaflet 依存のため自動テスト対象外。`npm run dev` で手動確認。

## 変更しないもの

- バックエンド（API・SQL・スキーマ）
- `useJunctions` / bounds 連動ロジック
- 既存の URL 直リンク（`#node=...`）とそのフォーカスアニメーション
