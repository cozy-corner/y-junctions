use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser, Debug)]
#[command(name = "import-google-streetview")]
#[command(about = "Query Google Street View coverage for non-China Y-junctions")]
struct Args {
    /// Also re-query junctions already recorded as uncovered. Google adds
    /// imagery over time, so `has_coverage = false` is not permanent.
    #[arg(long)]
    refresh: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();

    tracing::info!(
        "Starting Street View coverage lookup (refresh={})",
        args.refresh
    );

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment or .env file");

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Database connection established");

    let count =
        y_junction_backend::importer::import_google_coverage_data(&pool, args.refresh).await?;

    tracing::info!(
        "Street View coverage lookup complete: {} rows written",
        count
    );

    Ok(())
}
