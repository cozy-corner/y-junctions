import { useState } from 'react';
import { MapView } from './components/MapView';
import { FilterPanel } from './components/FilterPanel';
import { StatsDisplay } from './components/StatsDisplay';
import { useFilters } from './hooks/useFilters';

function App() {
  const [isLoading, setIsLoading] = useState(false);
  const [totalCount, setTotalCount] = useState(0);

  // フィルタ状態管理
  const {
    angleTypes,
    minAngleLt,
    minAngleGt,
    toggleAngleType,
    setMinAngleLt,
    setMinAngleGt,
    resetFilters,
    toFilterParams,
  } = useFilters();

  // フィルタパラメータ
  const filterParams = toFilterParams();

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'sans-serif',
      }}
    >
      {/* ヘッダー */}
      <header
        style={{
          padding: '15px 20px',
          background: '#2c3e50',
          color: 'white',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
        }}
      >
        <h1 style={{ margin: 0, fontSize: 24 }}>Y字路マップ</h1>
      </header>

      {/* メインコンテンツ */}
      <main style={{ flex: 1, display: 'flex', overflow: 'hidden' }}>
        {/* 左サイドバー */}
        <aside
          style={{
            width: 300,
            display: 'flex',
            flexDirection: 'column',
            borderRight: '1px solid #e0e0e0',
          }}
        >
          {/* 統計表示 */}
          <StatsDisplay count={totalCount} isLoading={isLoading} />

          {/* フィルターパネル */}
          <div style={{ flex: 1, overflow: 'auto' }}>
            <FilterPanel
              angleTypes={angleTypes}
              minAngleLt={minAngleLt}
              minAngleGt={minAngleGt}
              onToggleAngleType={toggleAngleType}
              onMinAngleLtChange={setMinAngleLt}
              onMinAngleGtChange={setMinAngleGt}
              onReset={resetFilters}
            />
          </div>
        </aside>

        {/* 右側の地図 */}
        <div style={{ flex: 1 }}>
          <MapView
            useMockData={true}
            filters={filterParams}
            onLoadingChange={setIsLoading}
            onDataChange={data => setTotalCount(data?.total_count ?? 0)}
          />
        </div>
      </main>
    </div>
  );
}

export default App;
