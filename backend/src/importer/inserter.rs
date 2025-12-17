use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};

use super::detector::JunctionForInsert;

const BATCH_SIZE: usize = 1000;

/// Insert Y-junctions into the database
pub async fn insert_junctions(pool: &PgPool, junctions: Vec<JunctionForInsert>) -> Result<()> {
    if junctions.is_empty() {
        tracing::info!("No junctions to insert");
        return Ok(());
    }

    let total_count = junctions.len();
    tracing::info!("Inserting {} junctions into database", total_count);

    // Start transaction
    let mut tx = pool.begin().await?;

    // Insert in batches
    let mut inserted_count = 0;

    for chunk in junctions.chunks(BATCH_SIZE) {
        insert_batch(&mut tx, chunk).await?;
        inserted_count += chunk.len();
        tracing::info!("Inserted {}/{} junctions", inserted_count, total_count);
    }

    // Commit transaction
    tx.commit().await?;

    tracing::info!("Successfully inserted all {} junctions", total_count);

    Ok(())
}

/// Insert a batch of junctions using bulk insert (single INSERT statement)
async fn insert_batch(
    tx: &mut Transaction<'_, Postgres>,
    junctions: &[JunctionForInsert],
) -> Result<()> {
    if junctions.is_empty() {
        return Ok(());
    }

    // Build VALUES clause dynamically for bulk insert
    // Example: VALUES ($1, ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4, $5, $6, ARRAY[$7, $8, $9], ...),
    //                 ($21, ST_SetSRID(ST_MakePoint($22, $23), 4326)::geography, $24, $25, $26, ARRAY[$27, $28, $29], ...), ...
    let mut query = String::from(
        "INSERT INTO y_junctions (osm_node_id, location, angle_1, angle_2, angle_3, bearings, \
         elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3, \
         elevation_diff_1, elevation_diff_2, elevation_diff_3, \
         min_angle_index, min_elevation_diff, max_elevation_diff) VALUES ",
    );

    const PARAMS_PER_ROW: usize = 19; // osm_node_id, lon, lat, angle_1, angle_2, angle_3, bearing_1, bearing_2, bearing_3,
                                      // elevation, neighbor_elevation_1~3, elevation_diff_1~3, min_angle_index, min/max_elevation_diff

    for (i, _) in junctions.iter().enumerate() {
        if i > 0 {
            query.push_str(", ");
        }
        let base = i * PARAMS_PER_ROW + 1;
        query.push_str(&format!(
            "(${}, ST_SetSRID(ST_MakePoint(${}, ${}), 4326)::geography, ${}, ${}, ${}, ARRAY[${}, ${}, ${}], \
             ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${}, ${})",
            base,        // osm_node_id
            base + 1,    // lon
            base + 2,    // lat
            base + 3,    // angle_1
            base + 4,    // angle_2
            base + 5,    // angle_3
            base + 6,    // bearing_1
            base + 7,    // bearing_2
            base + 8,    // bearing_3
            base + 9,    // elevation
            base + 10,   // neighbor_elevation_1
            base + 11,   // neighbor_elevation_2
            base + 12,   // neighbor_elevation_3
            base + 13,   // elevation_diff_1
            base + 14,   // elevation_diff_2
            base + 15,   // elevation_diff_3
            base + 16,   // min_angle_index
            base + 17,   // min_elevation_diff
            base + 18    // max_elevation_diff
        ));
    }

    query.push_str(" ON CONFLICT (osm_node_id) DO NOTHING");

    // Bind all parameters
    let mut q = sqlx::query(&query);
    for junction in junctions {
        q = q
            .bind(junction.osm_node_id)
            .bind(junction.lon) // lon first for ST_MakePoint
            .bind(junction.lat) // lat second for ST_MakePoint
            .bind(junction.angle_1)
            .bind(junction.angle_2)
            .bind(junction.angle_3)
            .bind(junction.bearings[0] as f32)
            .bind(junction.bearings[1] as f32)
            .bind(junction.bearings[2] as f32)
            .bind(junction.elevation)
            .bind(junction.neighbor_elevations.map(|e| e[0]))
            .bind(junction.neighbor_elevations.map(|e| e[1]))
            .bind(junction.neighbor_elevations.map(|e| e[2]))
            .bind(junction.elevation_diffs.map(|e| e[0]))
            .bind(junction.elevation_diffs.map(|e| e[1]))
            .bind(junction.elevation_diffs.map(|e| e[2]))
            .bind(junction.min_angle_index)
            .bind(junction.min_elevation_diff)
            .bind(junction.max_elevation_diff);
    }

    q.execute(&mut **tx).await?;

    Ok(())
}
