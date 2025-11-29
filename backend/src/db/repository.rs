use crate::domain::{AngleType, Junction};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, QueryBuilder};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct FilterParams {
    pub angle_type: Option<Vec<AngleType>>,
    pub min_angle_lt: Option<i16>,
    pub min_angle_gt: Option<i16>,
    pub limit: Option<i64>,
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
    road_types: Vec<String>,
    created_at: DateTime<Utc>,
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
            road_types: row.road_types,
            created_at: row.created_at,
        }
    }
}

// ヘルパー関数: bboxフィルタを追加
fn add_bbox_filter(
    builder: &mut QueryBuilder<sqlx::Postgres>,
    bbox: (f64, f64, f64, f64),
) {
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
fn add_angle_type_filter(
    builder: &mut QueryBuilder<sqlx::Postgres>,
    angle_types: &[AngleType],
) {
    if angle_types.is_empty() {
        return;
    }

    builder.push(" AND (");
    for (i, angle_type) in angle_types.iter().enumerate() {
        if i > 0 {
            builder.push(" OR ");
        }
        match angle_type {
            AngleType::Sharp => {
                builder.push("angle_1 < 45");
            }
            AngleType::Even => {
                builder.push("(angle_1 >= 100 AND angle_3 <= 140)");
            }
            AngleType::Skewed => {
                builder.push("angle_3 > 200");
            }
            AngleType::Normal => {
                builder.push("(angle_1 >= 45 AND NOT (angle_1 >= 100 AND angle_3 <= 140) AND angle_3 <= 200)");
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
        builder.push(" AND angle_1 < ");
        builder.push_bind(lt);
    }

    if let Some(gt) = min_angle_gt {
        builder.push(" AND angle_1 > ");
        builder.push_bind(gt);
    }
}

pub async fn find_by_bbox(
    pool: &PgPool,
    bbox: (f64, f64, f64, f64), // (min_lon, min_lat, max_lon, max_lat)
    filters: FilterParams,
) -> Result<Vec<Junction>, sqlx::Error> {
    let limit = filters.limit.unwrap_or(500).min(1000);

    let mut query_builder = QueryBuilder::new(
        "SELECT id, osm_node_id, \
         ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon, \
         angle_1, angle_2, angle_3, road_types, created_at \
         FROM y_junctions ",
    );

    // bbox フィルタ
    add_bbox_filter(&mut query_builder, bbox);

    // angle_type フィルタ
    if let Some(ref angle_types) = filters.angle_type {
        add_angle_type_filter(&mut query_builder, angle_types);
    }

    // min_angle フィルタ
    add_min_angle_filters(&mut query_builder, filters.min_angle_lt, filters.min_angle_gt);

    // LIMIT
    query_builder.push(" LIMIT ");
    query_builder.push_bind(limit);

    let rows: Vec<JunctionRow> = query_builder.build_query_as().fetch_all(pool).await?;

    Ok(rows.into_iter().map(Junction::from).collect())
}

pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<Junction>, sqlx::Error> {
    let row: Option<JunctionRow> = sqlx::query_as(
        "SELECT id, osm_node_id, \
         ST_Y(location::geometry) as lat, ST_X(location::geometry) as lon, \
         angle_1, angle_2, angle_3, road_types, created_at \
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
             WHEN angle_1 < 45 THEN 'sharp' \
             WHEN angle_1 >= 100 AND angle_3 <= 140 THEN 'even' \
             WHEN angle_3 > 200 THEN 'skewed' \
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
