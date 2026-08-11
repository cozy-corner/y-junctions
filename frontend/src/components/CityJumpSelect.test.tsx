import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { CityJumpSelect } from './CityJumpSelect';

describe('CityJumpSelect', () => {
  it('都市を選ぶと対応する City で onSelect が呼ばれる', () => {
    const onSelect = vi.fn();
    render(<CityJumpSelect onSelect={onSelect} />);
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: '東京' } });
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ name: '東京', lat: 35.681, lon: 139.767 })
    );
  });

  it('プレースホルダ選択では onSelect が呼ばれない', () => {
    const onSelect = vi.fn();
    render(<CityJumpSelect onSelect={onSelect} />);
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: '' } });
    expect(onSelect).not.toHaveBeenCalled();
  });

  it('国ごとの optgroup を持つ', () => {
    render(<CityJumpSelect onSelect={vi.fn()} />);
    // 日本・韓国・台湾・香港・シンガポール の 5 グループ
    const groups = document.querySelectorAll('optgroup');
    expect(groups).toHaveLength(5);
  });

  it('同じ都市を連続選択すると毎回 onSelect が呼ばれる', () => {
    const onSelect = vi.fn();
    render(<CityJumpSelect onSelect={onSelect} />);
    const select = screen.getByRole('combobox');
    fireEvent.change(select, { target: { value: '東京' } });
    // controlled で value="" に戻ることが「同一都市の再選択」を可能にする本質。
    // これを検証しないと、リセットが壊れても 2 回発火だけは通ってしまう。
    expect(select).toHaveValue('');
    fireEvent.change(select, { target: { value: '東京' } });
    expect(onSelect).toHaveBeenCalledTimes(2);
  });
});
