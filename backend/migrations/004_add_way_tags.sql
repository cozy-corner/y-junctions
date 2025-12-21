-- Add bridge and tunnel tags for connected ways
-- Migration 004: Add way tag information to identify bridges and tunnels

ALTER TABLE y_junctions
ADD COLUMN way_1_bridge BOOLEAN DEFAULT FALSE,
ADD COLUMN way_1_tunnel BOOLEAN DEFAULT FALSE,
ADD COLUMN way_2_bridge BOOLEAN DEFAULT FALSE,
ADD COLUMN way_2_tunnel BOOLEAN DEFAULT FALSE,
ADD COLUMN way_3_bridge BOOLEAN DEFAULT FALSE,
ADD COLUMN way_3_tunnel BOOLEAN DEFAULT FALSE;

-- Add index for filtering by bridge/tunnel
CREATE INDEX idx_y_junctions_way_tags ON y_junctions (
    way_1_bridge, way_1_tunnel,
    way_2_bridge, way_2_tunnel,
    way_3_bridge, way_3_tunnel
);
