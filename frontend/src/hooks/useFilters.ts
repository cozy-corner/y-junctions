import { useState, useCallback } from 'react';
import type { AngleType, FilterParams } from '../types';

export interface FilterState {
  angleTypes: AngleType[];
  minAngleRange: [number, number];
}

export function useFilters() {
  const [angleTypes, setAngleTypes] = useState<AngleType[]>(['verysharp', 'sharp', 'skewed', 'normal']);
  const [minAngleRange, setMinAngleRange] = useState<[number, number]>([0, 60]);

  // フィルタをリセット
  const resetFilters = useCallback(() => {
    setAngleTypes(['verysharp', 'sharp', 'skewed', 'normal']);
    setMinAngleRange([0, 60]);
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

    // minAngleRangeが初期値(0, 60)でない場合のみ送信
    if (minAngleRange[0] > 0) {
      params.min_angle_gt = minAngleRange[0];
    }

    if (minAngleRange[1] < 60) {
      params.min_angle_lt = minAngleRange[1];
    }

    return params;
  }, [angleTypes, minAngleRange]);

  return {
    // 状態
    angleTypes,
    minAngleRange,

    // セッター
    setAngleTypes,
    setMinAngleRange,
    toggleAngleType,

    // アクション
    resetFilters,
    toFilterParams,
  };
}
