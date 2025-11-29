import { useState, useCallback, useMemo } from 'react';
import { MapContainer, TileLayer, Marker, useMapEvents } from 'react-leaflet';
import { Icon } from 'leaflet';
import type { LatLngBounds, AngleType } from '../types';
import { useJunctions } from '../hooks/useJunctions';

// 初期位置: 東京駅
const INITIAL_CENTER: [number, number] = [35.6812, 139.7671];
const INITIAL_ZOOM = 14;

// angle_typeごとのマーカー色
const MARKER_COLORS: Record<AngleType, string> = {
  sharp: '#ff4444', // 赤
  even: '#44ff44', // 緑
  skewed: '#4444ff', // 青
  normal: '#ffaa00', // オレンジ
};

// カスタムマーカーアイコンを作成
function createMarkerIcon(angleType: AngleType): Icon {
  const color = MARKER_COLORS[angleType];

  // SVGマーカーを作成
  const svg = `
    <svg width="25" height="41" viewBox="0 0 25 41" xmlns="http://www.w3.org/2000/svg">
      <path d="M12.5 0C5.6 0 0 5.6 0 12.5c0 8.4 12.5 28.5 12.5 28.5S25 20.9 25 12.5C25 5.6 19.4 0 12.5 0z"
            fill="${color}" stroke="#fff" stroke-width="1.5"/>
      <circle cx="12.5" cy="12.5" r="6" fill="#fff"/>
    </svg>
  `;

  return new Icon({
    iconUrl: `data:image/svg+xml;base64,${btoa(svg)}`,
    iconSize: [25, 41],
    iconAnchor: [12, 41],
  });
}

// 地図のイベントハンドリング用コンポーネント
interface MapEventsHandlerProps {
  onBoundsChange: (bounds: LatLngBounds) => void;
}

function MapEventsHandler({ onBoundsChange }: MapEventsHandlerProps) {
  const map = useMapEvents({
    moveend: () => {
      const bounds = map.getBounds();
      onBoundsChange({
        north: bounds.getNorth(),
        south: bounds.getSouth(),
        east: bounds.getEast(),
        west: bounds.getWest(),
      });
    },
    zoomend: () => {
      const bounds = map.getBounds();
      onBoundsChange({
        north: bounds.getNorth(),
        south: bounds.getSouth(),
        east: bounds.getEast(),
        west: bounds.getWest(),
      });
    },
  });

  return null;
}

interface MapViewProps {
  useMockData?: boolean;
}

export function MapView({ useMockData = true }: MapViewProps) {
  const [bounds, setBounds] = useState<LatLngBounds | null>(null);

  // Y字路データを取得
  const { data } = useJunctions({
    bounds,
    useMockData,
  });

  // boundsの変更ハンドラ
  const handleBoundsChange = useCallback((newBounds: LatLngBounds) => {
    setBounds(newBounds);
  }, []);

  // マーカーアイコンをメモ化
  const markerIcons = useMemo(
    () => ({
      sharp: createMarkerIcon('sharp'),
      even: createMarkerIcon('even'),
      skewed: createMarkerIcon('skewed'),
      normal: createMarkerIcon('normal'),
    }),
    []
  );

  return (
    <div style={{ height: '100%', width: '100%' }}>
      <MapContainer
        center={INITIAL_CENTER}
        zoom={INITIAL_ZOOM}
        style={{ height: '100%', width: '100%' }}
      >
        {/* OpenStreetMap タイル */}
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />

        {/* イベントハンドラ */}
        <MapEventsHandler onBoundsChange={handleBoundsChange} />

        {/* マーカー表示 */}
        {data?.features.map(feature => {
          const [lon, lat] = feature.geometry.coordinates;
          const { id, angle_type } = feature.properties;

          return <Marker key={id} position={[lat, lon]} icon={markerIcons[angle_type]} />;
        })}
      </MapContainer>
    </div>
  );
}
