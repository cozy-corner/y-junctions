import { memo } from 'react';
import type { AngleType } from '../types';

interface FilterPanelProps {
  angleTypes: AngleType[];
  minAngleRange: [number, number];
  elevationDiffRange: [number, number]; // 変更
  onToggleAngleType: (type: AngleType) => void;
  onMinAngleRangeChange: (range: [number, number]) => void;
  onElevationDiffRangeChange: (range: [number, number]) => void; // 変更
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
  elevationDiffRange,
  onToggleAngleType,
  onMinAngleRangeChange,
  onElevationDiffRangeChange,
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
            <span>範囲</span>
            <span>
              {elevationDiffRange[0]}m 〜{' '}
              {elevationDiffRange[1] === 10 ? '10m以上' : `${elevationDiffRange[1]}m`}
            </span>
          </div>

          {/* 最小値スライダー */}
          <div style={{ marginBottom: 8 }}>
            <label style={{ fontSize: 12, color: '#666' }}>最小値: {elevationDiffRange[0]}m</label>
            <input
              type="range"
              min="0"
              max="10"
              step="0.5"
              value={elevationDiffRange[0]}
              onChange={e => {
                const newMin = Math.min(Number(e.target.value), elevationDiffRange[1] - 0.5);
                onElevationDiffRangeChange([newMin, elevationDiffRange[1]]);
              }}
              className="angle-range-slider"
            />
          </div>

          {/* 最大値スライダー */}
          <div style={{ marginBottom: 12 }}>
            <label style={{ fontSize: 12, color: '#666' }}>
              最大値: {elevationDiffRange[1] === 10 ? '10m以上' : `${elevationDiffRange[1]}m`}
            </label>
            <input
              type="range"
              min="0"
              max="10"
              step="0.5"
              value={elevationDiffRange[1]}
              onChange={e => {
                const newMax = Math.max(Number(e.target.value), elevationDiffRange[0] + 0.5);
                onElevationDiffRangeChange([elevationDiffRange[0], newMax]);
              }}
              className="angle-range-slider"
            />
          </div>

          <button
            onClick={() => onElevationDiffRangeChange([0, 10])}
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
