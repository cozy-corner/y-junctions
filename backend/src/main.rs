use axum::{routing::get, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use y_junction_backend::api;

#[tokio::main]
async fn main() {
    // .envファイル読み込み
    dotenvy::dotenv().ok();

    // プロジェクトIDを環境変数から取得
    let project_id =
        std::env::var("GCP_PROJECT_ID").unwrap_or_else(|_| "y-junctions-prod".to_string());

    // Google Cloud Logging形式のログ初期化
    y_junction_backend::logging::init_google_cloud_logging(project_id)
        .expect("Failed to initialize logging");

    // データベース接続プール作成
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let start = std::time::Instant::now();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_lazy(&database_url)
        .expect("Failed to create connection pool");
    let pool_create_duration = start.elapsed();

    tracing::info!(
        "Connection pool created (lazy) in {:?}",
        pool_create_duration
    );

    // CORS設定
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // ルーター作成
    let api_router = api::routes::create_router(pool);

    let app = Router::new()
        .route("/", get(|| async { "Y-Junction API" }))
        .route("/health", get(health_handler))
        .merge(api_router)
        .layer(cors)
        .layer(axum::middleware::from_fn(
            y_junction_backend::middleware::trace_context_middleware,
        ));

    // サーバー起動
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    tracing::info!("Server listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn health_handler() -> &'static str {
    tracing::info!("Health check endpoint called");
    "OK"
}
