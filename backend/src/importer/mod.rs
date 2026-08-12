pub mod baidu;
pub mod calculator;
pub mod detector;
pub mod elevation;
pub mod google;
pub mod inserter;
pub mod parser;

use anyhow::Result;
use rayon::prelude::*;
use sqlx::PgPool;
use std::sync::Arc;

use crate::domain::china;

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

/// Looks up DEM elevation, logging real errors but treating out-of-
/// coverage as a silent `None`. ElevationProvider returns Ok(None) when
/// the mesh isn't in mesh_to_file (e.g. non-Japan coordinates), and Err
/// only on actual file I/O / parse / gzip failures — the latter are
/// operational issues worth surfacing in logs.
fn try_get_elevation(provider: &elevation::ElevationProvider, lat: f64, lon: f64) -> Option<f64> {
    match provider.get_elevation(lat, lon) {
        Ok(opt) => opt,
        Err(e) => {
            tracing::warn!("ElevationProvider error at ({lat}, {lon}): {e:#}");
            None
        }
    }
}

/// Per-junction elevation enrichment payload — what
/// [`compute_elevation_enrichment`] returns on success. All fields use
/// f64 to match `ElevationProvider`'s native return type; downstream
/// consumers (DB UPDATE, Parquet write) cast to f32 at their boundary.
#[derive(Debug, Clone, Copy)]
pub struct ElevationEnrichment {
    pub elevation: f64,
    pub neighbor_elevations: [f64; 3],
    pub elevation_diffs: [f64; 3],
    pub min_angle_index: i16,
    pub min_elevation_diff: f64,
    pub max_elevation_diff: f64,
}

