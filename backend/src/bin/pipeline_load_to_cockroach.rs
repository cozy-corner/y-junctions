use anyhow::{Context, Result};
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

use y_junction_backend::importer::detector::JunctionForInsert;
use y_junction_backend::importer::inserter::insert_junctions;
use y_junction_backend::pipeline::parquet_io::{read_parquet_bytes, ServingJunctionParquetRecord};
use y_junction_backend::pipeline::storage::read_uri;

#[derive(Parser, Debug)]
#[command(name = "pipeline-load-to-cockroach")]
#[command(about = "Load extracted Parquet records into CockroachDB", long_about = None)]
struct Args {
    /// Parquet source URI — gs://...-yj-serving/... or file://...
    #[arg(long)]
    input: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    tracing::info!("load-to-cockroach: {}", args.input);

    let bytes = read_uri(&args.input).await?;
    tracing::info!("Read {} bytes of Parquet", bytes.len());

    let records: Vec<ServingJunctionParquetRecord> = read_parquet_bytes(bytes)?;
    tracing::info!("Decoded {} records", records.len());

    // ServingJunctionParquetRecord -> JunctionForInsert via From impl in
    // pipeline::parquet_io, which unwraps the -9999.0 elevation sentinel
    // back to None (issue #257).
    let junctions: Vec<JunctionForInsert> = records.into_iter().map(Into::into).collect();

    let database_url = std::env::var("DATABASE_URL")
        .context("DATABASE_URL must be set in environment or .env file")?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;
    tracing::info!("Database connection established");

    insert_junctions(&pool, junctions).await?;
    tracing::info!("Load complete");

    Ok(())
}
