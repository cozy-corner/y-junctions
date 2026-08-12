-- Google Street View coverage cache, keyed by osm_node_id.
-- Why a separate table instead of a y_junctions column: /deploy-data pushes to
-- prod with IMPORT INTO (append-only, no UPDATE), so coverage for already
-- deployed nodes can only be added as new rows in a table of its own. Keying on
-- osm_node_id (stable across re-imports, unlike the auto-incremented
-- y_junctions.id) keeps the cache from being orphaned.
--
-- Three states: row with has_coverage=true (panorama exists) / row with false
-- (none -> excluded from the map) / no row at all (not queried yet -> shown).
-- Only "queried successfully and confirmed absent" writes false.
--
-- No pano_id or panorama coordinates: Google Street View URLs are generated
-- from the junction coordinates alone, so nothing else would ever be read.
CREATE TABLE google_streetview_coverage (
  osm_node_id BIGINT PRIMARY KEY,
  has_coverage BOOLEAN NOT NULL,
  queried_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
