interface StatsDisplayProps {
  count: number;
  isLoading: boolean;
}

export function StatsDisplay({ count, isLoading }: StatsDisplayProps) {
  return (
    <div
      style={{
        padding: '12px 20px',
        background: 'white',
        borderBottom: '1px solid #e0e0e0',
        fontSize: 14,
      }}
    >
      {isLoading ? (
        <span style={{ color: '#666' }}>読み込み中...</span>
      ) : (
        <span>
          <strong>{count}</strong> 件のY字路が見つかりました
        </span>
      )}
    </div>
  );
}
