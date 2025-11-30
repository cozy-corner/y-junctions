import { useState, useEffect, useRef } from 'react';
import { fetchJunctions } from '../api/client';
import type { JunctionFeatureCollection, FilterParams, LatLngBounds } from '../types';

// モックデータ（バックエンドがない場合に使用）
const MOCK_DATA: JunctionFeatureCollection = {
  type: 'FeatureCollection',
  features: [
    {
      type: 'Feature',
      geometry: {
        type: 'Point',
        coordinates: [139.7671, 35.6812], // 東京駅
      },
      properties: {
        id: 1,
        osm_node_id: 1001,
        angles: [45, 135, 180],
        angle_type: 'sharp',
        road_types: ['residential', 'residential', 'tertiary'],
        streetview_url: 'https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=35.6812,139.7671',
      },
    },
    {
      type: 'Feature',
      geometry: {
        type: 'Point',
        coordinates: [139.77, 35.682],
      },
      properties: {
        id: 2,
        osm_node_id: 1002,
        angles: [110, 120, 130],
        angle_type: 'even',
        road_types: ['primary', 'secondary', 'tertiary'],
        streetview_url: 'https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=35.682,139.770',
      },
    },
    {
      type: 'Feature',
      geometry: {
        type: 'Point',
        coordinates: [139.765, 35.68],
      },
      properties: {
        id: 3,
        osm_node_id: 1003,
        angles: [30, 100, 230],
        angle_type: 'skewed',
        road_types: ['residential', 'unclassified', 'living_street'],
        streetview_url: 'https://www.google.com/maps/@?api=1&map_action=pano&viewpoint=35.680,139.765',
      },
    },
  ],
  total_count: 3,
};

interface UseJunctionsOptions {
  bounds: LatLngBounds | null;
  filters?: Omit<FilterParams, 'bbox'>;
  debounceMs?: number;
  useMockData?: boolean;
}

export function useJunctions({
  bounds,
  filters,
  debounceMs = 300,
  useMockData = false,
}: UseJunctionsOptions) {
  const [data, setData] = useState<JunctionFeatureCollection | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const timeoutRef = useRef<number | null>(null);

  useEffect(() => {
    // boundsがない場合は何もしない
    if (!bounds) {
      return;
    }

    // 前のタイムアウトをクリア
    if (timeoutRef.current !== null) {
      clearTimeout(timeoutRef.current);
    }

    // デバウンス処理
    timeoutRef.current = window.setTimeout(async () => {
      setIsLoading(true);
      setError(null);

      try {
        if (useMockData) {
          // モックデータを使用
          await new Promise(resolve => setTimeout(resolve, 300)); // ローディング表現のため遅延
          setData(MOCK_DATA);
        } else {
          // 実際のAPIを呼び出し
          const bbox = `${bounds.west},${bounds.south},${bounds.east},${bounds.north}`;
          const result = await fetchJunctions(bbox, filters);
          setData(result);
        }
      } catch (err) {
        console.error('Failed to fetch junctions:', err);
        setError(err instanceof Error ? err : new Error('Unknown error'));
        // エラー時はモックデータにフォールバック
        if (!useMockData) {
          console.warn('Falling back to mock data');
          setData(MOCK_DATA);
        }
      } finally {
        setIsLoading(false);
      }
    }, debounceMs);

    // クリーンアップ
    return () => {
      if (timeoutRef.current !== null) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, [bounds, filters, debounceMs, useMockData]);

  return {
    data,
    isLoading,
    error,
  };
}
