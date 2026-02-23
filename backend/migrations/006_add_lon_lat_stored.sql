-- Add STORED lat/lon columns to avoid ST_Y/ST_X per-row computation
-- and enable efficient B-tree range scan instead of spatial index full scan
ALTER TABLE y_junctions
  ADD COLUMN lat FLOAT8 GENERATED ALWAYS AS (ST_Y(location::geometry)) STORED,
  ADD COLUMN lon FLOAT8 GENERATED ALWAYS AS (ST_X(location::geometry)) STORED;

-- B-tree index on (lon, lat) for efficient bbox range queries
-- Replaces ST_Intersects + spatial inverted index with simple BETWEEN scan
CREATE INDEX idx_y_junctions_lon_lat ON y_junctions (lon, lat);
