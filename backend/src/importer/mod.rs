pub mod calculator;
pub mod detector;
pub mod elevation;
pub mod inserter;
pub mod parser;

use anyhow::Result;
use rayon::prelude::*;
use sqlx::PgPool;
use std::sync::Arc;

pub async fn import_three_way_junctions(
    pool: &PgPool,
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<usize> {
    tracing::info!("Opening PBF file: {}", input_path);

    // Parse PBF and extract 3-way Y-junctions only
    let junctions = parser::parse_pbf_three_way(input_path, min_lon, min_lat, max_lon, max_lat)?;

    let count = junctions.len();
    tracing::info!("Found {} 3-way Y-junctions to insert", count);

    // Insert into database
    inserter::insert_junctions(pool, junctions).await?;

    Ok(count)
}

pub async fn import_two_way_junctions(
    pool: &PgPool,
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<usize> {
    tracing::info!("Opening PBF file: {}", input_path);

    // Parse PBF and extract 2-way Y-junctions only
    let junctions = parser::parse_pbf_two_way(input_path, min_lon, min_lat, max_lon, max_lat)?;

    let count = junctions.len();
    tracing::info!("Found {} 2-way Y-junctions to insert", count);

    // Insert into database
    inserter::insert_junctions(pool, junctions).await?;

    Ok(count)
}

pub async fn import_elevation_data(pool: &PgPool, elevation_dir: &str) -> Result<usize> {
    tracing::info!("Starting elevation data import from: {}", elevation_dir);

    // Initialize elevation provider (thread-safe)
    let elevation_provider = Arc::new(elevation::ElevationProvider::new(elevation_dir)?);

    // Fetch junctions without elevation data from database using repository
    let junctions = crate::db::repository::find_without_elevation(pool).await?;

    tracing::info!(
        "Found {} Y-junctions without elevation data to process",
        junctions.len()
    );

    // Process junctions in parallel
    let elevation_updates: Vec<crate::db::repository::ElevationUpdate> = junctions
        .par_iter()
        .filter_map(|junction| {
            // Get junction elevation
            let junction_elev = match elevation_provider.get_elevation(junction.lat, junction.lon) {
                Ok(Some(elev)) => elev,
                Ok(None) => {
                    // No elevation data available
                    return None;
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to get elevation for junction {} at ({}, {}): {}",
                        junction.id,
                        junction.lat,
                        junction.lon,
                        e
                    );
                    return None;
                }
            };

            // Calculate neighbor coordinates (approximately 10m away)
            let neighbor_coords: Vec<(f64, f64)> = junction
                .bearings
                .iter()
                .map(|&bearing| {
                    calculator::calculate_neighbor_coord(
                        junction.lat,
                        junction.lon,
                        bearing as f64,
                        10.0,
                    )
                })
                .collect();

            // Get neighbor elevations
            let neighbor_elevs: Vec<Option<f64>> = neighbor_coords
                .iter()
                .map(|(lat, lon)| elevation_provider.get_elevation(*lat, *lon).ok().flatten())
                .collect();

            // Only update if all neighbor elevations are available
            let [Some(n1), Some(n2), Some(n3)] =
                [neighbor_elevs[0], neighbor_elevs[1], neighbor_elevs[2]]
            else {
                return None;
            };

            let neighbor_elevations = [n1, n2, n3];
            let angles = [junction.angle_1, junction.angle_2, junction.angle_3];

            let elevation_diffs = detector::JunctionForInsert::calculate_elevation_diffs(
                junction_elev,
                &neighbor_elevations,
            );
            let (min_diff, max_diff) =
                detector::JunctionForInsert::calculate_min_max_diffs(&elevation_diffs);
            let min_angle_index = detector::JunctionForInsert::calculate_min_angle_index(&angles);

            Some(crate::db::repository::ElevationUpdate {
                id: junction.id,
                elevation: junction_elev as f32,
                neighbor_elevations: [n1 as f32, n2 as f32, n3 as f32],
                elevation_diffs: [
                    elevation_diffs[0] as f32,
                    elevation_diffs[1] as f32,
                    elevation_diffs[2] as f32,
                ],
                min_angle_index,
                min_elevation_diff: min_diff as f32,
                max_elevation_diff: max_diff as f32,
            })
        })
        .collect();

    tracing::info!(
        "Elevation collection stats: total={}, collected={}",
        junctions.len(),
        elevation_updates.len()
    );

    tracing::info!(
        "Collected {} elevation updates, performing bulk update",
        elevation_updates.len()
    );

    // Perform bulk update using repository
    let updated_count =
        crate::db::repository::bulk_update_elevations(pool, &elevation_updates).await?;

    tracing::info!("Updated {} Y-junctions with elevation data", updated_count);

    Ok(updated_count)
}
