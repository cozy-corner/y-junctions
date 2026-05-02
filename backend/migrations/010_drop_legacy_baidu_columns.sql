-- CONTRACT half of the expand-and-contract pair started in 009. Now that
-- application code reads/writes baidu_panoramas exclusively (PR #227) and the
-- prod backfill has populated the new table, the legacy columns on y_junctions
-- are dead weight and can be removed.
ALTER TABLE y_junctions
  DROP COLUMN baidu_panoid,
  DROP COLUMN baidu_pano_mc_x,
  DROP COLUMN baidu_pano_mc_y,
  DROP COLUMN baidu_queried_at;
