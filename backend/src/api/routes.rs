use axum::{routing::get, Router};
use sqlx::PgPool;

use super::handlers;

pub fn create_router(pool: PgPool) -> Router {
    Router::new()
        .route("/api/junctions", get(handlers::get_junctions))
        .route("/api/junctions/:id", get(handlers::get_junction_by_id))
        .route("/api/stats", get(handlers::get_stats))
        .with_state(pool)
}