/// Compute elevation enrichment for a single junction. Returns `None` if
/// the junction's own coordinate OR any of the three 10m-neighbor
/// coordinates has no DEM coverage. Out-of-coverage rows (e.g. China,
/// edges of Japan) return `None` silently — the GSI DEM lookup
/// distinguishes "mesh not present" (Ok(None)) from "mesh present but
/// failed to read" (Err), and only the latter emits a warn log. No DB
/// access. Shared by `import_elevation_data` (legacy DB-direct CLI) and
/// `pipeline-enrich-elevation` (Cloud Run Job, issue #257).
pub fn compute_elevation_enrichment(
    provider: &elevation::ElevationProvider,
    lat: f64,
    lon: f64,
    bearings: &[f64; 3],
    angles: &[i16; 3],
) -> Option<ElevationEnrichment> {
    let junction_elev = try_get_elevation(provider, lat, lon)?;

    let neighbor_coords = [
        calculator::calculate_neighbor_coord(lat, lon, bearings[0], 10.0),
        calculator::calculate_neighbor_coord(lat, lon, bearings[1], 10.0),
        calculator::calculate_neighbor_coord(lat, lon, bearings[2], 10.0),
    ];

    let neighbor_elevations = [
        try_get_elevation(provider, neighbor_coords[0].0, neighbor_coords[0].1)?,
        try_get_elevation(provider, neighbor_coords[1].0, neighbor_coords[1].1)?,
        try_get_elevation(provider, neighbor_coords[2].0, neighbor_coords[2].1)?,
    ];

    let elevation_diffs =
        detector::JunctionForInsert::calculate_elevation_diffs(junction_elev, &neighbor_elevations);
    let (min_elevation_diff, max_elevation_diff) =
        detector::JunctionForInsert::calculate_min_max_diffs(&elevation_diffs);
    let min_angle_index = detector::JunctionForInsert::calculate_min_angle_index(angles);

    Some(ElevationEnrichment {
        elevation: junction_elev,
        neighbor_elevations,
        elevation_diffs,
        min_angle_index,
        min_elevation_diff,
        max_elevation_diff,
    })
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
            let bearings = [
                junction.bearings[0] as f64,
                junction.bearings[1] as f64,
                junction.bearings[2] as f64,
            ];
            let angles = [junction.angle_1, junction.angle_2, junction.angle_3];
            let enrich = compute_elevation_enrichment(
                &elevation_provider,
                junction.lat,
                junction.lon,
                &bearings,
                &angles,
            )?;
            Some(crate::db::repository::ElevationUpdate {
                id: junction.id,
                elevation: enrich.elevation as f32,
                neighbor_elevations: [
                    enrich.neighbor_elevations[0] as f32,
                    enrich.neighbor_elevations[1] as f32,
                    enrich.neighbor_elevations[2] as f32,
                ],
                elevation_diffs: [
                    enrich.elevation_diffs[0] as f32,
                    enrich.elevation_diffs[1] as f32,
                    enrich.elevation_diffs[2] as f32,
                ],
                min_angle_index: enrich.min_angle_index,
                min_elevation_diff: enrich.min_elevation_diff as f32,
                max_elevation_diff: enrich.max_elevation_diff as f32,
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

/// Fetch Baidu panoids for every mainland-China Y-junction missing one.
/// Sequential HTTP with jittered 80–150 ms pacing (~6.7–12.5 req/s, mean
/// ~115 ms) — adequate for the Shanghai pilot scale. Full-country rollout
/// will need bounded concurrency; deferred out of this PR. Out-of-China
/// rows are skipped in-process without hitting Baidu. Transport failures
/// abort immediately so operators can retry after fixing the underlying
/// Baidu/network issue; results buffered up to the previous chunk flush
/// stay in DB so the retry skips already-fetched junctions.
pub async fn import_baidu_panoid_data(pool: &PgPool, refresh: bool) -> Result<usize> {
    /// Flush every 100 fetches. Worst-case loss on transport error is
    /// CHUNK_SIZE - 1 in-flight items; matches the existing progress-log
    /// cadence so each "Progress" line corresponds to one flush boundary.
    const CHUNK_SIZE: usize = 100;

    let junctions = if refresh {
        crate::db::baidu_repository::find_all_for_refresh(pool).await?
    } else {
        crate::db::baidu_repository::find_without_baidu_panoid(pool).await?
    };

    let china_junctions: Vec<_> = junctions
        .into_iter()
        .filter(|j| china::is_in_china_mainland(j.lon, j.lat))
        .collect();

    tracing::info!(
        "Found {} mainland-China Y-junctions to query Baidu",
        china_junctions.len()
    );

    let client = baidu::build_client()?;
    let mut updates: Vec<(i64, crate::domain::china::BaiduPanorama)> =
        Vec::with_capacity(CHUNK_SIZE);
    let mut missed_osm_node_ids: Vec<i64> = Vec::with_capacity(CHUNK_SIZE);
    let mut total_updated: usize = 0;
    let mut total_tombstoned: usize = 0;

    for (idx, junction) in china_junctions.iter().enumerate() {
        if idx > 0 {
            baidu::pace_next_request().await;
        }

        match baidu::fetch_nearest_panorama(&client, junction.lon, junction.lat).await {
            Ok(Some(pano)) => updates.push((junction.osm_node_id, pano)),
            Ok(None) => missed_osm_node_ids.push(junction.osm_node_id),
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Baidu qsdata failed for junction osm_node_id={} ({}, {}): {}",
                    junction.osm_node_id,
                    junction.lat,
                    junction.lon,
                    e
                ));
            }
        }

        if updates.len() + missed_osm_node_ids.len() >= CHUNK_SIZE {
            flush_baidu_chunk(
                pool,
                &mut updates,
                &mut missed_osm_node_ids,
                &mut total_updated,
                &mut total_tombstoned,
            )
            .await?;

            tracing::info!(
                "Progress: {}/{} (flushed: ok={}, none={})",
                idx + 1,
                china_junctions.len(),
                total_updated,
                total_tombstoned
            );
        }
    }

    flush_baidu_chunk(
        pool,
        &mut updates,
        &mut missed_osm_node_ids,
        &mut total_updated,
        &mut total_tombstoned,
    )
    .await?;

    tracing::info!(
        "Baidu panoid fetch complete: total={}, ok={}, none={}",
        china_junctions.len(),
        total_updated,
        total_tombstoned
    );

    Ok(total_updated)
}

/// Persist the current buffer of fetched panoids and tombstones, then clear
/// the buffers. Called every CHUNK_SIZE fetches and once more at the end of
/// the loop for the trailing partial chunk. No-op when both buffers are
/// empty (e.g. when the run length is an exact multiple of CHUNK_SIZE).
async fn flush_baidu_chunk(
    pool: &PgPool,
    updates: &mut Vec<(i64, crate::domain::china::BaiduPanorama)>,
    missed_osm_node_ids: &mut Vec<i64>,
    total_updated: &mut usize,
    total_tombstoned: &mut usize,
) -> Result<()> {
    if updates.is_empty() && missed_osm_node_ids.is_empty() {
        return Ok(());
    }

    let updated = crate::db::baidu_repository::bulk_update_baidu(pool, updates).await?;
    let tombstoned =
        crate::db::baidu_repository::bulk_mark_queried(pool, missed_osm_node_ids).await?;

    *total_updated += updated;
    *total_tombstoned += tombstoned;

    updates.clear();
    missed_osm_node_ids.clear();

    Ok(())
}

