import { describe, it, expect } from 'vitest';
import { CITIES } from './cities';

describe('CITIES', () => {
  it('12 都市を含む', () => {
    expect(CITIES).toHaveLength(12);
  });

  it('各エントリの座標が有効範囲内', () => {
    for (const c of CITIES) {
      expect(c.lat).toBeGreaterThanOrEqual(-90);
      expect(c.lat).toBeLessThanOrEqual(90);
      expect(c.lon).toBeGreaterThanOrEqual(-180);
      expect(c.lon).toBeLessThanOrEqual(180);
    }
  });

  it('name と country が非空', () => {
    for (const c of CITIES) {
      expect(c.name.length).toBeGreaterThan(0);
      expect(c.country.length).toBeGreaterThan(0);
    }
  });

  it('都市名が重複しない', () => {
    const names = CITIES.map(c => c.name);
    expect(new Set(names).size).toBe(names.length);
  });
});
