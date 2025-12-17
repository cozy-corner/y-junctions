-- Add elevation data columns to y_junctions table
-- 標高データ: GSI DEM5A（5メートルメッシュ標高データ）由来

ALTER TABLE y_junctions
ADD COLUMN elevation REAL,
ADD COLUMN neighbor_elevation_1 REAL,
ADD COLUMN neighbor_elevation_2 REAL,
ADD COLUMN neighbor_elevation_3 REAL,
ADD COLUMN elevation_diff_1 REAL CHECK (elevation_diff_1 >= 0),
ADD COLUMN elevation_diff_2 REAL CHECK (elevation_diff_2 >= 0),
ADD COLUMN elevation_diff_3 REAL CHECK (elevation_diff_3 >= 0),
ADD COLUMN min_angle_index SMALLINT CHECK (min_angle_index BETWEEN 1 AND 3),
ADD COLUMN min_elevation_diff REAL CHECK (min_elevation_diff >= 0),
ADD COLUMN max_elevation_diff REAL CHECK (max_elevation_diff >= 0),
ADD COLUMN min_angle_elevation_diff REAL GENERATED ALWAYS AS (
    CASE min_angle_index
        WHEN 1 THEN ABS(neighbor_elevation_1 - neighbor_elevation_2)
        WHEN 2 THEN ABS(neighbor_elevation_2 - neighbor_elevation_3)
        WHEN 3 THEN ABS(neighbor_elevation_3 - neighbor_elevation_1)
    END
) STORED;

CREATE INDEX idx_y_junctions_elevation
    ON y_junctions (elevation)
    WHERE elevation IS NOT NULL;

CREATE INDEX idx_y_junctions_min_elevation_diff
    ON y_junctions (min_elevation_diff)
    WHERE min_elevation_diff IS NOT NULL;

CREATE INDEX idx_y_junctions_min_angle_elevation_diff
    ON y_junctions (min_angle_elevation_diff)
    WHERE min_angle_elevation_diff IS NOT NULL;

COMMENT ON COLUMN y_junctions.elevation IS 'ジャンクションノードの標高（メートル、GSI DEM5Aデータ由来）';
COMMENT ON COLUMN y_junctions.neighbor_elevation_1 IS '隣接ノード1の標高（メートル）';
COMMENT ON COLUMN y_junctions.neighbor_elevation_2 IS '隣接ノード2の標高（メートル）';
COMMENT ON COLUMN y_junctions.neighbor_elevation_3 IS '隣接ノード3の標高（メートル）';
COMMENT ON COLUMN y_junctions.elevation_diff_1 IS 'ジャンクションノードと隣接ノード1の標高差（メートル）';
COMMENT ON COLUMN y_junctions.elevation_diff_2 IS 'ジャンクションノードと隣接ノード2の標高差（メートル）';
COMMENT ON COLUMN y_junctions.elevation_diff_3 IS 'ジャンクションノードと隣接ノード3の標高差（メートル）';
COMMENT ON COLUMN y_junctions.min_angle_index IS '最小角のインデックス（1=angle_1, 2=angle_2, 3=angle_3）';
COMMENT ON COLUMN y_junctions.min_elevation_diff IS '最小高低差（メートル）';
COMMENT ON COLUMN y_junctions.max_elevation_diff IS '最大高低差（メートル）';
COMMENT ON COLUMN y_junctions.min_angle_elevation_diff IS '最小角を構成する2本の道路間の標高差（メートル）';
