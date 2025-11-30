import { useState, useCallback } from 'react';
import type { AngleType, FilterParams } from '../types';

export interface FilterState {
  angleTypes: AngleType[];
  minAngleLt: number | null;
  minAngleGt: number | null;
}

export function useFilters() {
  const [angleTypes, setAngleTypes] = useState<AngleType[]>(['sharp', 'even', 'skewed', 'normal']);
  const [minAngleLt, setMinAngleLt] = useState<number | null>(null);
  const [minAngleGt, setMinAngleGt] = useState<number | null>(null);

  // フィルタをリセット
  const resetFilters = useCallback(() => {
    setAngleTypes(['sharp', 'even', 'skewed', 'normal']);
    setMinAngleLt(null);
    setMinAngleGt(null);
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

  // API用のFilterParamsに変換
  const toFilterParams = useCallback((): Omit<FilterParams, 'bbox'> => {
    const params: Omit<FilterParams, 'bbox'> = {};

    // angle_typeを配列として送る（0個または4個の場合は送らない＝全選択扱い）
    if (angleTypes.length > 0 && angleTypes.length < 4) {
      params.angle_type = angleTypes;
    }

    if (minAngleLt !== null) {
      params.min_angle_lt = minAngleLt;
    }

    if (minAngleGt !== null) {
      params.min_angle_gt = minAngleGt;
    }

    return params;
  }, [angleTypes, minAngleLt, minAngleGt]);

  return {
    // 状態
    angleTypes,
    minAngleLt,
    minAngleGt,

    // セッター
    setAngleTypes,
    setMinAngleLt,
    setMinAngleGt,
    toggleAngleType,

    // アクション
    resetFilters,
    toFilterParams,
  };
}
