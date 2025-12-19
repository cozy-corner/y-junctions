import { memo } from 'react';
import type { AngleType } from '../types';

interface FilterPanelProps {
  angleTypes: AngleType[];
  minAngleRange: [number, number];
  minAngleElevationDiff: number | null;
  onToggleAngleType: (type: AngleType) => void;
  onMinAngleRangeChange: (range: [number, number]) => void;
  onMinAngleElevationDiffChange: (value: number | null) => void;
  onReset: () => void;
}

const ANGLE_TYPE_LABELS: Record<AngleType, string> = {
  verysharp: '超鋭角',
  sharp: '鋭角',
  normal: '広角',
};

const ANGLE_TYPE_COLORS: Record<AngleType, string> = {
  verysharp: '#8B5CF6', // 紫（violet-500） - 最小角度が最も小さい
  sharp: '#3B82F6', // 明るい青
  normal: '#F59E0B', // 濃い黄色（琥珀色） - 通常
};

export const FilterPanel = memo(function FilterPanel({
  angleTypes,
  minAngleRange,
  minAngleElevationDiff,
  onToggleAngleType,
  onMinAngleRangeChange,
  onMinAngleElevationDiffChange,
  onReset,
}: FilterPanelProps) {
  const [minValue, maxValue] = minAngleRange;

  const handleMinChange = (value: number) => {
    // 最小値が最大値を超えないようにする
    const newMin = Math.min(value, maxValue - 1);
    onMinAngleRangeChange([newMin, maxValue]);
  };

  const handleMaxChange = (value: number) => {
    // 最大値が最小値未満にならないようにする
    const newMax = Math.max(value, minValue + 1);
    onMinAngleRangeChange([minValue, newMax]);
  };

  return (
    <div className="filter-panel">
      <h2>フィルター</h2>

      {/* 角度タイプ */}
      <div className="filter-section">
        <h3>角度タイプ</h3>
        <div className="angle-type-options">
          {/* 最小角度の小さい順に並べる */}
          {(['verysharp', 'sharp', 'normal'] as AngleType[]).map(type => (
            <label key={type} className="angle-type-label">
              <input
                type="checkbox"
                checked={angleTypes.includes(type)}
                onChange={() => onToggleAngleType(type)}
                className="angle-type-checkbox"
              />
              <span
                className="angle-type-indicator"
                style={{ backgroundColor: ANGLE_TYPE_COLORS[type] }}
              />
              <span>{ANGLE_TYPE_LABELS[type]}</span>
            </label>
          ))}
        </div>
      </div>

      {/* 最小角度の範囲 */}
      <div className="filter-section">
        <h3>最小角度の範囲</h3>

        <div className="angle-range-control">
          <div className="angle-range-header">
            <span>範囲</span>
            <span>
              {minValue}° 〜 {maxValue}°
            </span>
          </div>

          {/* 最小値スライダー */}
          <div style={{ marginBottom: 8 }}>
            <label style={{ fontSize: 12, color: '#666' }}>最小値: {minValue}°</label>
            <input
              type="range"
              min="0"
              max="60"
              value={minValue}
              onChange={e => handleMinChange(Number(e.target.value))}
              className="angle-range-slider"
            />
          </div>

          {/* 最大値スライダー */}
          <div style={{ marginBottom: 12 }}>
            <label style={{ fontSize: 12, color: '#666' }}>最大値: {maxValue}°</label>
            <input
              type="range"
              min="0"
              max="60"
              value={maxValue}
              onChange={e => handleMaxChange(Number(e.target.value))}
              className="angle-range-slider"
            />
          </div>

          <button
            onClick={() => onMinAngleRangeChange([0, 60])}
            className="angle-range-clear-button"
          >
            リセット
          </button>
        </div>
      </div>

      {/* 標高差フィルタ */}
      <div className="filter-section">
        <h3>最小角度の標高差</h3>
        <div className="angle-range-control">
          <div className="angle-range-header">
            <span>
              {minAngleElevationDiff !== null ? `${minAngleElevationDiff}m以上` : '指定なし'}
            </span>
          </div>

          <div style={{ marginBottom: 12 }}>
            <input
              type="range"
              min="0"
              max="5"
              step="0.5"
              value={minAngleElevationDiff ?? 0}
              onChange={e => {
                const value = Number(e.target.value);
                onMinAngleElevationDiffChange(value > 0 ? value : null);
              }}
              className="angle-range-slider"
            />
          </div>

          <button
            onClick={() => onMinAngleElevationDiffChange(null)}
            className="angle-range-clear-button"
          >
            リセット
          </button>
        </div>
      </div>

      {/* リセットボタン */}
      <button onClick={onReset} className="filter-reset-button">
        フィルターをリセット
      </button>
    </div>
  );
});
