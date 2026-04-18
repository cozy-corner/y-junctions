use crate::domain::china::BaiduPanorama;
use crate::domain::Junction;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;

#[derive(Debug, FromRow)]
struct BaiduRow {
    id: i64,
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

/// Fetch saved Baidu panorama metadata for the given junction ids. Rows with
/// NULL panoid are skipped so the returned map contains only junctions that
/// actually have a linked panorama.
pub async fn find_by_junction_ids(
    pool: &PgPool,
    ids: &[i64],
) -> Result<HashMap<i64, BaiduPanorama>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let rows: Vec<BaiduRow> = sqlx::query_as(
        "SELECT id, baidu_panoid AS panoid, \
                baidu_pano_mc_x AS pano_mc_x, baidu_pano_mc_y AS pano_mc_y \
         FROM y_junctions \
         WHERE id = ANY($1) AND baidu_panoid IS NOT NULL",
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.id,
                BaiduPanorama {
                    panoid: r.panoid,
                    pano_mc_x: r.pano_mc_x,
                    pano_mc_y: r.pano_mc_y,
                },
            )
        })
        .collect())
}

/// Fetch every junction that has never been queried against Baidu. Rows that
/// were queried previously and returned no coverage are skipped via the
/// `baidu_queried_at` tombstone so re-runs don't re-hit dead coordinates;
/// use `find_all_for_refresh` to force a full re-query.
pub async fn find_without_baidu_panoid(pool: &PgPool) -> Result<Vec<Junction>, sqlx::Error> {
    let rows: Vec<JunctionRow> = sqlx::query_as(
        "SELECT id, osm_node_id, \
         lat, lon, \
         angle_1, angle_2, angle_3, bearings, created_at, \
         elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff, \
         way_1_highway_type, way_2_highway_type, way_3_highway_type, \
         way_1_category, way_2_category, way_3_category \
         FROM y_junctions \
         WHERE baidu_panoid IS NULL AND baidu_queried_at IS NULL",
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

/// Bulk upsert panorama metadata for the given junction ids. Batched with a
/// `FROM (VALUES ...)` update to avoid exceeding PostgreSQL's parameter limit.
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
            "UPDATE y_junctions SET \
             baidu_panoid = updates.panoid, \
             baidu_pano_mc_x = updates.pano_mc_x, \
             baidu_pano_mc_y = updates.pano_mc_y, \
             baidu_queried_at = NOW() \
             FROM (VALUES ",
        );

        for (i, (id, pano)) in chunk.iter().enumerate() {
            if i > 0 {
                qb.push(", ");
            }
            qb.push("(");
            qb.push_bind(*id);
            qb.push(", ");
            qb.push_bind(pano.panoid.clone());
            qb.push(", ");
            qb.push_bind(pano.pano_mc_x);
            qb.push(", ");
            qb.push_bind(pano.pano_mc_y);
            qb.push(")");
        }

        qb.push(
            ") AS updates(id, panoid, pano_mc_x, pano_mc_y) \
             WHERE y_junctions.id = updates.id",
        );

        let result = qb.build().execute(&mut *tx).await?;
        total_updated += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(total_updated)
}

/// Stamp `baidu_queried_at = NOW()` for junctions that returned no panorama
/// so they're excluded from the next `find_without_baidu_panoid` run. The
/// panoid columns stay NULL (there's nothing to store).
pub async fn bulk_mark_queried(pool: &PgPool, ids: &[i64]) -> Result<usize, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }

    let result = sqlx::query("UPDATE y_junctions SET baidu_queried_at = NOW() WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() as usize)
}
