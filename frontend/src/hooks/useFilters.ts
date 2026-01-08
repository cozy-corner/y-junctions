import { useState, useCallback } from 'react';
import type { AngleType, RoadCategory, FilterParams } from '../types';

export interface FilterState {
  angleTypes: AngleType[];
  minAngleRange: [number, number];
  elevationDiffRange: [number, number]; // 変更: number | null → [number, number]
  categories: RoadCategory[]; // 道路カテゴリ
}

export function useFilters() {
  const [angleTypes, setAngleTypes] = useState<AngleType[]>(['verysharp', 'sharp', 'normal']);
  const [minAngleRange, setMinAngleRange] = useState<[number, number]>([0, 60]);
  const [elevationDiffRange, setElevationDiffRange] = useState<[number, number]>([0, 10]);
  const [categories, setCategories] = useState<RoadCategory[]>([
    'highway',
    'major',
    'local',
    'pedestrian',
  ]);

  // フィルタをリセット
  const resetFilters = useCallback(() => {
    setAngleTypes(['verysharp', 'sharp', 'normal']);
    setMinAngleRange([0, 60]);
    setElevationDiffRange([0, 10]);
    setCategories(['highway', 'major', 'local', 'pedestrian']);
  }, []);

  // angle_typeの切り替え
  const toggleAngleType = useCallback((type: AngleType) => {
    setAngleTypes(prev => {
      if (prev.includes(type)) {
        return prev.filter(t => t !== type);
      } else {
        return [...prev, type];
      }
    });
  }, []);

  // categoryの切り替え
  const toggleCategory = useCallback((category: RoadCategory) => {
    setCategories(prev => {
      if (prev.includes(category)) {
        return prev.filter(c => c !== category);
      } else {
        return [...prev, category];
      }
    });
  }, []);

  // API用のFilterParamsに変換
  const toFilterParams = useCallback((): Omit<FilterParams, 'bbox'> => {
    const params: Omit<FilterParams, 'bbox'> = {};

    // angle_typeを配列として送る（0個または3個の場合は送らない＝全選択扱い）
    if (angleTypes.length > 0 && angleTypes.length < 3) {
      params.angle_type = angleTypes;
    }

    // minAngleRangeが初期値(0, 60)でない場合のみ送信
    if (minAngleRange[0] > 0) {
      params.min_angle_gt = minAngleRange[0];
    }

    if (minAngleRange[1] < 60) {
      params.min_angle_lt = minAngleRange[1];
    }

    // 標高差フィルタ（初期値[0, 10]でない場合のみ送信）
    if (elevationDiffRange[0] > 0) {
      params.min_angle_elevation_diff = elevationDiffRange[0];
    }
    if (elevationDiffRange[1] < 10) {
      params.max_angle_elevation_diff = elevationDiffRange[1];
    }

    // categoryを配列として送る（0個または4個の場合は送らない＝全選択扱い）
    if (categories.length > 0 && categories.length < 4) {
      params.category = categories;
    }

    return params;
  }, [angleTypes, minAngleRange, elevationDiffRange, categories]);

  return {
    // 状態
    angleTypes,
    minAngleRange,
    elevationDiffRange,
    categories,

    // セッター
    setAngleTypes,
    setMinAngleRange,
    setElevationDiffRange,
    setCategories,
    toggleAngleType,
    toggleCategory,

    // アクション
    resetFilters,
    toFilterParams,
  };
}
