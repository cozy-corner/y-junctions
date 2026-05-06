use anyhow::Result;
use clap::Parser;

use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-prepare-serving")]
#[command(about = "Stage extracted Parquet into the serving bucket", long_about = None)]
struct Args {
    /// Extracted-stage Parquet URI — gs://...-yj-extracted/... or file://...
    #[arg(long)]
    input: String,

    /// Serving-stage Parquet URI — gs://...-yj-serving/... or file://...
    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    tracing::info!("prepare-serving: {} -> {}", args.input, args.output);

    let body = read_uri(&args.input).await?;
    tracing::info!("Read {} bytes from extracted", body.len());

    write_uri(&args.output, body).await?;
    tracing::info!("Wrote serving copy");

    Ok(())
}
