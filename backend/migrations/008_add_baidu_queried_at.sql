-- Track when we last asked Baidu about a junction so re-runs can skip
-- coordinates that already returned "no coverage". Three-state semantics:
--   baidu_panoid IS NOT NULL                             → pano found
--   baidu_panoid IS NULL AND baidu_queried_at IS NOT NULL → queried, no coverage
--   both NULL                                            → never queried
ALTER TABLE y_junctions
  ADD COLUMN baidu_queried_at TIMESTAMPTZ NULL;
