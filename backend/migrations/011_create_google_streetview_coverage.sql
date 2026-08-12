-- Google Street View coverage cache. See doc/streetview-coverage-filter.md.
--
-- Three states: true = panorama exists, false = none (excluded from the map),
-- no row at all = not queried yet (still shown).
-- Keyed on osm_node_id, not y_junctions.id: ids churn on re-import and would
-- orphan the cache (same reason as baidu_panoramas in migration 009).
CREATE TABLE google_streetview_coverage (
  osm_node_id BIGINT PRIMARY KEY,
  has_coverage BOOLEAN NOT NULL,
  queried_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
