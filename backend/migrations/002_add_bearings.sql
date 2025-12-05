-- Add bearings column to y_junctions table
-- bearings: 各道路の方位角（北を0度として時計回りに0-360度）
-- 3本の道路の方位角を配列で保存（angle_1, angle_2, angle_3と対応順序）
ALTER TABLE y_junctions
ADD COLUMN bearings REAL[3] NOT NULL;

-- Update angle constraints to allow any angle to be > 180°
-- 角度は時計回り順で保存するため、どの角度も360度まで可能
ALTER TABLE y_junctions
  DROP CONSTRAINT IF EXISTS y_junctions_angle_1_check,
  DROP CONSTRAINT IF EXISTS y_junctions_angle_2_check,
  DROP CONSTRAINT IF EXISTS y_junctions_angle_3_check;

ALTER TABLE y_junctions
  ADD CONSTRAINT y_junctions_angle_1_check CHECK (angle_1 >= 0 AND angle_1 <= 360),
  ADD CONSTRAINT y_junctions_angle_2_check CHECK (angle_2 >= 0 AND angle_2 <= 360),
  ADD CONSTRAINT y_junctions_angle_3_check CHECK (angle_3 >= 0 AND angle_3 <= 360);
