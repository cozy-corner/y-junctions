export interface City {
  name: string; // 表示名（例: "東京"）
  country: string; // optgroup ラベル（例: "日本"）
  lat: number;
  lon: number;
}

export const CITIES: City[] = [
  { name: '東京', country: '日本', lat: 35.681, lon: 139.767 },
  { name: '名古屋', country: '日本', lat: 35.17, lon: 136.906 },
  { name: '大阪', country: '日本', lat: 34.694, lon: 135.5 },
  { name: '広島', country: '日本', lat: 34.39, lon: 132.46 },
  { name: '福岡', country: '日本', lat: 33.59, lon: 130.401 },
  { name: '仙台', country: '日本', lat: 38.268, lon: 140.872 },
  { name: '札幌', country: '日本', lat: 43.062, lon: 141.347 },
  { name: 'ソウル', country: '韓国', lat: 37.567, lon: 126.978 },
  { name: '釜山', country: '韓国', lat: 35.18, lon: 129.076 },
  { name: '台北', country: '台湾', lat: 25.033, lon: 121.565 },
  { name: '高雄', country: '台湾', lat: 22.627, lon: 120.301 },
  { name: '香港', country: '香港', lat: 22.32, lon: 114.17 },
];
