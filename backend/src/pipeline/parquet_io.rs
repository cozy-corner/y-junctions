use std::sync::Arc;

use anyhow::Result;
use bytes::Bytes;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use parquet::file::reader::FileReader;
use parquet::file::reader::SerializedFileReader;
use parquet::file::writer::SerializedFileWriter;
use parquet::record::{RecordReader, RecordWriter};
use parquet_derive::{ParquetRecordReader, ParquetRecordWriter};

use crate::importer::detector::JunctionForInsert;

/// Sentinel value for "elevation field is absent" in the serving Parquet
/// stage. `parquet_derive` v58 does not support `Option<T>` scalars, so
/// nullability is encoded by this sentinel and unwrapped to `None` at the
/// load-to-cockroach boundary. Matches the existing GSI DEM convention
/// (`backend/src/importer/elevation.rs:161,177` — `-9999` is GSI's own
/// "no data" marker, so reusing the value avoids a second sentinel space).
pub const ELEVATION_SENTINEL: f32 = -9999.0;

/// Extracted-stage record. PBF-derived geometry + road metadata, no
/// enrichment columns. Lives in `gs://...-yj-extracted/{three-way,two-way}/`.
///
/// `i32` is used for angle/index fields because Parquet has no `i16` physical
/// type. `parquet_derive::ParquetRecordReader` does not support `Option<T>`
/// scalars in the v58 release, so all columns are non-nullable.
#[derive(Debug, Clone, ParquetRecordWriter, ParquetRecordReader)]
pub struct JunctionParquetRecord {
    pub osm_node_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub angle_1: i32,
    pub angle_2: i32,
    pub angle_3: i32,
    pub bearing_1: f64,
    pub bearing_2: f64,
    pub bearing_3: f64,
    pub way_1_bridge: bool,
    pub way_1_tunnel: bool,
    pub way_2_bridge: bool,
    pub way_2_tunnel: bool,
    pub way_3_bridge: bool,
    pub way_3_tunnel: bool,
    pub way_1_highway_type: String,
    pub way_2_highway_type: String,
    pub way_3_highway_type: String,
}

impl From<JunctionForInsert> for JunctionParquetRecord {
    fn from(j: JunctionForInsert) -> Self {
        Self {
            osm_node_id: j.osm_node_id,
            lat: j.lat,
            lon: j.lon,
            angle_1: j.angle_1 as i32,
            angle_2: j.angle_2 as i32,
            angle_3: j.angle_3 as i32,
            bearing_1: j.bearings[0],
            bearing_2: j.bearings[1],
            bearing_3: j.bearings[2],
            way_1_bridge: j.way_1_bridge,
            way_1_tunnel: j.way_1_tunnel,
            way_2_bridge: j.way_2_bridge,
            way_2_tunnel: j.way_2_tunnel,
            way_3_bridge: j.way_3_bridge,
            way_3_tunnel: j.way_3_tunnel,
            way_1_highway_type: j.way_1_highway_type,
            way_2_highway_type: j.way_2_highway_type,
            way_3_highway_type: j.way_3_highway_type,
        }
    }
}

impl From<JunctionParquetRecord> for JunctionForInsert {
    fn from(r: JunctionParquetRecord) -> Self {
        Self {
            osm_node_id: r.osm_node_id,
            lat: r.lat,
            lon: r.lon,
            angle_1: r.angle_1 as i16,
            angle_2: r.angle_2 as i16,
            angle_3: r.angle_3 as i16,
            bearings: [r.bearing_1, r.bearing_2, r.bearing_3],
            elevation: None,
            neighbor_elevations: None,
            elevation_diffs: None,
            min_angle_index: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
            way_1_bridge: r.way_1_bridge,
            way_1_tunnel: r.way_1_tunnel,
            way_2_bridge: r.way_2_bridge,
            way_2_tunnel: r.way_2_tunnel,
            way_3_bridge: r.way_3_bridge,
            way_3_tunnel: r.way_3_tunnel,
            way_1_highway_type: r.way_1_highway_type,
            way_2_highway_type: r.way_2_highway_type,
            way_3_highway_type: r.way_3_highway_type,
        }
    }
}

