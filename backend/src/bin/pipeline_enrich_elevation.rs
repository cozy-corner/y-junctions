//! Enrich-stage Cloud Run Job (issue #257).
//!
//! Reads extracted-stage junctions (3-way or 2-way), computes DEM-based
//! elevation enrichment per junction using the shared
//! [`y_junction_backend::importer::compute_elevation_enrichment`] helper,
//! and writes an `osm_node_id`-keyed side parquet to the enriched stage.
//! Rows whose calculation fails (no DEM coverage, partial mesh, etc.) are
//! simply absent from the output — `prepare-serving` reconstructs the
//! "no enrichment" state via LEFT JOIN.
//!
//! DEM XML files are accessed through a Cloud Run gen2 GCS volume mount
//! at the path supplied via `--dem-dir`; the underlying yj-raw bucket
//! uses Coldline storage with gzip-compressed XML.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use clap::Parser;
use rayon::prelude::*;

use y_junction_backend::importer::compute_elevation_enrichment;
use y_junction_backend::importer::elevation::ElevationProvider;
use y_junction_backend::pipeline::parquet_io::{
    read_parquet_bytes, write_parquet_bytes, ElevationParquetRecord, JunctionParquetRecord,
};
use y_junction_backend::pipeline::storage::{read_uri, write_uri};

#[derive(Parser, Debug)]
#[command(name = "pipeline-enrich-elevation")]
#[command(
    about = "Compute DEM-based elevation enrichment for extracted Y-junctions and write a side parquet keyed by osm_node_id"
)]
struct Args {
    /// Extracted-stage Parquet URI — gs://...-yj-extracted/... or file://...
    #[arg(long)]
    input: String,

    /// Enriched-stage output Parquet URI — gs://...-yj-enriched/elevations/... or file://...
    #[arg(long)]
    output: String,

    /// Directory containing GSI DEM XML files. Two layouts are supported:
    ///   - **direct**: `{dir}/*.xml{,.gz}` or `{dir}/xml/*.xml{,.gz}` —
    ///     used in local dev / tests
    ///   - **prefix-versioned**: `{dir}/{YYYYMMDD}/*.xml{,.gz}` — used by
    ///     the Cloud Run gen2 GCS FUSE mount; the binary picks the
    ///     lexicographically maximum subdirectory
    #[arg(long)]
    dem_dir: String,
}

/// Resolve `--dem-dir` to the directory the ElevationProvider should read.
///
/// Prefers a date-versioned subdirectory `{YYYYMMDD}/` when one is present,
/// even if the base also has stray XML files or a legacy `xml/` subdir —
/// the Cloud Run mount of yj-raw should never have direct XMLs at the
/// mount root, so any direct match there is almost certainly a stray
/// (operator one-off, partial cleanup) that would otherwise preempt the
/// real DEM snapshot.
///
/// Only subdirs matching `^\d{8}$` are eligible to prevent non-date names
/// (e.g. `staging/`, `tmp/`) from being lex-sorted ahead of real
/// `YYYYMMDD/` entries — ASCII digits (0x30-0x39) sort lower than
/// lowercase letters (0x61-0x7A), so a subdir like `staging/` would have
/// silently won under a naive lex-max selection.
fn resolve_dem_dir(base: &str) -> Result<PathBuf> {
    let base_path = Path::new(base);

    // First: pick lex-max YYYYMMDD subdir if any exists.
    let mut date_subdirs: Vec<PathBuf> = std::fs::read_dir(base_path)
        .with_context(|| format!("Failed to read DEM base dir: {}", base))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(is_yyyymmdd)
                .unwrap_or(false)
        })
        .collect();
    date_subdirs.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    if let Some(latest) = date_subdirs.pop() {
        return Ok(latest);
    }

    // Fallback for local-dev / tests: direct layout with XMLs at base level
    // or under a legacy `xml/` subdir.
    let direct_patterns = [
        format!("{}/*.xml", base),
        format!("{}/*.xml.gz", base),
        format!("{}/xml/*.xml", base),
        format!("{}/xml/*.xml.gz", base),
    ];
    for p in &direct_patterns {
        if glob::glob(p)?.filter_map(|e| e.ok()).next().is_some() {
            return Ok(base_path.to_path_buf());
        }
    }

    Err(anyhow!(
        "No DEM date-versioned subdir (YYYYMMDD/) nor direct XML files found under {}",
        base
    ))
}

