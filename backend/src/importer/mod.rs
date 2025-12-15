pub mod calculator;
pub mod detector;
pub mod elevation;
pub mod inserter;
pub mod parser;

use anyhow::Result;
use sqlx::PgPool;

pub async fn import_from_pbf(
    pool: &PgPool,
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<()> {
    tracing::info!("Opening PBF file: {}", input_path);

    // Parse PBF and extract Y-junctions
    let junctions = parser::parse_pbf(input_path, min_lon, min_lat, max_lon, max_lat)?;

    tracing::info!("Found {} Y-junctions to insert", junctions.len());

    // Insert into database
    inserter::insert_junctions(pool, junctions).await?;

    Ok(())
}
