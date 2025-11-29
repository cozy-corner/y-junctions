import { MapView } from './components/MapView';

function App() {
  return (
    <div style={{ height: '100vh', display: 'flex', flexDirection: 'column', fontFamily: 'sans-serif' }}>
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
      <main style={{ flex: 1, overflow: 'hidden' }}>
        <MapView useMockData={true} />
      </main>
    </div>
  );
}

export default App;
