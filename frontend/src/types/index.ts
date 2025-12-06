// AngleType
export type AngleType = 'verysharp' | 'sharp' | 'normal';

// Junction (単体取得時のレスポンス)
export interface Junction {
  id: number;
  osm_node_id: number;
  location: {
    lat: number;
    lon: number;
  };
  angles: [number, number, number];
  angle_type: AngleType;
  streetview_url: string;
  bearings: number[];
}

// GeoJSON型定義
export interface GeoJSONPoint {
  type: 'Point';
  coordinates: [number, number]; // [lon, lat]
}

export interface JunctionProperties {
  id: number;
  osm_node_id: number;
  angles: [number, number, number];
  angle_type: AngleType;
  streetview_url: string;
  bearings: number[];
}

export interface JunctionFeature {
  type: 'Feature';
  geometry: GeoJSONPoint;
  properties: JunctionProperties;
}

export interface JunctionFeatureCollection {
  type: 'FeatureCollection';
  features: JunctionFeature[];
  total_count: number;
}

// 統計情報
export interface Stats {
  total_count: number;
  by_type: {
    verysharp: number;
    sharp: number;
    normal: number;
  };
}

// フィルタパラメータ
export interface FilterParams {
  bbox?: string; // "min_lon,min_lat,max_lon,max_lat"
  angle_type?: AngleType[];
  min_angle_lt?: number;
  min_angle_gt?: number;
  limit?: number;
}

// 地図のbounds
export interface LatLngBounds {
  north: number;
  south: number;
  east: number;
  west: number;
}

// アプリケーション状態
export interface AppState {
  // 地図状態
  bounds: LatLngBounds | null;
  zoom: number;

  // フィルタ条件
  filters: {
    angleTypes: AngleType[];
    minAngleLt: number | null;
    minAngleGt: number | null;
  };

  // データ
  junctions: Junction[];
  isLoading: boolean;
  totalCount: number;
}
