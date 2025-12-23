use crate::domain::{AngleType, Junction};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;

/// Elevation data for bulk updates
///
/// Note: `min_angle_elevation_diff` is NOT included here because it's a GENERATED ALWAYS column
/// in PostgreSQL (defined in migration 003_add_elevation.sql). The database automatically
/// calculates this value based on `min_angle_index` and `neighbor_elevation_*` columns.
/// Attempting to explicitly set it in an UPDATE statement would cause an error.
#[derive(Debug, Clone)]
pub struct ElevationUpdate {
    pub id: i64,
    pub elevation: f32,
    pub neighbor_elevations: [f32; 3],
    pub elevation_diffs: [f32; 3],
    pub min_angle_index: i16,
    pub min_elevation_diff: f32,
    pub max_elevation_diff: f32,
}

#[derive(Debug, Clone, Default)]
pub struct FilterParams {
    pub angle_type: Option<Vec<AngleType>>,
    pub min_angle_lt: Option<i16>,
    pub min_angle_gt: Option<i16>,
    pub limit: Option<i64>,
    // 最小角の高低差フィルタ
    pub min_angle_elevation_diff: Option<f64>,
    // 最大角の高低差フィルタ（範囲検索用）
    pub max_angle_elevation_diff: Option<f64>,
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
}

#[derive(Debug, FromRow)]
struct JunctionRowWithCount {
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
    total_count: i64,
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
        }
    }
}

impl From<JunctionRowWithCount> for Junction {
    fn from(row: JunctionRowWithCount) -> Self {
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
        }
    }
}

// ヘルパー関数: bboxフィルタを追加
fn add_bbox_filter(builder: &mut QueryBuilder<sqlx::Postgres>, bbox: (f64, f64, f64, f64)) {
    builder.push("WHERE location && ST_MakeEnvelope(");
    builder.push_bind(bbox.0);
    builder.push(", ");
    builder.push_bind(bbox.1);
    builder.push(", ");
    builder.push_bind(bbox.2);
    builder.push(", ");
    builder.push_bind(bbox.3);
    builder.push(", 4326)");
}

// ヘルパー関数: angle_typeフィルタを追加
fn add_angle_type_filter(builder: &mut QueryBuilder<sqlx::Postgres>, angle_types: &[AngleType]) {
    if angle_types.is_empty() {
        return;
    }

    builder.push(" AND (");
    for (i, angle_type) in angle_types.iter().enumerate() {
        if i > 0 {
            builder.push(" OR ");
        }
        match angle_type {
            AngleType::VerySharp => {
                builder.push("LEAST(angle_1, angle_2, angle_3) < 30");
            }
            AngleType::Sharp => {
                builder.push("(LEAST(angle_1, angle_2, angle_3) >= 30 AND LEAST(angle_1, angle_2, angle_3) < 45)");
            }
            AngleType::Normal => {
                builder.push("LEAST(angle_1, angle_2, angle_3) >= 45");
            }
        }
    }
    builder.push(")");
}

// ヘルパー関数: min_angleフィルタを追加
fn add_min_angle_filters(
    builder: &mut QueryBuilder<sqlx::Postgres>,
    min_angle_lt: Option<i16>,
    min_angle_gt: Option<i16>,
) {
    if let Some(lt) = min_angle_lt {
        builder.push(" AND LEAST(angle_1, angle_2, angle_3) < ");
        builder.push_bind(lt);
    }

    if let Some(gt) = min_angle_gt {
        builder.push(" AND LEAST(angle_1, angle_2, angle_3) > ");
        builder.push_bind(gt);
    }
}

// ヘルパー関数: 最小角の高低差フィルタを追加
fn add_elevation_filters(builder: &mut QueryBuilder<sqlx::Postgres>, filters: &FilterParams) {
    if let Some(min) = filters.min_angle_elevation_diff {
        builder.push(" AND min_angle_elevation_diff >= ");
        builder.push_bind(min);
    }

    if let Some(max) = filters.max_angle_elevation_diff {
        builder.push(" AND min_angle_elevation_diff <= ");
        builder.push_bind(max);
    }
}

// ヘルパー関数: 橋・トンネル除外フィルタを追加（常に除外）
fn add_bridge_tunnel_filter(builder: &mut QueryBuilder<sqlx::Postgres>) {
    builder.push(
        " AND NOT (way_1_bridge OR way_1_tunnel \
                OR way_2_bridge OR way_2_tunnel \
                OR way_3_bridge OR way_3_tunnel)",
    );
}

