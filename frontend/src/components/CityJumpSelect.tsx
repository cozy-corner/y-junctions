import { memo, useMemo, type ChangeEvent } from 'react';
import { CITIES, type City } from '../constants/cities';

export interface CityJumpSelectProps {
  onSelect: (city: City) => void;
}

export const CityJumpSelect = memo(function CityJumpSelect({ onSelect }: CityJumpSelectProps) {
  // country の出現順を保ったグルーピング
  const grouped = useMemo(() => {
    const map = new Map<string, City[]>();
    for (const city of CITIES) {
      const list = map.get(city.country) ?? [];
      list.push(city);
      map.set(city.country, list);
    }
    return [...map.entries()];
  }, []);

  const handleChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const name = e.target.value;
    if (!name) return;
    const city = CITIES.find(c => c.name === name);
    if (city) onSelect(city);
  };

  return (
    <div className="city-jump">
      <select
        className="city-jump-select"
        value=""
        onChange={handleChange}
        aria-label="主要都市へジャンプ"
      >
        <option value="">都市を選択…</option>
        {grouped.map(([country, cities]) => (
          <optgroup key={country} label={country}>
            {cities.map(city => (
              <option key={city.name} value={city.name}>
                {city.name}
              </option>
            ))}
          </optgroup>
        ))}
      </select>
    </div>
  );
});
