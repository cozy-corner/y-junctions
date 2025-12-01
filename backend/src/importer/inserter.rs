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
    // Example: VALUES ($1, ST_SetSRID(ST_MakePoint($2, $3), 4326)::geography, $4, $5, $6, $7),
    //                 ($8, ST_SetSRID(ST_MakePoint($9, $10), 4326)::geography, $11, $12, $13, $14), ...
    let mut query = String::from(
        "INSERT INTO y_junctions (osm_node_id, location, angle_1, angle_2, angle_3) VALUES ",
    );

    const PARAMS_PER_ROW: usize = 6; // osm_node_id, lon, lat, angle_1, angle_2, angle_3

    for (i, _) in junctions.iter().enumerate() {
        if i > 0 {
            query.push_str(", ");
        }
        let base = i * PARAMS_PER_ROW + 1;
        query.push_str(&format!(
            "(${}, ST_SetSRID(ST_MakePoint(${}, ${}), 4326)::geography, ${}, ${}, ${})",
            base,
            base + 1,
            base + 2,
            base + 3,
            base + 4,
            base + 5
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
            .bind(junction.angle_3);
    }

    q.execute(&mut **tx).await?;

    Ok(())
}
