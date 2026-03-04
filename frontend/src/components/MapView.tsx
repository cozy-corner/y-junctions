import { useState, useCallback, useMemo, useEffect, useRef, memo, type ReactNode } from 'react';
import { MapContainer, TileLayer, Marker, Popup, useMapEvents, useMap } from 'react-leaflet';
import { Icon } from 'leaflet';
import type { Marker as LeafletMarker } from 'leaflet';
import type { LatLngBounds, AngleType, FilterParams, JunctionFeatureCollection } from '../types';
import { useJunctions } from '../hooks/useJunctions';
import { fetchJunctionByOsmNodeId } from '../api/client';
import { JunctionPopup } from './JunctionPopup';

// 初期位置: 東京駅
const DEFAULT_CENTER: [number, number] = [35.6812, 139.7671];
const INITIAL_ZOOM = 14;

// URLアクセス時のフォーカスアニメーション設定
const FOCUS_START_ZOOM = 5;
const FOCUS_TARGET_ZOOM = 16;

// angle_typeごとのマーカー色（Y字路書籍をイメージした紫、青、黄色のパレット）
const MARKER_COLORS: Record<AngleType, string> = {
  verysharp: '#8B5CF6', // 紫（violet-500） - 最小角度が最も小さい
  sharp: '#3B82F6', // 明るい青
  normal: '#F59E0B', // 濃い黄色（琥珀色） - 通常
};

// Y字路形状のSVGマーカーアイコンを生成する
function getMarkerIcon(angleType: AngleType, bearings: number[]): Icon {
  const color = MARKER_COLORS[angleType];
  const size = 38;
  const center = size / 2;
  const bgRadius = center - 1;
  const lineLen = bgRadius - 5;

  const lines = bearings
    .map(bearing => {
      const rad = (bearing * Math.PI) / 180;
      const x = (center + lineLen * Math.sin(rad)).toFixed(1);
      const y = (center - lineLen * Math.cos(rad)).toFixed(1);
      return `<line x1="${center}" y1="${center}" x2="${x}" y2="${y}" stroke="white" stroke-width="3" stroke-linecap="round"/>`;
    })
    .join('');

  const svg = `<svg width="${size}" height="${size}" viewBox="0 0 ${size} ${size}" xmlns="http://www.w3.org/2000/svg">
    <circle cx="${center}" cy="${center}" r="${bgRadius}" fill="${color}" stroke="white" stroke-width="2"/>
    ${lines}
    <circle cx="${center}" cy="${center}" r="3" fill="white"/>
  </svg>`;

  return new Icon({
    iconUrl: `data:image/svg+xml;base64,${btoa(svg)}`,
    iconSize: [size, size],
    iconAnchor: [center, center],
  });
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
const InitialBounds = memo(function InitialBounds({
  onBoundsChange,
}: {
  onBoundsChange: (bounds: LatLngBounds) => void;
}) {
  const map = useMap();

  useEffect(() => {
    const bounds = map.getBounds();
    onBoundsChange({
      north: bounds.getNorth(),
      south: bounds.getSouth(),
      east: bounds.getEast(),
      west: bounds.getWest(),
    });
  }, [map, onBoundsChange]);

  return null;
});

// URLアクセス時に上空からズームインするアニメーションコンポーネント
interface FocusAnimationProps {
  target: [number, number];
}

const FocusAnimation = memo(function FocusAnimation({ target }: FocusAnimationProps) {
  const map = useMap();

  useEffect(() => {
    map.flyTo(target, FOCUS_TARGET_ZOOM, {
      animate: true,
      duration: 5,
    });
  }, [map, target]);

  return null;
});

// isSelected=true のときポップアップを自動で開くマーカー
interface JunctionMarkerProps {
  position: [number, number];
  icon: Icon;
  isSelected: boolean;
  children: ReactNode;
}

const JunctionMarker = memo(function JunctionMarker({
  position,
  icon,
  isSelected,
  children,
}: JunctionMarkerProps) {
  const markerRef = useRef<LeafletMarker>(null);

  useEffect(() => {
    if (isSelected && markerRef.current) {
      markerRef.current.openPopup();
    }
  }, [isSelected]);

  return (
    <Marker ref={markerRef} position={position} icon={icon}>
      <Popup>{children}</Popup>
    </Marker>
  );
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
  // URLから osm_node_id を取得（文字列のまま扱う）
  const urlOsmNodeId = useMemo(() => {
    const match = window.location.hash.match(/^#node=(\d+)$/);
    return match?.[1] ?? null;
  }, []);

  // URL指定がない場合は即座にデフォルト中心を設定、ある場合はAPIで取得するまでnull
  const [initialCenter, setInitialCenter] = useState<[number, number] | null>(() =>
    urlOsmNodeId ? null : DEFAULT_CENTER
  );
  const [bounds, setBounds] = useState<LatLngBounds | null>(null);

  // URL指定がある場合、マウント時に座標を取得して initialCenter を設定
  useEffect(() => {
    if (!urlOsmNodeId) return;

    fetchJunctionByOsmNodeId(urlOsmNodeId)
      .then(({ lat, lon }) => {
        setInitialCenter([lat, lon]);
      })
      .catch(() => {
        setInitialCenter(DEFAULT_CENTER);
      });
  }, [urlOsmNodeId]);

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

  // マーカーリストをメモ化
  const markers = useMemo(() => {
    return data?.features.map(feature => {
      const [lon, lat] = feature.geometry.coordinates;
      const { osm_node_id, angle_type, bearings = [] } = feature.properties;
      const isSelected = urlOsmNodeId !== null && String(osm_node_id) === urlOsmNodeId;

      return (
        <JunctionMarker
          key={osm_node_id}
          position={[lat, lon]}
          icon={getMarkerIcon(angle_type, bearings)}
          isSelected={isSelected}
        >
          <JunctionPopup properties={feature.properties} />
        </JunctionMarker>
      );
    });
  }, [data, urlOsmNodeId]);

  // initialCenter が決まるまで描画しない（東京経由のflyToを防ぐ）
  if (initialCenter === null) {
    return null;
  }

  return (
    <div style={{ height: '100%', width: '100%' }}>
      <MapContainer
        center={initialCenter}
        zoom={urlOsmNodeId ? FOCUS_START_ZOOM : INITIAL_ZOOM}
        style={{ height: '100%', width: '100%' }}
      >
        {/* OpenStreetMap タイル */}
        <TileLayer
          attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
          url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
        />

        {/* URLアクセス時のフォーカスアニメーション */}
        {urlOsmNodeId && initialCenter && <FocusAnimation target={initialCenter} />}

        {/* 初期bounds設定 */}
        <InitialBounds onBoundsChange={handleBoundsChange} />

        {/* イベントハンドラ */}
        <MapEventsHandler onBoundsChange={handleBoundsChange} />

        {/* マーカー表示 */}
        {markers}
      </MapContainer>
    </div>
  );
});