pub async fn find_by_bbox(
    pool: &PgPool,
    bbox: (f64, f64, f64, f64), // (min_lon, min_lat, max_lon, max_lat)
    filters: FilterParams,
) -> Result<(Vec<Junction>, i64), sqlx::Error> {
    let limit = filters.limit.unwrap_or(500).min(1000);

    let mut query_builder = QueryBuilder::new(
        "SELECT id, osm_node_id, \
         ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon, \
         angle_1, angle_2, angle_3, bearings, created_at, \
         elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff, \
         COUNT(*) OVER() as total_count \
         FROM y_junctions ",
    );

    // bbox フィルタ
    add_bbox_filter(&mut query_builder, bbox);

    // angle_type フィルタ
    if let Some(ref angle_types) = filters.angle_type {
        add_angle_type_filter(&mut query_builder, angle_types);
    }

    // min_angle フィルタ
    add_min_angle_filters(
        &mut query_builder,
        filters.min_angle_lt,
        filters.min_angle_gt,
    );

    // 最小角の高低差フィルタ
    add_elevation_filters(&mut query_builder, &filters);

    // 橋・トンネル除外フィルタ（高低差検索時のみ適用）
    if filters.min_angle_elevation_diff.is_some() || filters.max_angle_elevation_diff.is_some() {
        add_bridge_tunnel_filter(&mut query_builder);
    }

    // LIMIT
    query_builder.push(" LIMIT ");
    query_builder.push_bind(limit);

    let rows: Vec<JunctionRowWithCount> = query_builder.build_query_as().fetch_all(pool).await?;

    // total_count を最初の行から取得（全行同じ値）
    let total_count = rows.first().map(|r| r.total_count).unwrap_or(0);

    let junctions: Vec<Junction> = rows.into_iter().map(Junction::from).collect();

    Ok((junctions, total_count))
}

pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Junction>, sqlx::Error> {
    let row: Option<JunctionRow> = sqlx::query_as(
        "SELECT id, osm_node_id, \
         ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon, \
         angle_1, angle_2, angle_3, bearings, created_at, \
         elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff \
         FROM y_junctions \
         WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(Junction::from))
}

pub async fn count_by_type(pool: &PgPool) -> Result<HashMap<String, i64>, sqlx::Error> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT \
           CASE \
             WHEN LEAST(angle_1, angle_2, angle_3) < 30 THEN 'verysharp' \
             WHEN LEAST(angle_1, angle_2, angle_3) < 45 THEN 'sharp' \
             ELSE 'normal' \
           END as angle_type, \
           COUNT(*) as count \
         FROM y_junctions \
         GROUP BY angle_type",
    )
    .fetch_all(pool)
    .await?;

    let mut result = HashMap::new();
    for (angle_type, count) in rows {
        result.insert(angle_type, count);
    }

    Ok(result)
}

pub async fn count_total(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM y_junctions")
        .fetch_one(pool)
        .await?;

    Ok(row.0)
}

pub async fn find_all(pool: &PgPool) -> Result<Vec<Junction>, sqlx::Error> {
    let rows: Vec<JunctionRow> = sqlx::query_as(
        "SELECT id, osm_node_id, \
         ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon, \
         angle_1, angle_2, angle_3, bearings, created_at, \
         elevation, min_elevation_diff, max_elevation_diff, min_angle_elevation_diff \
         FROM y_junctions",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(Junction::from).collect())
}

pub async fn bulk_update_elevations(
    pool: &PgPool,
    updates: &[ElevationUpdate],
) -> Result<usize, sqlx::Error> {
    if updates.is_empty() {
        return Ok(0);
    }

    // Use transaction for atomicity
    let mut tx = pool.begin().await?;

    // Batch updates in chunks of 1000 to avoid exceeding PostgreSQL parameter limits
    const BATCH_SIZE: usize = 1000;
    let mut total_updated = 0;

    for chunk in updates.chunks(BATCH_SIZE) {
        let mut query_builder = QueryBuilder::new(
            "UPDATE y_junctions SET \
             elevation = updates.elevation, \
             neighbor_elevation_1 = updates.neighbor_elevation_1, \
             neighbor_elevation_2 = updates.neighbor_elevation_2, \
             neighbor_elevation_3 = updates.neighbor_elevation_3, \
             elevation_diff_1 = updates.elevation_diff_1, \
             elevation_diff_2 = updates.elevation_diff_2, \
             elevation_diff_3 = updates.elevation_diff_3, \
             min_angle_index = updates.min_angle_index, \
             min_elevation_diff = updates.min_elevation_diff, \
             max_elevation_diff = updates.max_elevation_diff \
             FROM (VALUES ",
        );

        for (i, update) in chunk.iter().enumerate() {
            if i > 0 {
                query_builder.push(", ");
            }
            query_builder.push("(");
            query_builder.push_bind(update.id);
            query_builder.push(", ");
            query_builder.push_bind(update.elevation);
            query_builder.push(", ");
            query_builder.push_bind(update.neighbor_elevations[0]);
            query_builder.push(", ");
            query_builder.push_bind(update.neighbor_elevations[1]);
            query_builder.push(", ");
            query_builder.push_bind(update.neighbor_elevations[2]);
            query_builder.push(", ");
            query_builder.push_bind(update.elevation_diffs[0]);
            query_builder.push(", ");
            query_builder.push_bind(update.elevation_diffs[1]);
            query_builder.push(", ");
            query_builder.push_bind(update.elevation_diffs[2]);
            query_builder.push(", ");
            query_builder.push_bind(update.min_angle_index);
            query_builder.push(", ");
            query_builder.push_bind(update.min_elevation_diff);
            query_builder.push(", ");
            query_builder.push_bind(update.max_elevation_diff);
            query_builder.push(")");
        }

        query_builder.push(
            ") AS updates(id, elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, \
             elevation_diff_1, elevation_diff_2, elevation_diff_3, min_angle_index, \
             min_elevation_diff, max_elevation_diff) \
             WHERE y_junctions.id = updates.id"
        );

        let result = query_builder.build().execute(&mut *tx).await?;
        total_updated += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(total_updated)
}
