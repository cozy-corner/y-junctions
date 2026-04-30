use anyhow::Result;
use dashmap::DashMap;
use rayon::prelude::*;
use std::collections::HashSet;
use std::fs::File;
use std::sync::Arc;

use super::calculator::calculate_junction_angles;
use super::detector::{JunctionForInsert, NodeConnectionCounter, YJunctionWithCoords};
use crate::domain::junction::AngleType;

/// Way data collected during parallel parsing
#[derive(Debug)]
struct WayData {
    way_id: i64,
    node_ids: Vec<i64>,
    highway_type: String,
    bridge: bool,
    tunnel: bool,
}

/// Local state for each parallel parsing thread
#[derive(Debug, Default)]
struct LocalState {
    ways: Vec<WayData>,
    nodes: Vec<(i64, (f64, f64))>,
    bus_stop_node_ids: Vec<i64>,
}

impl LocalState {
    fn new() -> Self {
        Self::default()
    }

    fn merge(mut self, other: Self) -> Self {
        self.ways.extend(other.ways);
        self.nodes.extend(other.nodes);
        self.bus_stop_node_ids.extend(other.bus_stop_node_ids);
        self
    }
}

/// Type alias for the shared cache and counter structure
type CacheAndCounter = (Arc<DashMap<i64, (f64, f64)>>, NodeConnectionCounter);

/// Build node coordinates cache and NodeConnectionCounter from PBF file
/// This is shared by both 3-way and 2-way detection
fn build_cache_and_counter(input_path: &str) -> Result<CacheAndCounter> {
    let file = File::open(input_path)?;
    let reader = osmpbf::ElementReader::new(file);

    // Create valid highway types set for filtering (shared across threads)
    let temp_counter = NodeConnectionCounter::new();
    let valid_highway_types: Arc<HashSet<String>> =
        Arc::new(temp_counter.valid_highway_types().iter().cloned().collect());

    // Parallel map-reduce to collect ways and nodes
    let valid_types = Arc::clone(&valid_highway_types);
    let local_state = reader.par_map_reduce(
        move |element| {
            let mut local = LocalState::new();
            match element {
                osmpbf::Element::Way(way) => {
                    // Check if this way has a highway tag
                    if let Some(highway_type) =
                        way.tags().find(|&(k, _)| k == "highway").map(|(_, v)| v)
                    {
                        // Check if it's a valid highway type for Y-junction detection
                        if valid_types.contains(highway_type) {
                            // Collect node IDs from this way
                            let node_ids: Vec<i64> = way.refs().collect();

                            // Extract bridge and tunnel tags
                            let bridge = way.tags().any(|(k, v)| k == "bridge" && v == "yes");
                            let tunnel = way.tags().any(|(k, v)| k == "tunnel" && v == "yes");

                            // Store way data
                            local.ways.push(WayData {
                                way_id: way.id(),
                                node_ids,
                                highway_type: highway_type.to_string(),
                                bridge,
                                tunnel,
                            });
                        }
                    }
                }
                osmpbf::Element::Node(node) => {
                    // Cache all node coordinates
                    local.nodes.push((node.id(), (node.lat(), node.lon())));
                    if node
                        .tags()
                        .any(|(k, v)| k == "public_transport" && v == "stop_position")
                    {
                        local.bus_stop_node_ids.push(node.id());
                    }
                }
                osmpbf::Element::DenseNode(node) => {
                    // Cache all dense node coordinates
                    local.nodes.push((node.id(), (node.lat(), node.lon())));
                    if node
                        .tags()
                        .any(|(k, v)| k == "public_transport" && v == "stop_position")
                    {
                        local.bus_stop_node_ids.push(node.id());
                    }
                }
                _ => {}
            }
            local
        },
        LocalState::new,
        |a, b| a.merge(b),
    )?;

    // Build NodeConnectionCounter from collected data
    let way_count = local_state.ways.len();
    let highway_way_count = way_count; // All ways in local_state are highway ways

    let mut counter = NodeConnectionCounter::new();
    for way in &local_state.ways {
        counter.add_way(
            way.way_id,
            &way.node_ids,
            &way.highway_type,
            way.bridge,
            way.tunnel,
        );
    }
    for &node_id in &local_state.bus_stop_node_ids {
        counter.add_bus_stop_node(node_id);
    }

    // Build node coordinates map
    let node_coords: DashMap<i64, (f64, f64)> = DashMap::new();
    for (node_id, coords) in local_state.nodes {
        node_coords.insert(node_id, coords);
    }

    tracing::info!("Single pass complete:");
    tracing::info!("  Total ways processed: {}", way_count);
    tracing::info!("  Highway ways found: {}", highway_way_count);
    tracing::info!(
        "  Unique nodes in highway network: {}",
        counter.node_count()
    );
    tracing::info!("  Total node coordinates cached: {}", node_coords.len());
    tracing::info!(
        "  Bus stop nodes (public_transport=stop_position): {}",
        local_state.bus_stop_node_ids.len()
    );

    Ok((Arc::new(node_coords), counter))
}

