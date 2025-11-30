// Test comment for pre-commit hook verification
use axum::{routing::get, Router};
use sqlx::postgres::PgPoolOptions;
use tower_http::cors::{Any, CorsLayer};
use y_junction_backend::api;

#[tokio::main]
async fn main() {
    // .envファイル読み込み
    dotenvy::dotenv().ok();

    // ログ初期化
    tracing_subscriber::fmt::init();

    // データベース接続プール作成
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    tracing::info!("Connected to database");

    // CORS設定
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // ルーター作成
    let api_router = api::routes::create_router(pool);

    let app = Router::new()
        .route("/", get(|| async { "Y-Junction API" }))
        .route("/health", get(|| async { "OK" }))
        .merge(api_router)
        .layer(cors);

    // サーバー起動
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    tracing::info!("Server listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
