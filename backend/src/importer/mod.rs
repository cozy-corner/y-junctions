pub mod calculator;
pub mod detector;
pub mod elevation;
pub mod inserter;
pub mod parser;

use anyhow::Result;
use sqlx::PgPool;

pub async fn import_osm_data(
    pool: &PgPool,
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<usize> {
    tracing::info!("Opening PBF file: {}", input_path);

    // Parse PBF and extract Y-junctions (without elevation data)
    let junctions = parser::parse_pbf(input_path, min_lon, min_lat, max_lon, max_lat)?;

    let count = junctions.len();
    tracing::info!("Found {} Y-junctions to insert", count);

    // Insert into database
    inserter::insert_junctions(pool, junctions).await?;

    Ok(count)
}

pub async fn import_elevation_data(pool: &PgPool, elevation_dir: &str) -> Result<usize> {
    tracing::info!("Starting elevation data import from: {}", elevation_dir);

    // Initialize elevation provider
    let mut elevation_provider = elevation::ElevationProvider::new(elevation_dir)?;

    // Fetch all junctions from database using repository
    let junctions = crate::db::repository::find_all(pool).await?;

    tracing::info!(
        "Found {} Y-junctions to enrich with elevation",
        junctions.len()
    );

    // Collect all elevation updates in memory first
    let mut elevation_updates = Vec::new();
    let mut skipped_no_junction_elev = 0;
    let mut skipped_no_neighbor_elev = 0;

    for (idx, junction) in junctions.iter().enumerate() {
        // Get junction elevation
        let junction_elevation = elevation_provider.get_elevation(junction.lat, junction.lon)?;

        // Skip if no elevation data available
        let Some(junction_elev) = junction_elevation else {
            skipped_no_junction_elev += 1;
            if idx < 10 {
                tracing::warn!(
                    "Junction {} at ({}, {}) has no elevation data",
                    junction.id,
                    junction.lat,
                    junction.lon
                );
            }
            continue;
        };

        if idx < 5 {
            tracing::info!("Junction {} got elevation: {}m", junction.id, junction_elev);
        }

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
        if let [Some(n1), Some(n2), Some(n3)] =
            [neighbor_elevs[0], neighbor_elevs[1], neighbor_elevs[2]]
        {
            let neighbor_elevations = [n1, n2, n3];
            let angles = [junction.angle_1, junction.angle_2, junction.angle_3];

            let elevation_diffs = detector::JunctionForInsert::calculate_elevation_diffs(
                junction_elev,
                &neighbor_elevations,
            );
            let (min_diff, max_diff) =
                detector::JunctionForInsert::calculate_min_max_diffs(&elevation_diffs);
            let min_angle_index = detector::JunctionForInsert::calculate_min_angle_index(&angles);

            elevation_updates.push(crate::db::repository::ElevationUpdate {
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
            });
        } else {
            skipped_no_neighbor_elev += 1;
            if idx < 5 {
                tracing::warn!(
                    "Junction {} missing neighbor elevations: [{:?}, {:?}, {:?}]",
                    junction.id,
                    neighbor_elevs[0],
                    neighbor_elevs[1],
                    neighbor_elevs[2]
                );
            }
        }
    }

    tracing::info!(
        "Elevation collection stats: total={}, skipped_no_junction={}, skipped_no_neighbors={}, collected={}",
        junctions.len(), skipped_no_junction_elev, skipped_no_neighbor_elev, elevation_updates.len()
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
