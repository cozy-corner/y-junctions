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

/// Flat record format used between pipeline stages (extract → load).
///
/// Walking-skeleton scope: elevation/baidu enrichment columns are intentionally
/// omitted because this pipeline path does not pass through enrich-elevation.
/// `JunctionForInsert` consumers reconstruct those fields as `None`.
///
/// `i32` is used for angle/index fields because Parquet has no `i16` physical
/// type. `parquet_derive::ParquetRecordReader` does not support `Option<T>`
/// scalars in the v58 release, so all surviving columns are non-nullable.
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

/// Serialize a slice of records as a single-row-group Snappy-compressed
/// Parquet file in memory.
pub fn write_parquet_bytes(records: &[JunctionParquetRecord]) -> Result<Vec<u8>> {
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

/// Deserialize a Parquet file (any number of row groups) from an in-memory buffer.
pub fn read_parquet_bytes(bytes: Bytes) -> Result<Vec<JunctionParquetRecord>> {
    let reader = SerializedFileReader::new(bytes)?;
    let metadata = reader.metadata();
    let mut records: Vec<JunctionParquetRecord> = Vec::new();

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

        let restored = read_parquet_bytes(Bytes::from(bytes)).unwrap();
        assert_eq!(restored.len(), originals.len());
        for (a, b) in originals.iter().zip(restored.iter()) {
            assert_eq!(a.osm_node_id, b.osm_node_id);
            assert_eq!(a.angle_1, b.angle_1);
            assert_eq!(a.bearing_1, b.bearing_1);
            assert_eq!(a.way_1_highway_type, b.way_1_highway_type);
            assert_eq!(a.way_1_bridge, b.way_1_bridge);
        }
    }
}
