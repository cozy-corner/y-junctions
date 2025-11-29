use anyhow::Result;
use clap::Parser;

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
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

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

    // Import from PBF (skeleton - actual processing in next phases)
    y_junction_backend::importer::import_from_pbf(&args.input, min_lon, min_lat, max_lon, max_lat)
        .await?;

    tracing::info!("Import process completed");

    Ok(())
}
