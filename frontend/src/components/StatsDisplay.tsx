import { memo } from 'react';

interface StatsDisplayProps {
  count: number;
  isLoading: boolean;
}

const MARKER_WARNING_THRESHOLD = 1000;

export const StatsDisplay = memo(function StatsDisplay({ count, isLoading }: StatsDisplayProps) {
  const showWarning = !isLoading && count >= MARKER_WARNING_THRESHOLD;

  return (
    <div>
      <div className="stats-display">
        {isLoading ? (
          <span className="stats-loading">読み込み中...</span>
        ) : (
          <span className="stats-count">
            <strong>{count}</strong> 件のY字路が見つかりました
          </span>
        )}
      </div>

      {/* 大量マーカー警告 */}
      {showWarning && (
        <div className="marker-warning">
          <span className="marker-warning-icon">⚠️</span>
          <span>
            大量のマーカーが表示されています。地図の動作が重くなる可能性があります。フィルタで絞り込むことをお勧めします。
          </span>
        </div>
      )}
    </div>
  );
});
