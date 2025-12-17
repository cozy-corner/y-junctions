use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs::File;

use super::calculator::calculate_junction_angles;
use super::detector::{JunctionForInsert, NodeConnectionCounter, YJunctionWithCoords};
use super::elevation::ElevationProvider;
use crate::domain::junction::AngleType;

pub fn parse_pbf(
    input_path: &str,
    elevation_dir: Option<&str>,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<Vec<JunctionForInsert>> {
    tracing::info!(
        "Parsing PBF file with bbox: ({}, {}) to ({}, {})",
        min_lon,
        min_lat,
        max_lon,
        max_lat
    );

    // 1st pass: Count way connections per node
    tracing::info!("Starting 1st pass: collecting highway ways and counting node connections");
    let mut counter = NodeConnectionCounter::new();
    let mut way_count = 0;
    let mut highway_way_count = 0;

    let file = File::open(input_path)?;
    let reader = osmpbf::ElementReader::new(file);

    reader.for_each(|element| {
        if let osmpbf::Element::Way(way) = element {
            way_count += 1;

            // Check if this way has a highway tag
            if let Some(highway_type) = way.tags().find(|&(k, _)| k == "highway").map(|(_, v)| v) {
                // Check if it's a valid highway type for Y-junction detection
                if counter.is_valid_highway_type(highway_type) {
                    highway_way_count += 1;

                    // Collect node IDs from this way
                    let node_ids: Vec<i64> = way.refs().collect();

                    // Add this way and its nodes to the counter
                    counter.add_way(way.id(), &node_ids, highway_type);
                }
            }
        }
    })?;

    tracing::info!("1st pass complete:");
    tracing::info!("  Total ways processed: {}", way_count);
    tracing::info!("  Highway ways found: {}", highway_way_count);
    tracing::info!(
        "  Unique nodes in highway network: {}",
        counter.node_count()
    );

    // Find Y-junction candidates (nodes with exactly 3 way connections)
    let candidates = counter.find_y_junction_candidates();
    tracing::info!("Found {} Y-junction candidates", candidates.len());

    if candidates.is_empty() {
        tracing::warn!("No Y-junction candidates found");
        return Ok(Vec::new());
    }

    // 2nd pass: Retrieve coordinates for Y-junction candidates
    tracing::info!("Starting 2nd pass: retrieving node coordinates");

    // Create a HashSet of candidate node IDs for fast lookup
    let candidate_node_ids: HashSet<i64> = candidates.iter().map(|c| c.node_id).collect();

    // Map to store node coordinates
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    let file = File::open(input_path)?;
    let reader = osmpbf::ElementReader::new(file);

    reader.for_each(|element| {
        match element {
            osmpbf::Element::Node(node) => {
                let node_id = node.id();

                // Check if this node is a Y-junction candidate
                if candidate_node_ids.contains(&node_id) {
                    let lat = node.lat();
                    let lon = node.lon();

                    // Check if node is within bounding box
                    if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
                        node_coords.insert(node_id, (lat, lon));
                    }
                }
            }
            osmpbf::Element::DenseNode(node) => {
                let node_id = node.id();

                // Check if this node is a Y-junction candidate
                if candidate_node_ids.contains(&node_id) {
                    let lat = node.lat();
                    let lon = node.lon();

                    // Check if node is within bounding box
                    if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
                        node_coords.insert(node_id, (lat, lon));
                    }
                }
            }
            _ => {}
        }
    })?;

    tracing::info!("2nd pass complete:");
    tracing::info!("  Coordinates retrieved: {}", node_coords.len());

    // Combine candidates with their coordinates
    let y_junctions: Vec<YJunctionWithCoords> = candidates
        .iter()
        .filter_map(|candidate| {
            node_coords
                .get(&candidate.node_id)
                .map(|&(lat, lon)| YJunctionWithCoords {
                    node_id: candidate.node_id,
                    lat,
                    lon,
                    connected_ways: candidate.connected_ways.clone(),
                })
        })
        .collect();

    tracing::info!(
        "Found {} Y-junction candidates (within bbox)",
        y_junctions.len()
    );

    // 3rd pass: Get coordinates of neighboring nodes and calculate angles
    tracing::info!("Starting 3rd pass: calculating angles for Y-junctions");

    // Initialize elevation provider if directory is provided
    let mut elevation_provider = elevation_dir.map(ElevationProvider::new);
    let mut elevation_stats = ElevationStats::new();

    // Collect all neighboring node IDs
    let mut all_neighbor_ids = HashSet::new();
    for junction in &y_junctions {
        let neighbor_ids = counter.get_neighboring_nodes(junction.node_id);
        for id in neighbor_ids {
            all_neighbor_ids.insert(id);
        }
    }

    tracing::info!(
        "Need coordinates for {} neighboring nodes",
        all_neighbor_ids.len()
    );

    // Get coordinates for neighboring nodes
    let mut neighbor_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    let file = File::open(input_path)?;
    let reader = osmpbf::ElementReader::new(file);

    reader.for_each(|element| match element {
        osmpbf::Element::Node(node) => {
            let node_id = node.id();
            if all_neighbor_ids.contains(&node_id) {
                neighbor_coords.insert(node_id, (node.lat(), node.lon()));
            }
        }
        osmpbf::Element::DenseNode(node) => {
            let node_id = node.id();
            if all_neighbor_ids.contains(&node_id) {
                neighbor_coords.insert(node_id, (node.lat(), node.lon()));
            }
        }
        _ => {}
    })?;

    tracing::info!(
        "3rd pass complete: retrieved {} neighbor coordinates",
        neighbor_coords.len()
    );

    // Calculate angles for each Y-junction and create JunctionForInsert records
    let mut junctions_for_insert = Vec::new();
    let mut successful_calculations = 0;
    let mut failed_calculations = 0;

    for junction in &y_junctions {
        let neighbor_ids = counter.get_neighboring_nodes(junction.node_id);

        if neighbor_ids.len() != 3 {
            failed_calculations += 1;
            continue;
        }

        // Get coordinates for all 3 neighboring nodes
        let neighbor_points: Vec<(f64, f64)> = neighbor_ids
            .iter()
            .filter_map(|&id| neighbor_coords.get(&id).copied())
            .collect();

        if neighbor_points.len() != 3 {
            failed_calculations += 1;
            continue;
        }

        // Calculate angles and bearings
        if let Some((angles, bearings)) =
            calculate_junction_angles(junction.lat, junction.lon, &neighbor_points)
        {
            // Find minimum angle for filtering and type classification
            let min_angle = *angles.iter().min().unwrap();
            let mut sorted_angles = angles;
            sorted_angles.sort_unstable();
            let angle_type =
                AngleType::from_angles(sorted_angles[0], sorted_angles[1], sorted_angles[2]);

            // Log first 10 junctions for verification
            if junctions_for_insert.len() < 10 {
                tracing::info!(
                    "Node {}: [{}\u{00b0}, {}\u{00b0}, {}\u{00b0}] type={:?}, bearings=[{:.1}\u{00b0}, {:.1}\u{00b0}, {:.1}\u{00b0}]",
                    junction.node_id,
                    angles[0],
                    angles[1],
                    angles[2],
                    angle_type,
                    bearings[0],
                    bearings[1],
                    bearings[2]
                );
            }

            // 最小角度が60度以上の場合はT字路とみなして除外
            if min_angle >= 60 {
                continue;
            }

            successful_calculations += 1;

            // Get elevation data
            let elev_data = get_elevation_data(
                &mut elevation_provider,
                junction.lat,
                junction.lon,
                &neighbor_points,
                &angles,
                &mut elevation_stats,
            );

            // Create JunctionForInsert
            junctions_for_insert.push(JunctionForInsert {
                osm_node_id: junction.node_id,
                lat: junction.lat,
                lon: junction.lon,
                angle_1: angles[0],
                angle_2: angles[1],
                angle_3: angles[2],
                bearings,
                elevation: elev_data.elevation,
                neighbor_elevations: elev_data.neighbor_elevations,
                elevation_diffs: elev_data.elevation_diffs,
                min_angle_index: elev_data.min_angle_index,
                min_elevation_diff: elev_data.min_elevation_diff,
                max_elevation_diff: elev_data.max_elevation_diff,
            });
        } else {
            failed_calculations += 1;
        }
    }

    tracing::info!(
        "Angle calculation complete: {} successful, {} failed",
        successful_calculations,
        failed_calculations
    );

    // Log elevation statistics
    log_elevation_stats(&elevation_stats);

    Ok(junctions_for_insert)
}