/// Enrich-stage side table: DEM-derived elevation columns keyed by
/// `osm_node_id`. Lives in `gs://...-yj-enriched/elevations/{three-way,two-way}/`.
/// A row is only written when `enrich-elevation` successfully computed the
/// junction's elevation AND all three neighbor elevations — so all columns are
/// non-nullable. Rows whose calculation failed (no DEM coverage, partial mesh,
/// etc.) are simply absent from the output.
#[derive(Debug, Clone, ParquetRecordWriter, ParquetRecordReader)]
pub struct ElevationParquetRecord {
    pub osm_node_id: i64,
    pub elevation: f32,
    pub neighbor_elevation_1: f32,
    pub neighbor_elevation_2: f32,
    pub neighbor_elevation_3: f32,
    pub elevation_diff_1: f32,
    pub elevation_diff_2: f32,
    pub elevation_diff_3: f32,
    pub min_angle_index: i32,
    pub min_elevation_diff: f32,
    pub max_elevation_diff: f32,
}

/// Serving-stage record: extracted columns joined with optional elevation
/// enrichment. Lives in `gs://...-yj-serving/`. Missing enrichment is
/// represented by [`ELEVATION_SENTINEL`] on elevation columns and `-1` on
/// `min_angle_index`. The load-to-cockroach `From` impl unwraps these to
/// `None`.
#[derive(Debug, Clone, ParquetRecordWriter, ParquetRecordReader)]
pub struct ServingJunctionParquetRecord {
    // --- Extracted columns (mirror JunctionParquetRecord) ---
    pub osm_node_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub angle_1: i32,
    pub angle_2: i32,
    pub angle_3: i32,
    pub bearing_1: f64,
    pub bearing_2: f64,
    pub bearing_3: f64,
    pub way_1_bridge: bool,
    pub way_1_tunnel: bool,
    pub way_2_bridge: bool,
    pub way_2_tunnel: bool,
    pub way_3_bridge: bool,
    pub way_3_tunnel: bool,
    pub way_1_highway_type: String,
    pub way_2_highway_type: String,
    pub way_3_highway_type: String,
    // --- Elevation enrichment (sentinel = absent) ---
    pub elevation: f32,
    pub neighbor_elevation_1: f32,
    pub neighbor_elevation_2: f32,
    pub neighbor_elevation_3: f32,
    pub elevation_diff_1: f32,
    pub elevation_diff_2: f32,
    pub elevation_diff_3: f32,
    pub min_angle_index: i32,
    pub min_elevation_diff: f32,
    pub max_elevation_diff: f32,
}

impl ServingJunctionParquetRecord {
    /// Construct a serving record from an extracted record only (no
    /// enrichment available). All elevation columns are filled with
    /// [`ELEVATION_SENTINEL`] / `-1` and will round-trip to `None` at load
    /// time.
    pub fn from_extracted(j: JunctionParquetRecord) -> Self {
        Self {
            osm_node_id: j.osm_node_id,
            lat: j.lat,
            lon: j.lon,
            angle_1: j.angle_1,
            angle_2: j.angle_2,
            angle_3: j.angle_3,
            bearing_1: j.bearing_1,
            bearing_2: j.bearing_2,
            bearing_3: j.bearing_3,
            way_1_bridge: j.way_1_bridge,
            way_1_tunnel: j.way_1_tunnel,
            way_2_bridge: j.way_2_bridge,
            way_2_tunnel: j.way_2_tunnel,
            way_3_bridge: j.way_3_bridge,
            way_3_tunnel: j.way_3_tunnel,
            way_1_highway_type: j.way_1_highway_type,
            way_2_highway_type: j.way_2_highway_type,
            way_3_highway_type: j.way_3_highway_type,
            elevation: ELEVATION_SENTINEL,
            neighbor_elevation_1: ELEVATION_SENTINEL,
            neighbor_elevation_2: ELEVATION_SENTINEL,
            neighbor_elevation_3: ELEVATION_SENTINEL,
            elevation_diff_1: ELEVATION_SENTINEL,
            elevation_diff_2: ELEVATION_SENTINEL,
            elevation_diff_3: ELEVATION_SENTINEL,
            min_angle_index: -1,
            min_elevation_diff: ELEVATION_SENTINEL,
            max_elevation_diff: ELEVATION_SENTINEL,
        }
    }

