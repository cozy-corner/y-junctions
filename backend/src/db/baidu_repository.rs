use crate::domain::china::BaiduPanorama;
use crate::domain::Junction;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;

#[derive(Debug, FromRow)]
struct BaiduRow {
    osm_node_id: i64,
    panoid: String,
    pano_mc_x: f64,
    pano_mc_y: f64,
}

#[derive(Debug, FromRow)]
struct JunctionRow {
    id: i64,
    osm_node_id: i64,
    lat: f64,
    lon: f64,
    angle_1: i16,
    angle_2: i16,
    angle_3: i16,
    bearings: Vec<f32>,
    created_at: DateTime<Utc>,
    elevation: Option<f32>,
    min_elevation_diff: Option<f32>,
    max_elevation_diff: Option<f32>,
    min_angle_elevation_diff: Option<f32>,
    way_1_highway_type: Option<String>,
    way_2_highway_type: Option<String>,
    way_3_highway_type: Option<String>,
    way_1_category: Option<String>,
    way_2_category: Option<String>,
    way_3_category: Option<String>,
}

impl From<JunctionRow> for Junction {
    fn from(row: JunctionRow) -> Self {
        Junction {
            id: row.id,
            osm_node_id: row.osm_node_id,
            lat: row.lat,
            lon: row.lon,
            angle_1: row.angle_1,
            angle_2: row.angle_2,
            angle_3: row.angle_3,
            bearings: row.bearings,
            created_at: row.created_at,
            elevation: row.elevation.map(|e| e as f64),
            min_elevation_diff: row.min_elevation_diff.map(|e| e as f64),
            max_elevation_diff: row.max_elevation_diff.map(|e| e as f64),
            min_angle_elevation_diff: row.min_angle_elevation_diff.map(|e| e as f64),
            way_1_highway_type: row.way_1_highway_type,
            way_2_highway_type: row.way_2_highway_type,
            way_3_highway_type: row.way_3_highway_type,
            way_1_category: row.way_1_category,
            way_2_category: row.way_2_category,
            way_3_category: row.way_3_category,
        }
    }
}

/// Fetch saved Baidu panorama metadata for the given OSM node ids. Rows whose
/// `panoid` is NULL (tombstones for "queried, no coverage") are skipped so the
/// returned map contains only nodes that actually have a linked panorama.
pub async fn find_by_osm_node_ids(
    pool: &PgPool,
    osm_node_ids: &[i64],
) -> Result<HashMap<i64, BaiduPanorama>, sqlx::Error> {
    if osm_node_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<BaiduRow> = sqlx::query_as(
        "SELECT osm_node_id, panoid, pano_mc_x, pano_mc_y \
         FROM baidu_panoramas \
         WHERE osm_node_id = ANY($1) AND panoid IS NOT NULL",
    )
    .bind(osm_node_ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.osm_node_id,
                BaiduPanorama {
                    panoid: r.panoid,
                    pano_mc_x: r.pano_mc_x,
                    pano_mc_y: r.pano_mc_y,
                },
            )
        })
        .collect())
}

/// Fetch every junction that has never been queried against Baidu. A junction
/// is "never queried" iff no row exists in `baidu_panoramas` for its
/// `osm_node_id` — tombstones (queried-but-no-coverage) are stored as rows
/// with `panoid IS NULL` and excluded by the LEFT JOIN filter so re-runs
/// don't re-hit dead coordinates. Use `find_all_for_refresh` to force a full
/// re-query.
pub async fn find_without_baidu_panoid(pool: &PgPool) -> Result<Vec<Junction>, sqlx::Error> {
    let rows: Vec<JunctionRow> = sqlx::query_as(
        "SELECT y.id, y.osm_node_id, \
         y.lat, y.lon, \
         y.angle_1, y.angle_2, y.angle_3, y.bearings, y.created_at, \
         y.elevation, y.min_elevation_diff, y.max_elevation_diff, y.min_angle_elevation_diff, \
         y.way_1_highway_type, y.way_2_highway_type, y.way_3_highway_type, \
         y.way_1_category, y.way_2_category, y.way_3_category \
         FROM y_junctions y \
         LEFT JOIN baidu_panoramas bp ON bp.osm_node_id = y.osm_node_id \
         WHERE bp.osm_node_id IS NULL",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Junction::from).collect())
}