fn is_yyyymmdd(s: &str) -> bool {
    s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args = Args::parse();
    tracing::info!("enrich-elevation: {} -> {}", args.input, args.output);
    tracing::info!("DEM base dir: {}", args.dem_dir);

    let resolved = resolve_dem_dir(&args.dem_dir)?;
    tracing::info!("Resolved DEM dir: {}", resolved.display());

    let provider = ElevationProvider::new(
        resolved
            .to_str()
            .ok_or_else(|| anyhow!("Non-UTF8 DEM path: {:?}", resolved))?,
    )
    .with_context(|| format!("Failed to initialize ElevationProvider from {:?}", resolved))?;

    let bytes = read_uri(&args.input).await?;
    tracing::info!("Read {} bytes of extracted Parquet", bytes.len());
    let extracted: Vec<JunctionParquetRecord> = read_parquet_bytes(bytes)?;
    tracing::info!("Decoded {} extracted records", extracted.len());

    // Compute enrichment per junction in parallel; out-of-coverage rows
    // (e.g. China, or partial DEM mesh) naturally return None and are
    // dropped from the output — the row is then absent from the side
    // table, which prepare-serving renders as "no enrichment" via LEFT JOIN.
    let enriched: Vec<ElevationParquetRecord> = extracted
        .par_iter()
        .filter_map(|j| {
            let bearings = [j.bearing_1, j.bearing_2, j.bearing_3];
            let angles = [j.angle_1 as i16, j.angle_2 as i16, j.angle_3 as i16];
            let e = compute_elevation_enrichment(&provider, j.lat, j.lon, &bearings, &angles)?;
            Some(ElevationParquetRecord {
                osm_node_id: j.osm_node_id,
                elevation: e.elevation as f32,
                neighbor_elevation_1: e.neighbor_elevations[0] as f32,
                neighbor_elevation_2: e.neighbor_elevations[1] as f32,
                neighbor_elevation_3: e.neighbor_elevations[2] as f32,
                elevation_diff_1: e.elevation_diffs[0] as f32,
                elevation_diff_2: e.elevation_diffs[1] as f32,
                elevation_diff_3: e.elevation_diffs[2] as f32,
                min_angle_index: e.min_angle_index as i32,
                min_elevation_diff: e.min_elevation_diff as f32,
                max_elevation_diff: e.max_elevation_diff as f32,
            })
        })
        .collect();

    tracing::info!(
        "Computed enrichment for {} / {} junctions",
        enriched.len(),
        extracted.len()
    );

    let bytes_out = write_parquet_bytes(&enriched)?;
    tracing::info!("Encoded {} bytes of enriched Parquet", bytes_out.len());

    write_uri(&args.output, Bytes::from(bytes_out)).await?;
    tracing::info!("Wrote enriched Parquet");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yyyymmdd_format_check() {
        assert!(is_yyyymmdd("20260523"));
        assert!(is_yyyymmdd("99991231"));
        assert!(!is_yyyymmdd("2026-05-23"));
        assert!(!is_yyyymmdd("staging"));
        assert!(!is_yyyymmdd("tmp"));
        assert!(!is_yyyymmdd("2026052")); // too short
        assert!(!is_yyyymmdd("202605231")); // too long
        assert!(!is_yyyymmdd(""));
    }

    #[test]
    fn resolve_dem_dir_prefers_date_subdir_over_xml_subdir() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();

        // Stray legacy xml/ subdir with files — must NOT preempt the date subdir.
        std::fs::create_dir_all(base.join("xml")).unwrap();
        std::fs::write(base.join("xml/stray.xml"), b"<x/>").unwrap();

        // Real date-versioned subdir.
        let date_dir = base.join("20260523");
        std::fs::create_dir_all(&date_dir).unwrap();
        std::fs::write(date_dir.join("FG-GML-5238-40-00-DEM5B.xml.gz"), b"dummy").unwrap();

        let resolved = resolve_dem_dir(base.to_str().unwrap()).unwrap();
        assert_eq!(resolved, date_dir);
    }

    #[test]
    fn resolve_dem_dir_picks_lex_max_of_date_subdirs() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base.join("20260101")).unwrap();
        std::fs::create_dir_all(base.join("20260901")).unwrap();
        std::fs::create_dir_all(base.join("20260601")).unwrap();

        let resolved = resolve_dem_dir(base.to_str().unwrap()).unwrap();
        assert_eq!(resolved, base.join("20260901"));
    }

    #[test]
    fn resolve_dem_dir_skips_non_date_subdirs() {
        // The historical bug: `staging` would lex-sort AFTER `20260523`
        // because lowercase letters (0x73) > digits (0x32). Ensure non-
        // YYYYMMDD subdirs are excluded entirely.
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        std::fs::create_dir_all(base.join("20260523")).unwrap();
        std::fs::create_dir_all(base.join("staging")).unwrap();
        std::fs::create_dir_all(base.join("zzzz")).unwrap();

        let resolved = resolve_dem_dir(base.to_str().unwrap()).unwrap();
        assert_eq!(resolved, base.join("20260523"));
    }

    #[test]
    fn resolve_dem_dir_falls_back_to_xml_subdir() {
        // Local-dev layout: no date subdirs, files live under `xml/`.
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        let xml_dir = base.join("xml");
        std::fs::create_dir_all(&xml_dir).unwrap();
        std::fs::write(xml_dir.join("FG-GML-X.xml"), b"<x/>").unwrap();

        let resolved = resolve_dem_dir(base.to_str().unwrap()).unwrap();
        assert_eq!(resolved, base.to_path_buf());
    }

    #[test]
    fn resolve_dem_dir_errors_when_empty() {
        let temp = tempfile::tempdir().unwrap();
        let result = resolve_dem_dir(temp.path().to_str().unwrap());
        assert!(result.is_err(), "Empty base dir should be an error");
    }
}
