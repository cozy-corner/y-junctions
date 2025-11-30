pub mod calculator;
pub mod detector;
pub mod parser;

use anyhow::Result;

pub async fn import_from_pbf(
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<()> {
    tracing::info!("Opening PBF file: {}", input_path);

    parser::parse_pbf(input_path, min_lon, min_lat, max_lon, max_lat)?;

    Ok(())
}
