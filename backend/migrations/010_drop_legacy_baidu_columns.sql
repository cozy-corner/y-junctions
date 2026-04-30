-- Drop the legacy Baidu panorama columns from y_junctions. By this point all
-- application code reads/writes baidu_panoramas (added in 009 and switched in
-- the (2/3) PR), and the prod data has been backfilled to the new table, so
-- these columns are pure orphans.
--
-- This is the CONTRACT half of the expand-and-contract pair started by
-- migration 009.
ALTER TABLE y_junctions
  DROP COLUMN baidu_panoid,
  DROP COLUMN baidu_pano_mc_x,
  DROP COLUMN baidu_pano_mc_y,
  DROP COLUMN baidu_queried_at;
