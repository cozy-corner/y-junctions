-- Enable PostGIS extension
CREATE EXTENSION IF NOT EXISTS postgis;

-- Create y_junctions table
CREATE TABLE y_junctions (
    id BIGSERIAL PRIMARY KEY,
    osm_node_id BIGINT UNIQUE NOT NULL,
    location GEOGRAPHY(POINT, 4326) NOT NULL,

    -- 角度（度数法、小さい順にソート済み）
    angle_1 SMALLINT NOT NULL CHECK (angle_1 BETWEEN 0 AND 180),
    angle_2 SMALLINT NOT NULL CHECK (angle_2 BETWEEN 0 AND 180),
    angle_3 SMALLINT NOT NULL CHECK (angle_3 BETWEEN 0 AND 360),

    -- メタデータ
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- インデックス
CREATE INDEX idx_y_junctions_location ON y_junctions USING GIST (location);
CREATE INDEX idx_y_junctions_angle_1 ON y_junctions (angle_1);
