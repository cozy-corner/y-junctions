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

  // URLパスから osm_node_id を取得（例: /node/123456）
  const [selectedOsmNodeId, setSelectedOsmNodeId] = useState<string | undefined>(() => {
    const match = window.location.pathname.match(/^\/node\/(\d+)$/);
    return match?.[1];
  });

  // フィルタ状態管理
  const {
    angleTypes,
    minAngleRange,
    elevationDiffRange,
    categories,
    toggleAngleType,
    setMinAngleRange,
    setElevationDiffRange,
    toggleCategory,
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
              elevationDiffRange={elevationDiffRange}
              categories={categories}
              onToggleAngleType={toggleAngleType}
              onMinAngleRangeChange={setMinAngleRange}
              onElevationDiffRangeChange={setElevationDiffRange}
              onToggleCategory={toggleCategory}
              onReset={resetFilters}
            />
          </div>
        </aside>

        {/* 右側の地図 */}
        <div className="app-map-container">
          <MapView
            useMockData={false}
            filters={filterParams}
            onLoadingChange={setIsLoading}
            onDataChange={handleDataChange}
            selectedOsmNodeId={selectedOsmNodeId}
            onSelectOsmNodeId={setSelectedOsmNodeId}
          />
        </div>
      </main>

      {/* フッター */}
      <footer className="app-footer">
        <p>
          © 2025 Y字路マップ | Created by{' '}
          <a href="https://x.com/coozy_corner" target="_blank" rel="noopener noreferrer">
            @coozy_corner
          </a>
          {' | '}
          <a
            href="https://github.com/cozy-corner/y-junctions/releases"
            target="_blank"
            rel="noopener noreferrer"
          >
            リリースノート
          </a>
        </p>
      </footer>
    </div>
  );
}

export default App;