    /// Overlay enrichment columns onto an extracted serving record. The
    /// `osm_node_id` is asserted to match in debug builds.
    pub fn with_enrichment(mut self, e: ElevationParquetRecord) -> Self {
        debug_assert_eq!(self.osm_node_id, e.osm_node_id);
        self.elevation = e.elevation;
        self.neighbor_elevation_1 = e.neighbor_elevation_1;
        self.neighbor_elevation_2 = e.neighbor_elevation_2;
        self.neighbor_elevation_3 = e.neighbor_elevation_3;
        self.elevation_diff_1 = e.elevation_diff_1;
        self.elevation_diff_2 = e.elevation_diff_2;
        self.elevation_diff_3 = e.elevation_diff_3;
        self.min_angle_index = e.min_angle_index;
        self.min_elevation_diff = e.min_elevation_diff;
        self.max_elevation_diff = e.max_elevation_diff;
        self
    }
}

impl From<ServingJunctionParquetRecord> for JunctionForInsert {
    fn from(r: ServingJunctionParquetRecord) -> Self {
        // f32 in the Parquet column to keep serving size small; widened back
        // to f64 here because JunctionForInsert / DB columns are f64. The
        // existing import_elevation_data flow (importer/mod.rs:136-145) does
        // the same f64 → f32 truncation at the repository boundary.
        let has_enrichment = r.elevation != ELEVATION_SENTINEL;
        Self {
            osm_node_id: r.osm_node_id,
            lat: r.lat,
            lon: r.lon,
            angle_1: r.angle_1 as i16,
            angle_2: r.angle_2 as i16,
            angle_3: r.angle_3 as i16,
            bearings: [r.bearing_1, r.bearing_2, r.bearing_3],
            elevation: has_enrichment.then_some(r.elevation as f64),
            neighbor_elevations: has_enrichment.then_some([
                r.neighbor_elevation_1 as f64,
                r.neighbor_elevation_2 as f64,
                r.neighbor_elevation_3 as f64,
            ]),
            elevation_diffs: has_enrichment.then_some([
                r.elevation_diff_1 as f64,
                r.elevation_diff_2 as f64,
                r.elevation_diff_3 as f64,
            ]),
            min_angle_index: has_enrichment.then_some(r.min_angle_index as i16),
            min_elevation_diff: has_enrichment.then_some(r.min_elevation_diff as f64),
            max_elevation_diff: has_enrichment.then_some(r.max_elevation_diff as f64),
            way_1_bridge: r.way_1_bridge,
            way_1_tunnel: r.way_1_tunnel,
            way_2_bridge: r.way_2_bridge,
            way_2_tunnel: r.way_2_tunnel,
            way_3_bridge: r.way_3_bridge,
            way_3_tunnel: r.way_3_tunnel,
            way_1_highway_type: r.way_1_highway_type,
            way_2_highway_type: r.way_2_highway_type,
            way_3_highway_type: r.way_3_highway_type,
        }
    }
}

