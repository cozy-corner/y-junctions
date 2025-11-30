// Test comment for pre-commit hook verification
import { useState, useCallback, useMemo } from 'react';
import { MapView } from './components/MapView';
import { FilterPanel } from './components/FilterPanel';
import { StatsDisplay } from './components/StatsDisplay';
import { useFilters } from './hooks/useFilters';
import type { JunctionFeatureCollection } from './types';
import './App.css';

function App() {
  const [isLoading, setIsLoading] = useState(false);
  const [totalCount, setTotalCount] = useState(0);
  const [isSidebarOpen, setIsSidebarOpen] = useState(false);

  // フィルタ状態管理
  const {
    angleTypes,
    minAngleRange,
    toggleAngleType,
    setMinAngleRange,
    resetFilters,
    toFilterParams,
  } = useFilters();

  // フィルタパラメータ（useMemoで最適化）
  const filterParams = useMemo(() => toFilterParams(), [toFilterParams]);

  // サイドバートグル
  const toggleSidebar = useCallback(() => {
    setIsSidebarOpen(prev => !prev);
  }, []);

  // データ変更ハンドラ（useCallback最適化）
  const handleDataChange = useCallback((data: JunctionFeatureCollection | null) => {
    setTotalCount(data?.total_count ?? 0);
  }, []);

  return (
    <div className="app">
      {/* ヘッダー */}
      <header className="app-header">
        <h1>Y字路マップ</h1>
        <button
          className="mobile-menu-button"
          onClick={toggleSidebar}
          aria-label="メニューを開閉"
          aria-expanded={isSidebarOpen}
          aria-controls="app-sidebar"
        >
          ☰
        </button>
      </header>

      {/* メインコンテンツ */}
      <main className="app-main">
        {/* 左サイドバー */}
        <aside id="app-sidebar" className={`app-sidebar ${isSidebarOpen ? 'sidebar-open' : ''}`}>
          {/* 統計表示 */}
          <StatsDisplay count={totalCount} isLoading={isLoading} />

          {/* フィルターパネル */}
          <div style={{ flex: 1, overflow: 'auto' }}>
            <FilterPanel
              angleTypes={angleTypes}
              minAngleRange={minAngleRange}
              onToggleAngleType={toggleAngleType}
              onMinAngleRangeChange={setMinAngleRange}
              onReset={resetFilters}
            />
          </div>
        </aside>

        {/* 右側の地図 */}
        <div className="app-map-container">
          <MapView
            useMockData={true}
            filters={filterParams}
            onLoadingChange={setIsLoading}
            onDataChange={handleDataChange}
          />
        </div>
      </main>
    </div>
  );
}

export default App;
