-- Extract Baidu panorama metadata to a separate table keyed by osm_node_id.
-- Why: y_junctions.id is auto-incremented, so DELETE + re-import (e.g. when
-- tweaking import filters) churns ids and orphans the panoid cache. Keying on
-- osm_node_id (stable across re-imports) means the cache survives.
--
-- This migration is the EXPAND half of an expand-and-contract pair. It only
-- creates the new table; existing rows in y_junctions.baidu_* must be moved
-- via backend/scripts/backfill_baidu_panoramas.sql (run once after this
-- migration). A follow-up migration drops the legacy columns from
-- y_junctions after the new code path is verified in prod.
CREATE TABLE baidu_panoramas (
  osm_node_id BIGINT PRIMARY KEY,
  panoid VARCHAR NULL,
  pano_mc_x DOUBLE PRECISION NULL,
  pano_mc_y DOUBLE PRECISION NULL,
  queried_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
