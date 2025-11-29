import type { Junction, JunctionFeatureCollection, Stats, FilterParams } from '../types';

const BASE_URL = 'http://localhost:8080/api';

// カスタムエラークラス
export class ApiError extends Error {
  constructor(
    message: string,
    public status?: number,
    public response?: unknown
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

// Y字路一覧を取得
export async function fetchJunctions(
  bbox: string,
  filters?: Omit<FilterParams, 'bbox'>
): Promise<JunctionFeatureCollection> {
  try {
    const params = new URLSearchParams({ bbox });

    if (filters?.angle_type) {
      params.append('angle_type', filters.angle_type);
    }
    if (filters?.min_angle_lt !== undefined) {
      params.append('min_angle_lt', filters.min_angle_lt.toString());
    }
    if (filters?.min_angle_gt !== undefined) {
      params.append('min_angle_gt', filters.min_angle_gt.toString());
    }
    if (filters?.limit !== undefined) {
      params.append('limit', filters.limit.toString());
    }

    const url = `${BASE_URL}/junctions?${params.toString()}`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new ApiError(`Failed to fetch junctions: ${response.statusText}`, response.status);
    }

    const data: JunctionFeatureCollection = await response.json();
    return data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw error;
    }
    throw new ApiError(
      `Network error: ${error instanceof Error ? error.message : 'Unknown error'}`
    );
  }
}

// 特定のY字路詳細を取得
export async function fetchJunctionById(id: number): Promise<Junction> {
  try {
    const url = `${BASE_URL}/junctions/${id}`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new ApiError(`Failed to fetch junction: ${response.statusText}`, response.status);
    }

    const data: Junction = await response.json();
    return data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw error;
    }
    throw new ApiError(
      `Network error: ${error instanceof Error ? error.message : 'Unknown error'}`
    );
  }
}

// 統計情報を取得
export async function fetchStats(): Promise<Stats> {
  try {
    const url = `${BASE_URL}/stats`;
    const response = await fetch(url);

    if (!response.ok) {
      throw new ApiError(`Failed to fetch stats: ${response.statusText}`, response.status);
    }

    const data: Stats = await response.json();
    return data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw error;
    }
    throw new ApiError(
      `Network error: ${error instanceof Error ? error.message : 'Unknown error'}`
    );
  }
}
