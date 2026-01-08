-- Add highway_type columns
ALTER TABLE y_junctions
ADD COLUMN way_1_highway_type VARCHAR(50),
ADD COLUMN way_2_highway_type VARCHAR(50),
ADD COLUMN way_3_highway_type VARCHAR(50);

-- Add category columns (generated from highway_type)
ALTER TABLE y_junctions
ADD COLUMN way_1_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_1_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_1_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_1_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_1_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED,
ADD COLUMN way_2_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_2_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_2_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_2_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_2_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED,
ADD COLUMN way_3_category VARCHAR(20) GENERATED ALWAYS AS (
    CASE
        WHEN way_3_highway_type IN ('motorway', 'trunk', 'motorway_link', 'trunk_link') THEN 'highway'
        WHEN way_3_highway_type IN ('primary', 'secondary', 'tertiary', 'primary_link', 'secondary_link', 'tertiary_link') THEN 'major'
        WHEN way_3_highway_type IN ('residential', 'unclassified', 'service') THEN 'local'
        WHEN way_3_highway_type IN ('steps', 'pedestrian', 'path') THEN 'pedestrian'
        ELSE NULL
    END
) STORED;

-- Create indexes for efficient filtering
CREATE INDEX idx_y_junctions_highway_types ON y_junctions (way_1_highway_type, way_2_highway_type, way_3_highway_type);
CREATE INDEX idx_y_junctions_categories ON y_junctions (way_1_category, way_2_category, way_3_category);
