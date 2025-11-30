import type { AngleType } from '../types';

interface FilterPanelProps {
  angleTypes: AngleType[];
  minAngleLt: number | null;
  minAngleGt: number | null;
  onToggleAngleType: (type: AngleType) => void;
  onMinAngleLtChange: (value: number | null) => void;
  onMinAngleGtChange: (value: number | null) => void;
  onReset: () => void;
}

const ANGLE_TYPE_LABELS: Record<AngleType, string> = {
  sharp: '鋭角',
  even: '均等',
  skewed: '歪み',
  normal: '通常',
};

const ANGLE_TYPE_COLORS: Record<AngleType, string> = {
  sharp: '#ff4444',
  even: '#44ff44',
  skewed: '#4444ff',
  normal: '#ffaa00',
};

export function FilterPanel({
  angleTypes,
  minAngleLt,
  minAngleGt,
  onToggleAngleType,
  onMinAngleLtChange,
  onMinAngleGtChange,
  onReset,
}: FilterPanelProps) {
  return (
    <div
      style={{
        padding: '20px',
        background: '#f8f9fa',
        height: '100%',
        overflowY: 'auto',
      }}
    >
      <h2 style={{ marginTop: 0, marginBottom: 20, fontSize: 18 }}>フィルター</h2>

      {/* 角度タイプ */}
      <div style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 14, marginBottom: 12, fontWeight: 600 }}>角度タイプ</h3>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {(['sharp', 'even', 'skewed', 'normal'] as AngleType[]).map(type => (
            <label
              key={type}
              style={{
                display: 'flex',
                alignItems: 'center',
                cursor: 'pointer',
                gap: 8,
              }}
            >
              <input
                type="checkbox"
                checked={angleTypes.includes(type)}
                onChange={() => onToggleAngleType(type)}
                style={{ cursor: 'pointer' }}
              />
              <span
                style={{
                  width: 16,
                  height: 16,
                  borderRadius: '50%',
                  backgroundColor: ANGLE_TYPE_COLORS[type],
                  display: 'inline-block',
                }}
              />
              <span>{ANGLE_TYPE_LABELS[type]}</span>
            </label>
          ))}
        </div>
      </div>

      {/* 最小角度範囲 */}
      <div style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 14, marginBottom: 12, fontWeight: 600 }}>最小角度</h3>

        {/* 最小角度 未満 */}
        <div style={{ marginBottom: 16 }}>
          <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span>未満 (度)</span>
              <span>{minAngleLt !== null ? `${minAngleLt}°` : '指定なし'}</span>
            </div>
            <input
              type="range"
              min="0"
              max="180"
              value={minAngleLt ?? 90}
              onChange={e => onMinAngleLtChange(Number(e.target.value))}
              style={{ width: '100%', cursor: 'pointer' }}
            />
            <button
              onClick={() => onMinAngleLtChange(null)}
              style={{
                padding: '4px 8px',
                fontSize: 11,
                cursor: 'pointer',
                border: '1px solid #ccc',
                background: 'white',
                borderRadius: 4,
              }}
            >
              クリア
            </button>
          </label>
        </div>

        {/* 最小角度 より大きい */}
        <div style={{ marginBottom: 16 }}>
          <label style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 12 }}>
              <span>より大きい (度)</span>
              <span>{minAngleGt !== null ? `${minAngleGt}°` : '指定なし'}</span>
            </div>
            <input
              type="range"
              min="0"
              max="180"
              value={minAngleGt ?? 90}
              onChange={e => onMinAngleGtChange(Number(e.target.value))}
              style={{ width: '100%', cursor: 'pointer' }}
            />
            <button
              onClick={() => onMinAngleGtChange(null)}
              style={{
                padding: '4px 8px',
                fontSize: 11,
                cursor: 'pointer',
                border: '1px solid #ccc',
                background: 'white',
                borderRadius: 4,
              }}
            >
              クリア
            </button>
          </label>
        </div>
      </div>

      {/* リセットボタン */}
      <button
        onClick={onReset}
        style={{
          width: '100%',
          padding: '10px',
          fontSize: 14,
          fontWeight: 600,
          cursor: 'pointer',
          border: '1px solid #2c3e50',
          background: '#2c3e50',
          color: 'white',
          borderRadius: 4,
        }}
      >
        フィルターをリセット
      </button>
    </div>
  );
}
