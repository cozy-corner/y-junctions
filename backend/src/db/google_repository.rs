use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;

/// A junction awaiting (or due for re-)coverage lookup. Only the fields the
/// enrich batch needs: the id to key the cache on and the coordinate to query
/// and to run the China check against.
#[derive(Debug, FromRow, PartialEq)]
pub struct CoverageCandidate {
    pub osm_node_id: i64,
    pub lon: f64,
    pub lat: f64,
}

/// Look up cached coverage for the given OSM node ids. A missing key means
/// "never queried" — distinct from a present `false`, which means Google
/// confirmed there is no panorama.
pub async fn find_coverage_by_osm_node_ids(
    pool: &PgPool,
    osm_node_ids: &[i64],
) -> Result<HashMap<i64, bool>, sqlx::Error> {
    if osm_node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<(i64, bool)> = sqlx::query_as(
        "SELECT osm_node_id, has_coverage \
         FROM google_streetview_coverage \
         WHERE osm_node_id = ANY($1)",
    )
    .bind(osm_node_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

/// Junctions to query: never-queried ones, plus (when `refresh`) the ones
/// previously recorded as uncovered, since Google's coverage grows over time.
///
/// Returns every region — the mainland-China check lives only in
/// `domain::china::is_in_china_mainland` (hand-written bboxes in Rust) and
/// `y_junctions` has no region column, so the caller filters in-process.
pub async fn find_uncovered_nodes(
    pool: &PgPool,
    refresh: bool,
) -> Result<Vec<CoverageCandidate>, sqlx::Error> {
    let rows: Vec<CoverageCandidate> = sqlx::query_as(
        "SELECT y.osm_node_id, y.lon, y.lat \
         FROM y_junctions y \
         LEFT JOIN google_streetview_coverage g ON g.osm_node_id = y.osm_node_id \
         WHERE g.osm_node_id IS NULL OR ($1 AND g.has_coverage = false)",
    )
    .bind(refresh)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

/// Upsert coverage results keyed by `osm_node_id`. Existing rows are
/// overwritten because a `--refresh` run can flip `false` to `true` once
/// Google adds imagery.
pub async fn upsert_coverage(pool: &PgPool, rows: &[(i64, bool)]) -> Result<usize, sqlx::Error> {
    if rows.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    // Keeps each statement well under PostgreSQL's parameter limit.
    const BATCH_SIZE: usize = 1000;
    let mut total = 0;

    for chunk in rows.chunks(BATCH_SIZE) {
        let mut qb = QueryBuilder::new(
            "INSERT INTO google_streetview_coverage (osm_node_id, has_coverage, queried_at) VALUES ",
        );

        for (i, (osm_node_id, has_coverage)) in chunk.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push("(");
            qb.push_bind(*osm_node_id);
            qb.push(", ");
            qb.push_bind(*has_coverage);
            qb.push(", NOW())");
        }

        qb.push(
            " ON CONFLICT (osm_node_id) DO UPDATE SET \
             has_coverage = EXCLUDED.has_coverage, \
             queried_at = NOW()",
        );

        let result = qb.build().execute(&mut *tx).await?;
        total += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(total)
}