/// Query Google Street View coverage for every non-China Y-junction that has
/// no answer cached yet, and store the result in `google_streetview_coverage`.
/// `refresh` also re-queries nodes previously recorded as uncovered (Google
/// adds imagery over time, so `false` is not permanent).
///
/// Mainland-China junctions are skipped in-process — they use Baidu panoramas
/// and Google has no coverage there. Metadata requests are free, so the cost
/// of a full backfill is time, not money. Any query failure aborts the batch
/// (rather than recording "no coverage") so a broken key or network outage
/// cannot mass-delete junctions from the map; results already flushed stay in
/// the DB, so a re-run resumes where it stopped.
pub async fn import_google_coverage_data(pool: &PgPool, refresh: bool) -> Result<usize> {
    /// Flush every 100 lookups, matching the progress-log cadence: worst-case
    /// loss on abort is the in-flight partial chunk.
    const CHUNK_SIZE: usize = 100;

    // Check the key before the full-table scan: a worktree whose generated
    // .env has no GOOGLE_MAPS_API_KEY should fail immediately, not after
    // scanning every junction.
    let api_key = google::api_key_from_env()?;
    let client = google::build_client()?;

    let candidates = crate::db::google_repository::find_uncovered_nodes(pool, refresh).await?;

    let targets: Vec<_> = candidates
        .into_iter()
        .filter(|c| !china::is_in_china_mainland(c.lon, c.lat))
        .collect();

    tracing::info!(
        "Found {} non-China Y-junctions to query for Street View coverage",
        targets.len()
    );

    let mut buffer: Vec<(i64, bool)> = Vec::with_capacity(CHUNK_SIZE);
    let mut total_written: usize = 0;
    let mut total_covered: usize = 0;

    for (idx, target) in targets.iter().enumerate() {
        if idx > 0 {
            google::pace_next_request().await;
        }

        match google::fetch_coverage(&client, &api_key, target.lon, target.lat).await {
            Ok(has_coverage) => {
                if has_coverage {
                    total_covered += 1;
                }
                buffer.push((target.osm_node_id, has_coverage));
            }
            Err(e) => {
                // Persist what already succeeded before giving up: otherwise a
                // failure that recurs within the first CHUNK_SIZE lookups makes
                // every re-run re-query the same nodes and die again, never
                // banking any progress.
                crate::db::google_repository::upsert_coverage(pool, &buffer).await?;
                return Err(anyhow::anyhow!(
                    "Street View metadata failed for junction osm_node_id={} ({}, {}): {:#}",
                    target.osm_node_id,
                    target.lat,
                    target.lon,
                    e
                ));
            }
        }

        if buffer.len() >= CHUNK_SIZE {
            total_written += crate::db::google_repository::upsert_coverage(pool, &buffer).await?;
            buffer.clear();

            tracing::info!(
                "Progress: {}/{} (written={}, covered={})",
                idx + 1,
                targets.len(),
                total_written,
                total_covered
            );
        }
    }

    total_written += crate::db::google_repository::upsert_coverage(pool, &buffer).await?;

    tracing::info!(
        "Street View coverage lookup complete: total={}, covered={}, uncovered={}",
        targets.len(),
        total_covered,
        targets.len() - total_covered
    );

    Ok(total_written)
}

#[cfg(test)]
mod compute_elevation_enrichment_tests {
    use super::*;

    /// Fixture covers (35.0–35.01, 138.0–138.01). compute_elevation_enrichment
    /// against that coordinate exercises both the junction-lookup and the
    /// 3-neighbor lookups in one call.
    #[test]
    fn returns_enrichment_for_fixture_coordinate() {
        let provider = elevation::ElevationProvider::new("tests/fixtures/gsi").unwrap();
        let bearings = [0.0_f64, 120.0, 240.0]; // 3 spread directions
        let angles = [35_i16, 145, 180];

        let result = compute_elevation_enrichment(&provider, 35.005, 138.005, &bearings, &angles);

        let enrich = result.expect("fixture coordinate should yield enrichment");
        assert!(
            (100.0..=150.0).contains(&enrich.elevation),
            "fixture elevation should fall in 100-150m range, got {}",
            enrich.elevation
        );
        // All three neighbor lookups succeeded
        for ne in enrich.neighbor_elevations {
            assert!(
                (100.0..=150.0).contains(&ne),
                "neighbor elevation out of expected range: {}",
                ne
            );
        }
    }

    #[test]
    fn returns_none_for_out_of_coverage_coordinate() {
        let provider = elevation::ElevationProvider::new("tests/fixtures/gsi").unwrap();
        let bearings = [0.0_f64, 120.0, 240.0];
        let angles = [35_i16, 145, 180];

        // Beijing — far outside the fixture mesh (which is around 35N 138E)
        let result = compute_elevation_enrichment(&provider, 39.9, 116.4, &bearings, &angles);

        assert!(
            result.is_none(),
            "Out-of-coverage coordinate should return None, got Some"
        );
    }
}
