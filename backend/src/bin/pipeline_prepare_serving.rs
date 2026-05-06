use anyhow::Result;
use bytes::Bytes;
use clap::Parser;

use y_junction_backend::pipeline::parquet_io::{
    read_parquet_bytes, write_parquet_bytes, JunctionParquetRecord,
};
use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-prepare-serving")]
#[command(about = "Merge extracted Parquet inputs into a single serving Parquet", long_about = None)]
struct Args {
    /// Extracted-stage Parquet URI — repeat for each input (3-way / 2-way / ...)
    #[arg(long = "input", required = true)]
    inputs: Vec<String>,

    /// Serving-stage Parquet URI — gs://...-yj-serving/... or file://...
    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    tracing::info!(
        "prepare-serving: {} inputs -> {}",
        args.inputs.len(),
        args.output
    );

    let mut merged: Vec<JunctionParquetRecord> = Vec::new();
    for input in &args.inputs {
        let body = read_uri(input).await?;
        tracing::info!("Read {} bytes from {}", body.len(), input);
        let records = read_parquet_bytes(body)?;
        tracing::info!("Decoded {} records from {}", records.len(), input);
        merged.extend(records);
    }

    tracing::info!("Merged {} total records", merged.len());

    let parquet_bytes = write_parquet_bytes(&merged)?;
    tracing::info!("Encoded {} bytes of serving Parquet", parquet_bytes.len());

    write_uri(&args.output, Bytes::from(parquet_bytes)).await?;
    tracing::info!("Wrote serving copy");

    Ok(())
}
