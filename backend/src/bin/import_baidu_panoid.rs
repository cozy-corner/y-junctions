use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser, Debug)]
#[command(name = "import-baidu-panoid")]
#[command(about = "Fetch Baidu panoids for Chinese mainland Y-junctions")]
struct Args {
    /// Re-query every mainland-China junction (including ones that already
    /// have a panoid). Use this when panoids drift after Baidu re-imagings.
    #[arg(long)]
    refresh: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();

    tracing::info!("Starting Baidu panoid import (refresh={})", args.refresh);

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment or .env file");

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Database connection established");

    let count = y_junction_backend::importer::import_baidu_panoid_data(&pool, args.refresh).await?;

    tracing::info!("Baidu panoid import complete: {} junctions updated", count);

    Ok(())
}
