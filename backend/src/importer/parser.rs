use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs::File;

use super::calculator::calculate_junction_angles;
use super::detector::{JunctionForInsert, NodeConnectionCounter, YJunctionWithCoords};
use crate::domain::junction::AngleType;

pub fn parse_pbf(
    input_path: &str,
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

        // Calculate angles
        if let Some(angles) =
            calculate_junction_angles(junction.lat, junction.lon, &neighbor_points)
        {
            let angle_type = AngleType::from_angles(angles[0], angles[1], angles[2]);
            successful_calculations += 1;

            // Log first 10 junctions for verification
            if successful_calculations <= 10 {
                tracing::info!(
                    "Node {}: [{}\u{00b0}, {}\u{00b0}, {}\u{00b0}] type={:?}",
                    junction.node_id,
                    angles[0],
                    angles[1],
                    angles[2],
                    angle_type
                );
            }

            // Create JunctionForInsert
            junctions_for_insert.push(JunctionForInsert {
                osm_node_id: junction.node_id,
                lat: junction.lat,
                lon: junction.lon,
                angle_1: angles[0],
                angle_2: angles[1],
                angle_3: angles[2],
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

    Ok(junctions_for_insert)
}
