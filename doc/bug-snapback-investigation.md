# バグ調査: ドラッグ後に元の場所へ戻る挙動

## 概要

地図をパンしたり、スライダーを操作した後に「元の場所に戻される」ように見える挙動がある。

**結論: `useJunctions.ts` のレースコンディションが原因**

---

## 調査対象ファイル

- `frontend/src/hooks/useJunctions.ts`
- `frontend/src/api/client.ts`
- `frontend/src/components/MapView.tsx`
- `frontend/src/components/FilterPanel.tsx`
- `frontend/src/hooks/useFilters.ts`
- `frontend/src/App.tsx`

---

## 根本原因: フェッチのキャンセル漏れ

### 問題のコード

`useJunctions.ts` のデバウンス処理はタイマーのみキャンセルする。**すでに開始済みの HTTP リクエストはキャンセルされない。**

```ts
// useJunctions.ts:86-88 (クリーンアップ関数)
return () => {
  if (timeoutRef.current !== null) {
    clearTimeout(timeoutRef.current); // タイマーのみ。フェッチは継続
  }
};
```

`api/client.ts:50` の `fetchJunctions` も `AbortSignal` を受け取っておらず、フェッチを中断できない。

### 発生シナリオ（地図パン）

```
T=0ms:    エリアAで600ms停止 → デバウンス発火 → Aフェッチ開始
T=100ms:  ユーザーがエリアBへパン → 新デバウンス開始
             cleanup 呼び出し → タイマークリアのみ、Aフェッチは継続中
T=700ms:  デバウンス発火 → Bフェッチ開始
T=800ms:  Bフェッチ完了 → setData(Bのデータ) → Bのマーカーが表示される ✓
T=1500ms: Aフェッチ完了 → setData(Aのデータ) → AのマーカーがBの地図上に出現 ❌
```

ユーザーはエリアBを見ているのに、エリアAの古いデータが後から上書きし、
「元の場所（エリアA）に戻された」ように見える。

### 発生シナリオ（スライダー操作）

```
T=0ms:    スライダーを動かして一瞬止まる → デバウンス発火 → フィルタAフェッチ開始
T=200ms:  スライダーをさらに動かす → 新デバウンス開始（フィルタAフェッチは継続中）
T=800ms:  デバウンス発火 → フィルタBフェッチ開始
T=850ms:  フィルタBフェッチ完了 → setData(フィルタBのデータ) ✓
T=1500ms: フィルタAフェッチ完了 → setData(フィルタAのデータ) → 古い結果に戻る ❌
```

---

## 再現条件

| 条件 | 詳細 |
|------|------|
| 発生しやすい操作 | 地図を素早くパン / スライダーを素早く操作（デバウンス発火後すぐに再操作） |
| ネットワーク | API レスポンスが遅い場合（>600ms）に顕在化しやすい |
| 開発環境 | ローカルのため API が速く再現しにくい |
| 本番環境 | リモートサーバー＋ネットワーク遅延で発生しやすい |

---

## 修正方針

`AbortController` を使って新しいフェッチ開始時に古いフェッチをキャンセルする。

### `useJunctions.ts` の修正イメージ

```ts
const abortControllerRef = useRef<AbortController | null>(null);

useEffect(() => {
  if (!bounds) return;

  if (timeoutRef.current !== null) {
    clearTimeout(timeoutRef.current);
  }

  timeoutRef.current = window.setTimeout(async () => {
    // 古いフェッチをキャンセル
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setIsLoading(true);
    setError(null);

    try {
      const bbox = `${bounds.west},${bounds.south},${bounds.east},${bounds.north}`;
      const result = await fetchJunctions(bbox, filters, controller.signal);
      setData(result);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') {
        return; // キャンセルされた場合は何もしない
      }
      // ... エラー処理
    } finally {
      setIsLoading(false);
    }
  }, debounceMs);

  return () => {
    if (timeoutRef.current !== null) {
      clearTimeout(timeoutRef.current);
    }
    // クリーンアップ時もフェッチをキャンセル
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
  };
}, [bounds, filters, debounceMs, useMockData]);
```

### `api/client.ts` の修正イメージ

```ts
export async function fetchJunctions(
  bbox: string,
  filters?: Omit<FilterParams, 'bbox'>,
  signal?: AbortSignal  // 追加
): Promise<JunctionFeatureCollection> {
  // ...
  const response = await fetch(url, { signal }); // signal を渡す
  // ...
}
```

---

## 調査で確認した「問題ではない」事項

以下は調査したが問題ではないことを確認した。

- **`MapContainer` が初期位置に戻る**: Leaflet の `center`/`zoom` は初期化専用プロパティ。再レンダリングで地図位置はリセットされない
- **`MapView` の remount**: key プロパティなし、条件付きレンダリングなし。remount は発生しない
- **`filterParams` の参照不安定性**: `useMemo` + `useCallback` で適切にメモ化されており、不要な再レンダリングは最小化されている
- **`MapEventsHandler` のクロージャ問題**: `handleBoundsChange` は `useCallback([])` で安定。`MapEventsHandler` は `memo` により不要な再レンダリングを防いでいる
- **スライダーの拘束スナップ**: `Math.min/max` によりスライダーが境界でスナップする仕様があるが、これは軽微で「元の場所に戻る」という主訴とは別の問題

---

## まとめ

| 項目 | 内容 |
|------|------|
| **根本原因** | `useJunctions.ts` にて古い HTTP フェッチがキャンセルされない |
| **影響箇所** | `useJunctions.ts`, `api/client.ts` |
| **修正方法** | `AbortController` でフェッチをキャンセル |
| **優先度** | 高（本番環境で再現しやすい）|