/// Parse PBF file and extract 3-way Y-junctions only
pub fn parse_pbf_three_way(
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<Vec<JunctionForInsert>> {
    tracing::info!(
        "Parsing PBF file (3-way only) with bbox: ({}, {}) to ({}, {})",
        min_lon,
        min_lat,
        max_lon,
        max_lat
    );

    // Build cache and counter
    tracing::info!("Starting single-pass (parallel): collecting ways, nodes, and coordinates");
    let (node_coords, counter) = build_cache_and_counter(input_path)?;

    // Find Y-junction candidates (nodes with exactly 3 way connections)
    let candidates = counter.find_y_junction_candidates();
    tracing::info!("Found {} Y-junction candidates", candidates.len());

    if candidates.is_empty() {
        tracing::warn!("No Y-junction candidates found");
        return Ok(Vec::new());
    }

    // In-memory processing: Retrieve coordinates for Y-junction candidates from cache
    tracing::info!("Processing candidates: retrieving coordinates from cache");

    // Combine candidates with their coordinates (with bbox filtering) - PARALLEL
    let y_junctions: Vec<YJunctionWithCoords> = candidates
        .par_iter()
        .filter_map(|candidate| {
            node_coords.get(&candidate.node_id).and_then(|coords_ref| {
                let (lat, lon) = *coords_ref;
                // Check if node is within bounding box
                if lon >= min_lon && lon <= max_lon && lat >= min_lat && lat <= max_lat {
                    Some(YJunctionWithCoords {
                        node_id: candidate.node_id,
                        lat,
                        lon,
                        connected_ways: candidate.connected_ways.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .collect();

    tracing::info!(
        "Found {} Y-junction candidates (within bbox)",
        y_junctions.len()
    );

    if y_junctions.is_empty() {
        tracing::warn!("No Y-junction candidates found within bounding box");
        return Ok(Vec::new());
    }

    // In-memory processing: Calculate angles for Y-junctions - PARALLEL
    tracing::info!("Processing angles: calculating bearings and angles from cached coordinates");

    let junctions: Vec<JunctionForInsert> = y_junctions
        .par_iter()
        .filter_map(|junction| {
            // Get neighboring nodes with their way tags in consistent order
            let neighbor_data = counter.get_neighbors_with_tags(junction.node_id);

            if neighbor_data.len() != 3 {
                return None;
            }

            // Extract neighbor IDs and way tags from the paired data
            let neighbor_ids: Vec<i64> = neighbor_data.iter().map(|(id, _)| *id).collect();
            let way_tags: Vec<_> = neighbor_data.iter().map(|(_, tag)| tag).collect();

            // Get coordinates for all 3 neighboring nodes from cache
            let neighbor_points: Vec<(f64, f64)> = neighbor_ids
                .iter()
                .filter_map(|&id| node_coords.get(&id).map(|coords_ref| *coords_ref))
                .collect();

            if neighbor_points.len() != 3 {
                return None;
            }

            // Calculate angles and bearings
            let (angles, bearings) =
                calculate_junction_angles(junction.lat, junction.lon, &neighbor_points)?;

            // Find minimum angle for filtering and type classification
            let min_angle = *angles.iter().min().unwrap();

            if min_angle < 10 {
                return None;
            }

            // 最小角度が60度以上の場合はT字路とみなして除外
            if min_angle >= 60 {
                return None;
            }

            // Exclude paired bus pull-outs (Pattern A): one branch is a service way
            // leading to a bus_stop that loops back to another branch via a second
            // service way. Catches both J1 and J2 of the pair symmetrically.
            if counter.junction_has_bus_stop_loop(junction.node_id) {
                return None;
            }

            // Extract bridge/tunnel flags from way tags (now guaranteed to be in same order as angles)
            let (way_1_bridge, way_1_tunnel) = (way_tags[0].bridge, way_tags[0].tunnel);
            let (way_2_bridge, way_2_tunnel) = (way_tags[1].bridge, way_tags[1].tunnel);
            let (way_3_bridge, way_3_tunnel) = (way_tags[2].bridge, way_tags[2].tunnel);

            // Extract highway types from way tags
            let way_1_highway_type = way_tags[0].highway_type.clone();
            let way_2_highway_type = way_tags[1].highway_type.clone();
            let way_3_highway_type = way_tags[2].highway_type.clone();

            // Create JunctionForInsert
            Some(JunctionForInsert {
                osm_node_id: junction.node_id,
                lat: junction.lat,
                lon: junction.lon,
                angle_1: angles[0],
                angle_2: angles[1],
                angle_3: angles[2],
                bearings,
                elevation: None,
                neighbor_elevations: None,
                elevation_diffs: None,
                min_angle_index: None,
                min_elevation_diff: None,
                max_elevation_diff: None,
                way_1_bridge,
                way_1_tunnel,
                way_2_bridge,
                way_2_tunnel,
                way_3_bridge,
                way_3_tunnel,
                way_1_highway_type,
                way_2_highway_type,
                way_3_highway_type,
            })
        })
        .collect();

    let successful_calculations = junctions.len();
    let failed_calculations = y_junctions.len() - successful_calculations;

    // Log first 10 junctions for verification (after parallel processing)
    for junction in junctions.iter().take(10) {
        let mut sorted_angles = [junction.angle_1, junction.angle_2, junction.angle_3];
        sorted_angles.sort_unstable();
        let angle_type =
            AngleType::from_angles(sorted_angles[0], sorted_angles[1], sorted_angles[2]);

        tracing::info!(
            "Node {}: [{}°, {}°, {}°] type={:?}, bearings=[{:.1}°, {:.1}°, {:.1}°]",
            junction.osm_node_id,
            junction.angle_1,
            junction.angle_2,
            junction.angle_3,
            angle_type,
            junction.bearings[0],
            junction.bearings[1],
            junction.bearings[2]
        );
    }

    tracing::info!(
        "Angle calculation complete: {} successful, {} failed",
        successful_calculations,
        failed_calculations
    );

    Ok(junctions)
}

/// Parse PBF file and extract 2-way Y-junctions only
pub fn parse_pbf_two_way(
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<Vec<JunctionForInsert>> {
    tracing::info!(
        "Parsing PBF file (2-way only) with bbox: ({}, {}) to ({}, {})",
        min_lon,
        min_lat,
        max_lon,
        max_lat
    );

    // Build cache and counter
    tracing::info!("Starting single-pass (parallel): collecting ways, nodes, and coordinates");
    let (node_coords, counter) = build_cache_and_counter(input_path)?;

    // Process 2-way junctions
    tracing::info!("Processing 2-way junction candidates");

    let two_way_candidates = counter.find_two_way_junction_candidates();
    tracing::info!(
        "Found {} 2-way junction candidates",
        two_way_candidates.len()
    );

    if two_way_candidates.is_empty() {
        tracing::warn!("No 2-way junction candidates found");
        return Ok(Vec::new());
    }

    // Process 2-way candidates - PARALLEL
    let junctions: Vec<JunctionForInsert> = two_way_candidates
        .par_iter()
        .filter_map(|candidate| {
            // Get coordinates for the junction node
            let (junction_lat, junction_lon) = *node_coords.get(&candidate.node_id)?;

            // Check if node is within bounding box
            if junction_lon < min_lon
                || junction_lon > max_lon
                || junction_lat < min_lat
                || junction_lat > max_lat
            {
                return None;
            }

            // Get coordinates for the three points
            let prev_coords = *node_coords.get(&candidate.prev_node)?;
            let next_coords = *node_coords.get(&candidate.next_node)?;
            let neighbor_coords = *node_coords.get(&candidate.neighbor_node)?;

            let points = [prev_coords, next_coords, neighbor_coords];

            // Calculate angles and bearings
            let (angles, bearings) =
                calculate_junction_angles(junction_lat, junction_lon, &points)?;

            // Find minimum angle for filtering
            let min_angle = *angles.iter().min().unwrap();

            if min_angle < 10 {
                return None;
            }

            // Filter out T-junctions (minimum angle >= 60°)
            if min_angle >= 60 {
                tracing::debug!(
                    "2-way REJECTED (min_angle={}°): node {} at ({:.7}, {:.7}), angles=[{}°, {}°, {}°]",
                    min_angle, candidate.node_id, junction_lat, junction_lon,
                    angles[0], angles[1], angles[2]
                );
                return None;
            }

            // Exclude paired bus pull-outs (Pattern A): see parse_pbf_three_way for details.
            if counter.junction_has_bus_stop_loop(candidate.node_id) {
                return None;
            }

            // Get way tags for the two ways
            // For 2-way junctions: way_1 = passing way (entry), way_2 = passing way (exit), way_3 = connecting way
            let passing_tag = counter.get_way_tag(candidate.passing_way_id)?;
            let connecting_tag = counter.get_way_tag(candidate.connecting_way_id)?;

            // Map the three directions to way information
            // Direction 0 (bearing[0]) = prev_node direction = way_1 (passing entry)
            // Direction 1 (bearing[1]) = next_node direction = way_2 (passing exit)
            // Direction 2 (bearing[2]) = neighbor_node direction = way_3 (connecting)
            let way_1_bridge = passing_tag.bridge;
            let way_1_tunnel = passing_tag.tunnel;
            let way_2_bridge = passing_tag.bridge; // Same way as way_1
            let way_2_tunnel = passing_tag.tunnel;
            let way_3_bridge = connecting_tag.bridge;
            let way_3_tunnel = connecting_tag.tunnel;

            let way_1_highway_type = passing_tag.highway_type.clone();
            let way_2_highway_type = passing_tag.highway_type.clone(); // Same way as way_1
            let way_3_highway_type = connecting_tag.highway_type.clone();

            Some(JunctionForInsert {
                osm_node_id: candidate.node_id,
                lat: junction_lat,
                lon: junction_lon,
                angle_1: angles[0],
                angle_2: angles[1],
                angle_3: angles[2],
                bearings,
                elevation: None,
                neighbor_elevations: None,
                elevation_diffs: None,
                min_angle_index: None,
                min_elevation_diff: None,
                max_elevation_diff: None,
                way_1_bridge,
                way_1_tunnel,
                way_2_bridge,
                way_2_tunnel,
                way_3_bridge,
                way_3_tunnel,
                way_1_highway_type,
                way_2_highway_type,
                way_3_highway_type,
            })
        })
        .collect();

    let successful = junctions.len();
    let failed = two_way_candidates.len() - successful;

    tracing::info!(
        "2-way junction processing complete: {} successful, {} filtered (angle >= 60° or missing data)",
        successful,
        failed
    );

    // Log first 5 two-way junctions
    for junction in junctions.iter().take(5) {
        let mut sorted_angles = [junction.angle_1, junction.angle_2, junction.angle_3];
        sorted_angles.sort_unstable();
        let angle_type =
            AngleType::from_angles(sorted_angles[0], sorted_angles[1], sorted_angles[2]);

        tracing::info!(
            "2-way Node {}: [{}°, {}°, {}°] type={:?}, bearings=[{:.1}°, {:.1}°, {:.1}°]",
            junction.osm_node_id,
            junction.angle_1,
            junction.angle_2,
            junction.angle_3,
            angle_type,
            junction.bearings[0],
            junction.bearings[1],
            junction.bearings[2]
        );
    }

    Ok(junctions)
}

#[cfg(test)]
mod tests {
    // Tests removed - elevation logic moved to separate module
}
