import type { JunctionProperties } from '../types';

interface JunctionPopupProps {
  properties: JunctionProperties;
}

const ANGLE_TYPE_LABELS: Record<string, string> = {
  sharp: '鋭角',
  even: '三叉路',
  skewed: '直線分岐',
  normal: '中間',
};

export function JunctionPopup({ properties }: JunctionPopupProps) {
  const { angles, angle_type, streetview_url } = properties;

  return (
    <div style={{ minWidth: 200 }}>
      {/* 角度情報 */}
      <div style={{ marginBottom: 12 }}>
        <h4 style={{ margin: 0, marginBottom: 8, fontSize: 14, fontWeight: 600 }}>角度情報</h4>
        <div style={{ fontSize: 13 }}>
          <div style={{ marginBottom: 4 }}>
            <strong>タイプ:</strong> {ANGLE_TYPE_LABELS[angle_type]}
          </div>
          <div>
            <strong>角度:</strong> {angles[0]}°, {angles[1]}°, {angles[2]}°
          </div>
        </div>
      </div>

      {/* Street Viewリンク */}
      <div>
        <a
          href={streetview_url}
          target="_blank"
          rel="noopener noreferrer"
          style={{
            display: 'inline-block',
            padding: '8px 12px',
            fontSize: 13,
            fontWeight: 600,
            color: 'white',
            background: '#4285f4',
            textDecoration: 'none',
            borderRadius: 4,
          }}
        >
          Street Viewで見る
        </a>
      </div>
    </div>
  );
}
