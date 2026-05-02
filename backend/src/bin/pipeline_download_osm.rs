use anyhow::Result;
use clap::Parser;

use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-download-osm")]
#[command(about = "Download an OSM PBF and stage it in raw object storage", long_about = None)]
struct Args {
    /// Source URI — https://... (Geofabrik), file://..., or gs://...
    #[arg(long)]
    input: String,

    /// Destination URI — gs://... or file://...
    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    tracing::info!("download-osm: {} -> {}", args.input, args.output);

    let body = read_uri(&args.input).await?;
    tracing::info!("Read {} bytes from source", body.len());

    write_uri(&args.output, body).await?;
    tracing::info!("Upload complete");

    Ok(())
}
