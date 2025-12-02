use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;

use crate::db::repository::{self, FilterParams};
use crate::domain::{AngleType, Junction};

// エラー型
#[derive(Debug)]
pub enum AppError {
    NotFound,
    BadRequest(&'static str),
    Internal(&'static str),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        eprintln!("Database error: {:?}", err);
        AppError::Internal("Database error")
    }
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Resource not found"),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };

        let body = Json(ErrorResponse {
            error: message.to_string(),
        });

        (status, body).into_response()
    }
}

// GET /api/junctions のクエリパラメータ
#[derive(Debug, Deserialize)]
pub struct JunctionsQuery {
    pub bbox: String,               // "min_lon,min_lat,max_lon,max_lat"
    pub angle_type: Option<String>, // "sharp,even" など
    pub min_angle_lt: Option<i16>,
    pub min_angle_gt: Option<i16>,
    pub limit: Option<i64>,
}

impl JunctionsQuery {
    fn parse_bbox(&self) -> Result<(f64, f64, f64, f64), AppError> {
        let parts: Vec<&str> = self.bbox.split(',').collect();
        if parts.len() != 4 {
            return Err(AppError::BadRequest(
                "bbox must be in format: min_lon,min_lat,max_lon,max_lat",
            ));
        }

        let coords: Result<Vec<f64>, _> = parts.iter().map(|s| s.parse::<f64>()).collect();
        let coords = coords.map_err(|_| AppError::BadRequest("Invalid bbox coordinates"))?;

        let (min_lon, min_lat, max_lon, max_lat) = (coords[0], coords[1], coords[2], coords[3]);

        // バリデーション
        if min_lon >= max_lon || min_lat >= max_lat {
            return Err(AppError::BadRequest("Invalid bbox range"));
        }

        if min_lon < -180.0 || max_lon > 180.0 || min_lat < -90.0 || max_lat > 90.0 {
            return Err(AppError::BadRequest("bbox out of valid range"));
        }

        Ok((min_lon, min_lat, max_lon, max_lat))
    }

    fn parse_angle_types(&self) -> Result<Option<Vec<AngleType>>, AppError> {
        if let Some(ref types_str) = self.angle_type {
            let types: Result<Vec<AngleType>, _> = types_str
                .split(',')
                .map(|s| match s.trim() {
                    "verysharp" => Ok(AngleType::VerySharp),
                    "sharp" => Ok(AngleType::Sharp),
                    "skewed" => Ok(AngleType::Skewed),
                    "normal" => Ok(AngleType::Normal),
                    _ => Err(AppError::BadRequest("Invalid angle_type")),
                })
                .collect();
            Ok(Some(types?))
        } else {
            Ok(None)
        }
    }

    fn to_filter_params(&self) -> Result<FilterParams, AppError> {
        // limit のバリデーション
        if let Some(v) = self.limit {
            if v <= 0 {
                return Err(AppError::BadRequest("limit must be a positive integer"));
            }
        }

        Ok(FilterParams {
            angle_type: self.parse_angle_types()?,
            min_angle_lt: self.min_angle_lt,
            min_angle_gt: self.min_angle_gt,
            limit: self.limit,
        })
    }
}

// GET /api/stats のレスポンス
#[derive(Serialize)]
pub struct StatsResponse {
    pub total_count: i64,
    pub by_type: HashMap<String, i64>,
}

// ハンドラー: GET /api/junctions
pub async fn get_junctions(
    State(pool): State<PgPool>,
    Query(query): Query<JunctionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let bbox = query.parse_bbox()?;
    let filters = query.to_filter_params()?;

    let (junctions, total_count) = repository::find_by_bbox(&pool, bbox, filters).await?;

    let feature_collection = Junction::to_feature_collection(junctions, total_count);

    Ok(Json(feature_collection))
}

// ハンドラー: GET /api/junctions/:id
pub async fn get_junction_by_id(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let junction = repository::find_by_id(&pool, id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(junction.to_feature()))
}

// ハンドラー: GET /api/stats
pub async fn get_stats(State(pool): State<PgPool>) -> Result<Json<StatsResponse>, AppError> {
    let total_count = repository::count_total(&pool).await?;
    let by_type = repository::count_by_type(&pool).await?;

    Ok(Json(StatsResponse {
        total_count,
        by_type,
    }))
}