/// Fetch every junction regardless of existing panoid (for `--refresh`).
pub async fn find_all_for_refresh(pool: &PgPool) -> Result<Vec<Junction>, sqlx::Error> {
    let rows: Vec<JunctionRow> = sqlx::query_as(
        "SELECT id, osm_node_id, \
         lat, lon, \
         angle_1, angle_2, angle_3, bearings, created_at, \
         elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff, \
         way_1_highway_type, way_2_highway_type, way_3_highway_type, \
         way_1_category, way_2_category, way_3_category \
         FROM y_junctions",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Junction::from).collect())
}

/// Bulk upsert panorama metadata keyed by `osm_node_id`. Existing rows
/// (tombstones or stale panoids) are overwritten; `queried_at` is refreshed
/// to NOW() on every successful fetch. Batched with a single VALUES list per
/// chunk to avoid exceeding PostgreSQL's parameter limit.
pub async fn bulk_update_baidu(
    pool: &PgPool,
    updates: &[(i64, BaiduPanorama)],
) -> Result<usize, sqlx::Error> {
    if updates.is_empty() {
        return Ok(0);
    }

    let mut tx = pool.begin().await?;
    const BATCH_SIZE: usize = 1000;
    let mut total_updated = 0;

    for chunk in updates.chunks(BATCH_SIZE) {
        let mut qb = QueryBuilder::new(
            "INSERT INTO baidu_panoramas (osm_node_id, panoid, pano_mc_x, pano_mc_y, queried_at) VALUES ",
        );

        for (i, (osm_node_id, pano)) in chunk.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push("(");
            qb.push_bind(*osm_node_id);
            qb.push(", ");
            qb.push_bind(pano.panoid.clone());
            qb.push(", ");
            qb.push_bind(pano.pano_mc_x);
            qb.push(", ");
            qb.push_bind(pano.pano_mc_y);
            qb.push(", NOW())");
        }

        qb.push(
            " ON CONFLICT (osm_node_id) DO UPDATE SET \
             panoid = EXCLUDED.panoid, \
             pano_mc_x = EXCLUDED.pano_mc_x, \
             pano_mc_y = EXCLUDED.pano_mc_y, \
             queried_at = NOW()",
        );

        let result = qb.build().execute(&mut *tx).await?;
        total_updated += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(total_updated)
}

/// Stamp a tombstone (`panoid` NULL, `queried_at = NOW()`) for OSM nodes that
/// returned no panorama, so they're excluded from the next
/// `find_without_baidu_panoid` run. `ON CONFLICT DO NOTHING` keeps this
/// strictly a tombstone writer: if a row already exists in `baidu_panoramas`
/// (either a successful fetch or a prior tombstone) we leave it untouched —
/// callers that mistakenly pass a node id with an existing panoid won't
/// damage the success row's `queried_at`.
pub async fn bulk_mark_queried(pool: &PgPool, osm_node_ids: &[i64]) -> Result<usize, sqlx::Error> {
    if osm_node_ids.is_empty() {
        return Ok(0);
    }

    let mut qb = QueryBuilder::new("INSERT INTO baidu_panoramas (osm_node_id, queried_at) VALUES ");

    for (i, osm_node_id) in osm_node_ids.iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push("(");
        qb.push_bind(*osm_node_id);
        qb.push(", NOW())");
    }

    qb.push(" ON CONFLICT (osm_node_id) DO NOTHING");

    let result = qb.build().execute(pool).await?;
    Ok(result.rows_affected() as usize)
}
