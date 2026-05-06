use std::path::PathBuf;

use anyhow::{Context, Result};
use bytes::Bytes;
use clap::Parser;
use tempfile::NamedTempFile;

use y_junction_backend::importer::parser;
use y_junction_backend::pipeline::parquet_io::{write_parquet_bytes, JunctionParquetRecord};
use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-extract-two-way")]
#[command(about = "Extract 2-way Y-junctions from a PBF and write a Parquet file", long_about = None)]
struct Args {
    /// PBF source URI — file://..., gs://..., or https://...
    #[arg(long)]
    input: String,

    /// Extracted-stage output URI — gs://...-yj-extracted/... or file://...
    #[arg(long)]
    extracted_output: String,

    /// Bounding box: min_lon,min_lat,max_lon,max_lat
    #[arg(long)]
    bbox: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    let bbox = parse_bbox(&args.bbox)?;

    tracing::info!(
        "extract-two-way: {} -> {}",
        args.input,
        args.extracted_output
    );

    let local_pbf = stage_pbf_locally(&args.input).await?;
    let pbf_path = local_pbf.path_str();
    tracing::info!("PBF staged at {}", pbf_path);

    let junctions = parser::parse_pbf_two_way(pbf_path, bbox.0, bbox.1, bbox.2, bbox.3)?;
    tracing::info!("Extracted {} 2-way junctions", junctions.len());

    let records: Vec<JunctionParquetRecord> = junctions.into_iter().map(Into::into).collect();
    let parquet_bytes = write_parquet_bytes(&records)?;
    tracing::info!("Encoded {} bytes of Parquet", parquet_bytes.len());

    write_uri(&args.extracted_output, Bytes::from(parquet_bytes)).await?;
    tracing::info!("Wrote extracted Parquet");

    Ok(())
}

fn parse_bbox(s: &str) -> Result<(f64, f64, f64, f64)> {
    let parts: Vec<&str> = s.split(',').collect();
    anyhow::ensure!(
        parts.len() == 4,
        "bbox must be min_lon,min_lat,max_lon,max_lat"
    );
    Ok((
        parts[0].parse().context("min_lon")?,
        parts[1].parse().context("min_lat")?,
        parts[2].parse().context("max_lon")?,
        parts[3].parse().context("max_lat")?,
    ))
}

/// `osmpbf::ElementReader` requires a real `File`, so non-`file://`
/// inputs are downloaded to tmpfs before parsing. See
/// `pipeline_extract_three_way.rs` for the streaming-deferred rationale.
enum LocalPbf {
    Existing(PathBuf),
    Owned(NamedTempFile),
}

impl LocalPbf {
    fn path_str(&self) -> &str {
        match self {
            LocalPbf::Existing(p) => p.to_str().expect("non-UTF8 PBF path"),
            LocalPbf::Owned(t) => t.path().to_str().expect("non-UTF8 PBF path"),
        }
    }
}

async fn stage_pbf_locally(input: &str) -> Result<LocalPbf> {
    if let Some(rest) = input.strip_prefix("file://") {
        return Ok(LocalPbf::Existing(PathBuf::from(rest)));
    }
    let bytes = read_uri(input).await?;
    let mut tmp = NamedTempFile::new()?;
    use std::io::Write as _;
    tmp.as_file_mut().write_all(&bytes)?;
    tmp.as_file_mut().flush()?;
    Ok(LocalPbf::Owned(tmp))
}
