import { useCallback, useState } from 'react';
import { Link2, Check } from 'lucide-react';
import type { JunctionProperties } from '../types';

interface JunctionPopupProps {
  properties: JunctionProperties;
}

const ANGLE_TYPE_LABELS: Record<string, string> = {
  verysharp: '超鋭角',
  sharp: '鋭角',
  normal: '広角',
};

export function JunctionPopup({ properties }: JunctionPopupProps) {
  const { osm_node_id, angles, angle_type, streetview_url, min_angle_elevation_diff } = properties;
  const [copied, setCopied] = useState(false);

  const handleCopyUrl = useCallback(() => {
    const url = `${window.location.origin}/node/${osm_node_id}`;
    navigator.clipboard.writeText(url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [osm_node_id]);

  return (
    <div style={{ minWidth: 200 }}>
      {/* 角度情報 */}
      <div style={{ marginBottom: 12 }}>
        <h4 style={{ margin: 0, marginBottom: 8, fontSize: 14, fontWeight: 600 }}>角度情報</h4>
        <div style={{ fontSize: 13 }}>
          <div style={{ marginBottom: 4 }}>
            <strong>タイプ:</strong> {ANGLE_TYPE_LABELS[angle_type]}
          </div>
          <div style={{ marginBottom: 4 }}>
            <strong>角度:</strong> {angles[0]}°, {angles[1]}°, {angles[2]}°
          </div>
          {min_angle_elevation_diff != null && (
            <div>
              <strong>標高差:</strong> {min_angle_elevation_diff.toFixed(1)}m
            </div>
          )}
        </div>
      </div>

      {/* Street Viewリンク + URLコピーボタン */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
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
        <button
          type="button"
          onClick={handleCopyUrl}
          title={copied ? 'コピーしました!' : 'URLをコピー'}
          aria-label={copied ? 'コピーしました' : 'URLをコピー'}
          style={{
            display: 'inline-flex',
            alignItems: 'center',
            justifyContent: 'center',
            width: 32,
            height: 32,
            padding: 0,
            color: copied ? '#22c55e' : '#6b7280',
            background: 'transparent',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          {copied ? <Check size={16} /> : <Link2 size={16} />}
        </button>
      </div>
    </div>
  );
}
