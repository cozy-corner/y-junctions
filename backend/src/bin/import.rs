use anyhow::Result;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser, Debug)]
#[command(name = "import")]
#[command(about = "Import Y-junctions from OSM PBF file", long_about = None)]
struct Args {
    /// Path to OSM PBF file
    #[arg(short, long)]
    input: String,

    /// Bounding box: min_lon,min_lat,max_lon,max_lat
    #[arg(short, long)]
    bbox: String,

    /// Directory containing elevation data (e.g., GSI XML files)
    #[arg(long)]
    elevation_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Load environment variables from .env file
    dotenvy::dotenv().ok();

    let args = Args::parse();

    tracing::info!("Starting import process");
    tracing::info!("Input file: {}", args.input);
    tracing::info!("Bounding box: {}", args.bbox);

    // Parse bbox
    let bbox_parts: Vec<&str> = args.bbox.split(',').collect();
    if bbox_parts.len() != 4 {
        anyhow::bail!("Invalid bbox format. Expected: min_lon,min_lat,max_lon,max_lat");
    }

    let min_lon: f64 = bbox_parts[0].parse()?;
    let min_lat: f64 = bbox_parts[1].parse()?;
    let max_lon: f64 = bbox_parts[2].parse()?;
    let max_lat: f64 = bbox_parts[3].parse()?;

    tracing::info!(
        "Parsed bbox: min_lon={}, min_lat={}, max_lon={}, max_lat={}",
        min_lon,
        min_lat,
        max_lon,
        max_lat
    );

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment or .env file");

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Database connection established");

    // Import from PBF
    y_junction_backend::importer::import_from_pbf(
        &pool,
        &args.input,
        args.elevation_dir.as_deref(),
        min_lon,
        min_lat,
        max_lon,
        max_lat,
    )
    .await?;

    tracing::info!("Import process completed");

    Ok(())
}
