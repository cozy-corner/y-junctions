import { useState, useCallback, useMemo, useEffect, memo } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMapEvents } from 'react-leaflet';
import { Icon } from 'leaflet';
import type { LatLngBounds, AngleType, FilterParams, JunctionFeatureCollection } from '../types';
import { useJunctions } from '../hooks/useJunctions';
import { JunctionPopup } from './JunctionPopup';

// 初期位置: 東京駅
const INITIAL_CENTER: [number, number] = [35.6812, 139.7671];
const INITIAL_ZOOM = 14;

// angle_typeごとのマーカー色（Y字路書籍をイメージした紫、黄色、暗い青のパレット）
const MARKER_COLORS: Record<AngleType, string> = {
  verysharp: '#1E3A8A', // 暗い青（濃紺） - 最小角度が最も小さい
  sharp: '#3B82F6', // 明るい青
  normal: '#F59E0B', // 濃い黄色（琥珀色） - 通常
  skewed: '#7C3AED', // 紫 - 直線分岐（特殊）
};

// カスタムマーカーアイコンを作成（メモ化用に外部で定義）
const markerIconCache: Partial<Record<AngleType, Icon>> = {};

function getMarkerIcon(angleType: AngleType): Icon {
  if (markerIconCache[angleType]) {
    return markerIconCache[angleType]!;
  }

  const color = MARKER_COLORS[angleType];
  const svg = `
    <svg width="25" height="41" viewBox="0 0 25 41" xmlns="http://www.w3.org/2000/svg">
      <path d="M12.5 0C5.6 0 0 5.6 0 12.5c0 8.4 12.5 28.5 12.5 28.5S25 20.9 25 12.5C25 5.6 19.4 0 12.5 0z"
            fill="${color}" stroke="#fff" stroke-width="1.5"/>
      <circle cx="12.5" cy="12.5" r="6" fill="#fff"/>
    </svg>
  `;

  const icon = new Icon({
    iconUrl: `data:image/svg+xml;base64,${btoa(svg)}`,
    iconSize: [25, 41],
    iconAnchor: [12, 41],
  });

  markerIconCache[angleType] = icon;
  return icon;
}

// 地図のイベントハンドリング用コンポーネント（メモ化）
interface MapEventsHandlerProps {
  onBoundsChange: (bounds: LatLngBounds) => void;
}

const MapEventsHandler = memo(function MapEventsHandler({ onBoundsChange }: MapEventsHandlerProps) {
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
});

interface MapViewProps {
  useMockData?: boolean;
  filters?: Omit<FilterParams, 'bbox'>;
  onLoadingChange?: (isLoading: boolean) => void;
  onDataChange?: (data: JunctionFeatureCollection | null) => void;
}

export const MapView = memo(function MapView({
  useMockData = true,
  filters,
  onLoadingChange,
  onDataChange,
}: MapViewProps) {
  const [bounds, setBounds] = useState<LatLngBounds | null>(null);

  // Y字路データを取得
  const { data, isLoading } = useJunctions({
    bounds,
    filters,
    useMockData,
  });

  // ローディング状態の変化を通知（useCallbackで最適化）
  useEffect(() => {
    onLoadingChange?.(isLoading);
  }, [isLoading, onLoadingChange]);

  // データの変化を通知（useCallbackで最適化）
  useEffect(() => {
    onDataChange?.(data);
  }, [data, onDataChange]);

  // boundsの変更ハンドラ（useCallbackで最適化）
  const handleBoundsChange = useCallback((newBounds: LatLngBounds) => {
    setBounds(newBounds);
  }, []);

  // マーカーリストをメモ化（データが変わった時のみ再計算）
  const markers = useMemo(() => {
    return data?.features.map(feature => {
      const [lon, lat] = feature.geometry.coordinates;
      const { id, angle_type } = feature.properties;

      return (
        <Marker key={id} position={[lat, lon]} icon={getMarkerIcon(angle_type)}>
          <Popup>
            <JunctionPopup properties={feature.properties} />
          </Popup>
        </Marker>
      );
    });
  }, [data]);

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
        {markers}
      </MapContainer>
    </div>
  );
});