/// Statistics for elevation data retrieval
#[derive(Default)]
struct ElevationStats {
    total_junctions: usize,
    with_elevation: usize,
    with_all_neighbors: usize,
    with_partial_neighbors: usize,
    elevation_errors: usize,
}

impl ElevationStats {
    fn new() -> Self {
        Self::default()
    }
}

/// Elevation information for a junction
struct JunctionElevation {
    elevation: Option<f64>,
    neighbor_elevations: Option<[f64; 3]>,
    elevation_diffs: Option<[f64; 3]>,
    min_angle_index: Option<i16>,
    min_elevation_diff: Option<f64>,
    max_elevation_diff: Option<f64>,
}

/// Get elevation data for a junction and its neighbors
fn get_elevation_data(
    elevation_provider: &mut Option<ElevationProvider>,
    junction_lat: f64,
    junction_lon: f64,
    neighbor_points: &[(f64, f64)],
    angles: &[i16; 3],
    stats: &mut ElevationStats,
) -> JunctionElevation {
    stats.total_junctions += 1;

    // If no elevation provider, return all None
    let Some(provider) = elevation_provider.as_mut() else {
        return JunctionElevation {
            elevation: None,
            neighbor_elevations: None,
            elevation_diffs: None,
            min_angle_index: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
        };
    };

    // Get junction elevation
    let junction_elevation = match provider.get_elevation(junction_lat, junction_lon) {
        Ok(Some(elev)) => {
            stats.with_elevation += 1;
            Some(elev)
        }
        Ok(None) => None,
        Err(e) => {
            stats.elevation_errors += 1;
            tracing::debug!("Failed to get junction elevation: {}", e);
            None
        }
    };

    // Get neighbor elevations
    let neighbor_elevs: Vec<Option<f64>> = neighbor_points
        .iter()
        .map(|(lat, lon)| match provider.get_elevation(*lat, *lon) {
            Ok(Some(elev)) => Some(elev),
            Ok(None) => None,
            Err(e) => {
                stats.elevation_errors += 1;
                tracing::debug!("Failed to get neighbor elevation: {}", e);
                None
            }
        })
        .collect();

    // Only calculate if all elevations are available
    if let (Some(junction_elev), [Some(n1), Some(n2), Some(n3)]) = (
        junction_elevation,
        [neighbor_elevs[0], neighbor_elevs[1], neighbor_elevs[2]],
    ) {
        stats.with_all_neighbors += 1;

        let neighbor_elevations = [n1, n2, n3];
        let elevation_diffs =
            JunctionForInsert::calculate_elevation_diffs(junction_elev, &neighbor_elevations);
        let (min_diff, max_diff) = JunctionForInsert::calculate_min_max_diffs(&elevation_diffs);
        let min_angle_index = JunctionForInsert::calculate_min_angle_index(angles);

        JunctionElevation {
            elevation: Some(junction_elev),
            neighbor_elevations: Some(neighbor_elevations),
            elevation_diffs: Some(elevation_diffs),
            min_angle_index: Some(min_angle_index),
            min_elevation_diff: Some(min_diff),
            max_elevation_diff: Some(max_diff),
        }
    } else {
        // If partial elevations, count it
        if neighbor_elevs.iter().any(|e| e.is_some()) {
            stats.with_partial_neighbors += 1;
        }
        JunctionElevation {
            elevation: junction_elevation,
            neighbor_elevations: None,
            elevation_diffs: None,
            min_angle_index: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
        }
    }
}

