use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser, Debug)]
#[command(name = "import-elevation")]
#[command(about = "Import elevation data for existing Y-junctions")]
struct Args {
    /// Directory containing elevation data (e.g., GSI XML files)
    #[arg(long)]
    elevation_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    let args = Args::parse();

    tracing::info!("Starting elevation import process");
    tracing::info!("Elevation directory: {}", args.elevation_dir);

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment or .env file");

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Database connection established");

    // Import elevation data
    let count =
        y_junction_backend::importer::import_elevation_data(&pool, &args.elevation_dir).await?;

    tracing::info!(
        "Elevation import process completed: {} junctions updated",
        count
    );

    Ok(())
}
