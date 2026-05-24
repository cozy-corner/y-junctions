//! Serving-stage Cloud Run Job.
//!
//! Reads N extracted-stage Parquet inputs (`--extracted`, one per
//! extract variant like 3-way / 2-way) and 0..M enrichment-stage Parquet
//! inputs (`--enrichment`, currently only elevation), LEFT JOINs the
//! enrichments onto the extracted records by `osm_node_id`, and writes a
//! single [`ServingJunctionParquetRecord`] Parquet file at `--output`.
//!
//! Extracted rows with no matching enrichment get `-9999.0` sentinel
//! values in the elevation columns; the load-to-cockroach `From` impl
//! maps these back to `None` (issue #257).

use std::collections::HashMap;

use anyhow::Result;
use bytes::Bytes;
use clap::Parser;

use y_junction_backend::pipeline::parquet_io::{
    read_parquet_bytes, write_parquet_bytes, ElevationParquetRecord, JunctionParquetRecord,
    ServingJunctionParquetRecord,
};
use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-prepare-serving")]
#[command(
    about = "LEFT JOIN extracted Parquet inputs with optional enrichment Parquet inputs into a single serving Parquet"
)]
struct Args {
    /// Extracted-stage Parquet URI — repeat for each input (3-way / 2-way / ...)
    #[arg(long = "extracted", required = true)]
    extracted: Vec<String>,

    /// Enrichment-stage Parquet URI — repeat for each enrichment source.
    /// Currently only elevation. Joined by `osm_node_id` LEFT JOIN.
    /// Workflows may pass empty strings when the region has no enrichment;
    /// those are filtered before reading.
    #[arg(long = "enrichment")]
    enrichment: Vec<String>,

    /// Serving-stage Parquet URI — gs://...-yj-serving/... or file://...
    #[arg(long)]
    output: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    let enrichment_uris: Vec<&str> = args
        .enrichment
        .iter()
        .filter(|s| !s.is_empty())
        .map(String::as_str)
        .collect();

    tracing::info!(
        "prepare-serving: {} extracted + {} enrichment -> {}",
        args.extracted.len(),
        enrichment_uris.len(),
        args.output
    );

    // Read all extracted records (concat across 3-way / 2-way / ...)
    let mut extracted: Vec<JunctionParquetRecord> = Vec::new();
    for input in &args.extracted {
        let body = read_uri(input).await?;
        tracing::info!("Read {} bytes from {}", body.len(), input);
        let records: Vec<JunctionParquetRecord> = read_parquet_bytes(body)?;
        tracing::info!("Decoded {} extracted records from {}", records.len(), input);
        extracted.extend(records);
    }
    tracing::info!("Total {} extracted records", extracted.len());

    // Read all enrichments into a single osm_node_id -> ElevationParquetRecord
    // map. Multiple enrichment files (e.g. 3-way + 2-way) share the same
    // key space because osm_node_id is globally unique per junction; later
    // writes win but collisions are not expected in practice.
    let mut enrichment_map: HashMap<i64, ElevationParquetRecord> = HashMap::new();
    for input in &enrichment_uris {
        let body = read_uri(input).await?;
        tracing::info!("Read {} bytes from {}", body.len(), input);
        let records: Vec<ElevationParquetRecord> = read_parquet_bytes(body)?;
        tracing::info!(
            "Decoded {} enrichment records from {}",
            records.len(),
            input
        );
        for r in records {
            enrichment_map.insert(r.osm_node_id, r);
        }
    }
    tracing::info!(
        "Total {} unique enrichment rows in lookup map",
        enrichment_map.len()
    );

    // LEFT JOIN: every extracted row becomes a serving row; matching
    // enrichment rows fill in elevation columns, others retain sentinel.
    let mut joined_count = 0usize;
    let serving: Vec<ServingJunctionParquetRecord> = extracted
        .into_iter()
        .map(|j| {
            let osm_node_id = j.osm_node_id;
            let base = ServingJunctionParquetRecord::from_extracted(j);
            if let Some(e) = enrichment_map.get(&osm_node_id) {
                joined_count += 1;
                base.with_enrichment(e.clone())
            } else {
                base
            }
        })
        .collect();
    tracing::info!(
        "LEFT JOIN: {} of {} extracted rows received enrichment",
        joined_count,
        serving.len()
    );

    let parquet_bytes = write_parquet_bytes(&serving)?;
    tracing::info!("Encoded {} bytes of serving Parquet", parquet_bytes.len());

    write_uri(&args.output, Bytes::from(parquet_bytes)).await?;
    tracing::info!("Wrote serving copy");

    Ok(())
}
