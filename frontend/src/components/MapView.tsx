import { useState, useCallback, useRef, useEffect, memo } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMapEvents, useMap } from 'react-leaflet';
import { Icon } from 'leaflet';
import type { Marker as LeafletMarker } from 'leaflet';
import type {
  LatLngBounds,
  AngleType,
  FilterParams,
  JunctionFeatureCollection,
  JunctionFeature,
} from '../types';
import { useJunctions } from '../hooks/useJunctions';
import { JunctionPopup } from './JunctionPopup';
import { fetchJunctionByOsmNodeId } from '../api/client';

// 初期位置: 東京駅
const INITIAL_CENTER: [number, number] = [35.6812, 139.7671];
const INITIAL_ZOOM = 14;

// angle_typeごとのマーカー色（Y字路書籍をイメージした紫、青、黄色のパレット）
const MARKER_COLORS: Record<AngleType, string> = {
  verysharp: '#8B5CF6', // 紫（violet-500） - 最小角度が最も小さい
  sharp: '#3B82F6', // 明るい青
  normal: '#F59E0B', // 濃い黄色（琥珀色） - 通常
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

// マウント時に初期boundsを設定するコンポーネント
// （LeafletはuseEffectよりも前にmoveendを発火するため、初期boundsを手動でセットする）
function InitialBounds({ onBoundsChange }: { onBoundsChange: (bounds: LatLngBounds) => void }) {
  const map = useMap();

  useEffect(() => {
    const b = map.getBounds();
    onBoundsChange({
      north: b.getNorth(),
      south: b.getSouth(),
      east: b.getEast(),
      west: b.getWest(),
    });
  }, [map, onBoundsChange]);

  return null;
}

// 個別マーカーコンポーネント
interface JunctionMarkerProps {
  feature: JunctionFeature;
  isSelected: boolean;
  onSelect: (osmNodeId: string) => void;
}

const JunctionMarker = memo(function JunctionMarker({
  feature,
  isSelected,
  onSelect,
}: JunctionMarkerProps) {
  const markerRef = useRef<LeafletMarker | null>(null);
  const [lon, lat] = feature.geometry.coordinates;
  const { osm_node_id, angle_type } = feature.properties;

  useEffect(() => {
    if (isSelected && markerRef.current) {
      markerRef.current.openPopup();
    }
  }, [isSelected]);

  const handleClick = useCallback(() => {
    onSelect(osm_node_id.toString());
  }, [osm_node_id, onSelect]);

  return (
    <Marker
      ref={markerRef}
      position={[lat, lon]}
      icon={getMarkerIcon(angle_type)}
      eventHandlers={{ click: handleClick }}
    >
      <Popup>
        <JunctionPopup properties={feature.properties} />
      </Popup>
    </Marker>
  );
});

interface MapViewProps {
  useMockData?: boolean;
  filters?: Omit<FilterParams, 'bbox'>;
  onLoadingChange?: (isLoading: boolean) => void;
  onDataChange?: (data: JunctionFeatureCollection | null) => void;
  selectedOsmNodeId?: string;
  onSelectOsmNodeId?: (osmNodeId: string) => void;
}

export const MapView = memo(function MapView({
  useMockData = true,
  filters,
  onLoadingChange,
  onDataChange,
  selectedOsmNodeId,
  onSelectOsmNodeId,
}: MapViewProps) {
  const [bounds, setBounds] = useState<LatLngBounds | null>(null);

  // URL直接アクセス時の初期位置（MapContainerはマウント後にcenterを無視するため、事前に解決する）
  const initialOsmNodeId = useRef(selectedOsmNodeId);
  const [initialCenter, setInitialCenter] = useState<[number, number] | null>(
    selectedOsmNodeId ? null : INITIAL_CENTER
  );
  const initialZoom = selectedOsmNodeId !== undefined ? 16 : INITIAL_ZOOM;

  useEffect(() => {
    if (!initialOsmNodeId.current) return;
    fetchJunctionByOsmNodeId(initialOsmNodeId.current)
      .then(({ lat, lon }) => setInitialCenter([lat, lon]))
      .catch(() => setInitialCenter(INITIAL_CENTER));
  }, []);

  // Y字路データを取得
  const { data, isLoading } = useJunctions({
    bounds,
    filters,
    useMockData,
  });

  // ローディング状態の変化を通知
  useEffect(() => {
    onLoadingChange?.(isLoading);
  }, [isLoading, onLoadingChange]);

  // データの変化を通知
  useEffect(() => {
    onDataChange?.(data);
  }, [data, onDataChange]);

  // boundsの変更ハンドラ
  const handleBoundsChange = useCallback((newBounds: LatLngBounds) => {
    setBounds(newBounds);
  }, []);

  if (!initialCenter) return null;

  return (
    <div style={{ height: '100%', width: '100%' }}>
      <MapContainer
        center={initialCenter}
        zoom={initialZoom}
        style={{ height: '100%', width: '100%' }}
      >
        {/* OpenStreetMap タイル */}
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />

        {/* 初期bounds設定 */}
        <InitialBounds onBoundsChange={handleBoundsChange} />

        {/* イベントハンドラ */}
        <MapEventsHandler onBoundsChange={handleBoundsChange} />

        {/* マーカー表示 */}
        {data?.features.map(feature => (
          <JunctionMarker
            key={feature.properties.osm_node_id}
            feature={feature}
            isSelected={feature.properties.osm_node_id.toString() === selectedOsmNodeId}
            onSelect={onSelectOsmNodeId ?? (() => {})}
          />
        ))}
      </MapContainer>
    </div>
  );
});