/// Log elevation statistics
fn log_elevation_stats(stats: &ElevationStats) {
    if stats.total_junctions == 0 {
        return;
    }

    tracing::info!("Elevation data statistics:");
    tracing::info!("  Total junctions: {}", stats.total_junctions);
    tracing::info!(
        "  With junction elevation: {} ({:.1}%)",
        stats.with_elevation,
        (stats.with_elevation as f64 / stats.total_junctions as f64) * 100.0
    );
    tracing::info!(
        "  With all neighbor elevations: {} ({:.1}%)",
        stats.with_all_neighbors,
        (stats.with_all_neighbors as f64 / stats.total_junctions as f64) * 100.0
    );

    if stats.with_partial_neighbors > 0 {
        tracing::info!(
            "  With partial neighbor elevations: {}",
            stats.with_partial_neighbors
        );
    }

    if stats.elevation_errors > 0 {
        tracing::warn!("  Elevation errors encountered: {}", stats.elevation_errors);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_elevation_stats_initialization() {
        let stats = ElevationStats::new();
        assert_eq!(stats.total_junctions, 0);
        assert_eq!(stats.with_elevation, 0);
        assert_eq!(stats.with_all_neighbors, 0);
        assert_eq!(stats.with_partial_neighbors, 0);
        assert_eq!(stats.elevation_errors, 0);
    }

    #[test]
    fn test_junction_elevation_structure_none() {
        // Test that JunctionElevation can be created with None values
        let junction_elev = JunctionElevation {
            elevation: None,
            neighbor_elevations: None,
            elevation_diffs: None,
            min_angle_index: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
        };

        assert!(junction_elev.elevation.is_none());
        assert!(junction_elev.neighbor_elevations.is_none());
        assert!(junction_elev.elevation_diffs.is_none());
        assert!(junction_elev.min_angle_index.is_none());
        assert!(junction_elev.min_elevation_diff.is_none());
        assert!(junction_elev.max_elevation_diff.is_none());
    }

    #[test]
    fn test_junction_elevation_structure_with_data() {
        // Test that JunctionElevation can be created with Some values
        let junction_elev = JunctionElevation {
            elevation: Some(100.0),
            neighbor_elevations: Some([110.0, 120.0, 130.0]),
            elevation_diffs: Some([10.0, 20.0, 30.0]),
            min_angle_index: Some(1),
            min_elevation_diff: Some(10.0),
            max_elevation_diff: Some(30.0),
        };

        assert_eq!(junction_elev.elevation, Some(100.0));
        assert_eq!(
            junction_elev.neighbor_elevations,
            Some([110.0, 120.0, 130.0])
        );
        assert_eq!(junction_elev.elevation_diffs, Some([10.0, 20.0, 30.0]));
        assert_eq!(junction_elev.min_angle_index, Some(1));
        assert_eq!(junction_elev.min_elevation_diff, Some(10.0));
        assert_eq!(junction_elev.max_elevation_diff, Some(30.0));
    }
}
