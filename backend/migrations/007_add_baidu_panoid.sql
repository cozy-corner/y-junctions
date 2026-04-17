-- Baidu panorama metadata for Chinese mainland Y-junctions.
-- All three columns are NULLABLE so existing rows and non-China rows stay unaffected.
ALTER TABLE y_junctions
  ADD COLUMN baidu_panoid VARCHAR NULL,
  ADD COLUMN baidu_pano_mc_x DOUBLE PRECISION NULL,
  ADD COLUMN baidu_pano_mc_y DOUBLE PRECISION NULL;