/// Serialize a slice of records as a single-row-group Snappy-compressed
/// Parquet file in memory. Generic over any type that derives
/// `ParquetRecordWriter` so the same helper covers all pipeline-stage
/// record types ([`JunctionParquetRecord`], [`ElevationParquetRecord`],
/// [`ServingJunctionParquetRecord`]).
pub fn write_parquet_bytes<T>(records: &[T]) -> Result<Vec<u8>>
where
    for<'a> &'a [T]: RecordWriter<T>,
{
    let schema = records.schema()?;
    let props = Arc::new(
        WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .build(),
    );

    let mut buffer: Vec<u8> = Vec::new();
    let mut writer = SerializedFileWriter::new(&mut buffer, schema, props)?;
    let mut row_group = writer.next_row_group()?;
    records.write_to_row_group(&mut row_group)?;
    row_group.close()?;
    writer.close()?;

    Ok(buffer)
}

/// Deserialize a Parquet file (any number of row groups) from an in-memory
/// buffer. Generic over any type that derives `ParquetRecordReader`.
pub fn read_parquet_bytes<T>(bytes: Bytes) -> Result<Vec<T>>
where
    Vec<T>: RecordReader<T>,
{
    let reader = SerializedFileReader::new(bytes)?;
    let metadata = reader.metadata();
    let mut records: Vec<T> = Vec::new();

    for i in 0..metadata.num_row_groups() {
        let num_rows = metadata.row_group(i).num_rows() as usize;
        let mut row_group_reader = reader.get_row_group(i)?;
        records.read_from_row_group(&mut *row_group_reader, num_rows)?;
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> JunctionForInsert {
        JunctionForInsert {
            osm_node_id: 42,
            lat: 34.5,
            lon: 134.1,
            angle_1: 35,
            angle_2: 145,
            angle_3: 180,
            bearings: [10.0, 130.0, 250.0],
            elevation: None,
            neighbor_elevations: None,
            elevation_diffs: None,
            min_angle_index: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
            way_1_bridge: false,
            way_1_tunnel: true,
            way_2_bridge: true,
            way_2_tunnel: false,
            way_3_bridge: false,
            way_3_tunnel: false,
            way_1_highway_type: "primary".into(),
            way_2_highway_type: "secondary".into(),
            way_3_highway_type: "residential".into(),
        }
    }

    #[test]
    fn roundtrip_preserves_core_fields() {
        let original = sample();
        let record: JunctionParquetRecord = original.clone().into();
        let restored: JunctionForInsert = record.into();

        assert_eq!(restored.osm_node_id, original.osm_node_id);
        assert_eq!(restored.lat, original.lat);
        assert_eq!(restored.lon, original.lon);
        assert_eq!(restored.angle_1, original.angle_1);
        assert_eq!(restored.angle_2, original.angle_2);
        assert_eq!(restored.angle_3, original.angle_3);
        assert_eq!(restored.bearings, original.bearings);
        assert_eq!(restored.way_1_bridge, original.way_1_bridge);
        assert_eq!(restored.way_2_tunnel, original.way_2_tunnel);
        assert_eq!(restored.way_1_highway_type, original.way_1_highway_type);
        assert_eq!(restored.way_3_highway_type, original.way_3_highway_type);
    }

    #[test]
    fn extract_drops_enrichment_fields() {
        let record: JunctionParquetRecord = sample().into();
        let restored: JunctionForInsert = record.into();

        assert!(restored.elevation.is_none());
        assert!(restored.neighbor_elevations.is_none());
        assert!(restored.elevation_diffs.is_none());
        assert!(restored.min_angle_index.is_none());
        assert!(restored.min_elevation_diff.is_none());
        assert!(restored.max_elevation_diff.is_none());
    }

    #[test]
    fn parquet_roundtrip() {
        let originals: Vec<JunctionParquetRecord> = (0..10)
            .map(|i| {
                let mut j = sample();
                j.osm_node_id = i;
                j.into()
            })
            .collect();

        let bytes = write_parquet_bytes(&originals).unwrap();
        assert!(!bytes.is_empty());

        let restored: Vec<JunctionParquetRecord> = read_parquet_bytes(Bytes::from(bytes)).unwrap();
        assert_eq!(restored.len(), originals.len());
        for (a, b) in originals.iter().zip(restored.iter()) {
            assert_eq!(a.osm_node_id, b.osm_node_id);
            assert_eq!(a.angle_1, b.angle_1);
            assert_eq!(a.bearing_1, b.bearing_1);
            assert_eq!(a.way_1_highway_type, b.way_1_highway_type);
            assert_eq!(a.way_1_bridge, b.way_1_bridge);
        }
    }

    #[test]
    fn merge_two_inputs_roundtrip() {
        let three_way: Vec<JunctionParquetRecord> = (0..5)
            .map(|i| {
                let mut j = sample();
                j.osm_node_id = 1000 + i;
                j.into()
            })
            .collect();
        let two_way: Vec<JunctionParquetRecord> = (0..7)
            .map(|i| {
                let mut j = sample();
                j.osm_node_id = 2000 + i;
                j.into()
            })
            .collect();

        let three_way_bytes = write_parquet_bytes(&three_way).unwrap();
        let two_way_bytes = write_parquet_bytes(&two_way).unwrap();

        let mut merged: Vec<JunctionParquetRecord> =
            read_parquet_bytes(Bytes::from(three_way_bytes)).unwrap();
        merged.extend(
            read_parquet_bytes::<JunctionParquetRecord>(Bytes::from(two_way_bytes)).unwrap(),
        );

        let serving_bytes = write_parquet_bytes(&merged).unwrap();
        let restored: Vec<JunctionParquetRecord> =
            read_parquet_bytes(Bytes::from(serving_bytes)).unwrap();

        assert_eq!(restored.len(), 12);
        let ids: Vec<i64> = restored.iter().map(|r| r.osm_node_id).collect();
        assert!((1000..1005).all(|i| ids.contains(&i)));
        assert!((2000..2007).all(|i| ids.contains(&i)));
    }

    // ---- ElevationParquetRecord / ServingJunctionParquetRecord (issue #257) ----

    fn sample_elevation(osm_node_id: i64) -> ElevationParquetRecord {
        ElevationParquetRecord {
            osm_node_id,
            elevation: 123.5,
            neighbor_elevation_1: 122.0,
            neighbor_elevation_2: 124.5,
            neighbor_elevation_3: 121.0,
            elevation_diff_1: -1.5,
            elevation_diff_2: 1.0,
            elevation_diff_3: -2.5,
            min_angle_index: 0,
            min_elevation_diff: -2.5,
            max_elevation_diff: 1.0,
        }
    }

    #[test]
    fn elevation_record_roundtrip() {
        let originals: Vec<ElevationParquetRecord> =
            (0..5).map(|i| sample_elevation(1000 + i)).collect();
        let bytes = write_parquet_bytes(&originals).unwrap();
        let restored: Vec<ElevationParquetRecord> = read_parquet_bytes(Bytes::from(bytes)).unwrap();

        assert_eq!(restored.len(), originals.len());
        for (a, b) in originals.iter().zip(restored.iter()) {
            assert_eq!(a.osm_node_id, b.osm_node_id);
            assert_eq!(a.elevation, b.elevation);
            assert_eq!(a.neighbor_elevation_2, b.neighbor_elevation_2);
            assert_eq!(a.min_angle_index, b.min_angle_index);
            assert_eq!(a.max_elevation_diff, b.max_elevation_diff);
        }
    }

    #[test]
    fn serving_from_extracted_uses_sentinel() {
        let extracted: JunctionParquetRecord = sample().into();
        let serving = ServingJunctionParquetRecord::from_extracted(extracted);

        // Extracted columns preserved
        assert_eq!(serving.osm_node_id, 42);
        assert_eq!(serving.angle_1, 35);
        assert_eq!(serving.way_1_highway_type, "primary");

        // Enrichment columns all sentinel
        assert_eq!(serving.elevation, ELEVATION_SENTINEL);
        assert_eq!(serving.neighbor_elevation_1, ELEVATION_SENTINEL);
        assert_eq!(serving.neighbor_elevation_3, ELEVATION_SENTINEL);
        assert_eq!(serving.elevation_diff_2, ELEVATION_SENTINEL);
        assert_eq!(serving.min_angle_index, -1);
        assert_eq!(serving.min_elevation_diff, ELEVATION_SENTINEL);
        assert_eq!(serving.max_elevation_diff, ELEVATION_SENTINEL);
    }

    #[test]
    fn serving_with_enrichment_overlays_elevation() {
        let extracted: JunctionParquetRecord = sample().into();
        let base = ServingJunctionParquetRecord::from_extracted(extracted);
        let enriched = base.with_enrichment(sample_elevation(42));

        // Extracted columns still preserved
        assert_eq!(enriched.osm_node_id, 42);
        assert_eq!(enriched.angle_1, 35);
        assert_eq!(enriched.way_1_highway_type, "primary");

        // Enrichment columns now populated
        assert_eq!(enriched.elevation, 123.5);
        assert_eq!(enriched.neighbor_elevation_1, 122.0);
        assert_eq!(enriched.elevation_diff_1, -1.5);
        assert_eq!(enriched.min_angle_index, 0);
        assert_eq!(enriched.min_elevation_diff, -2.5);
        assert_eq!(enriched.max_elevation_diff, 1.0);
    }

    #[test]
    fn serving_to_junction_for_insert_sentinel_to_none() {
        // Sentinel-only row should map to all-None enrichment fields.
        let extracted: JunctionParquetRecord = sample().into();
        let serving = ServingJunctionParquetRecord::from_extracted(extracted);
        let restored: JunctionForInsert = serving.into();

        assert_eq!(restored.osm_node_id, 42);
        assert!(restored.elevation.is_none());
        assert!(restored.neighbor_elevations.is_none());
        assert!(restored.elevation_diffs.is_none());
        assert!(restored.min_angle_index.is_none());
        assert!(restored.min_elevation_diff.is_none());
        assert!(restored.max_elevation_diff.is_none());
    }

    #[test]
    fn serving_to_junction_for_insert_unwraps_enrichment() {
        let extracted: JunctionParquetRecord = sample().into();
        let serving = ServingJunctionParquetRecord::from_extracted(extracted)
            .with_enrichment(sample_elevation(42));
        let restored: JunctionForInsert = serving.into();

        assert_eq!(restored.osm_node_id, 42);
        assert_eq!(restored.elevation, Some(123.5));
        assert_eq!(restored.neighbor_elevations, Some([122.0, 124.5, 121.0]));
        assert_eq!(restored.elevation_diffs, Some([-1.5, 1.0, -2.5]));
        assert_eq!(restored.min_angle_index, Some(0));
        assert_eq!(restored.min_elevation_diff, Some(-2.5));
        assert_eq!(restored.max_elevation_diff, Some(1.0));
    }

    #[test]
    fn serving_record_roundtrip_mixed() {
        // Two extracted records, only one with enrichment. Roundtrip through
        // Parquet preserves sentinel/non-sentinel distinction → load-side
        // From impl recovers None/Some correctly.
        let extracted_1: JunctionParquetRecord = sample().into();
        let mut extracted_2: JunctionParquetRecord = sample().into();
        extracted_2.osm_node_id = 99;

        let serving_1 = ServingJunctionParquetRecord::from_extracted(extracted_1)
            .with_enrichment(sample_elevation(42));
        let serving_2 = ServingJunctionParquetRecord::from_extracted(extracted_2);

        let bytes = write_parquet_bytes(&[serving_1, serving_2]).unwrap();
        let restored: Vec<ServingJunctionParquetRecord> =
            read_parquet_bytes(Bytes::from(bytes)).unwrap();
        assert_eq!(restored.len(), 2);

        let inserts: Vec<JunctionForInsert> = restored.into_iter().map(Into::into).collect();
        assert_eq!(inserts[0].osm_node_id, 42);
        assert_eq!(inserts[0].elevation, Some(123.5));
        assert_eq!(inserts[1].osm_node_id, 99);
        assert!(inserts[1].elevation.is_none());
    }
}
