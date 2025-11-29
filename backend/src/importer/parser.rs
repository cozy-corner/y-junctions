use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs::File;

use super::detector::{NodeConnectionCounter, YJunctionWithCoords};

pub fn parse_pbf(
    input_path: &str,
    min_lon: f64,
    min_lat: f64,
    max_lon: f64,
    max_lat: f64,
) -> Result<()> {
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
                    counter.add_way(way.id(), &node_ids);
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
        return Ok(());
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

    // Log sample Y-junctions for verification
    for (i, junction) in y_junctions.iter().take(5).enumerate() {
        tracing::info!(
            "  Sample {}: Node {} at ({:.6}, {:.6}) with {} connected ways",
            i + 1,
            junction.node_id,
            junction.lat,
            junction.lon,
            junction.connected_ways.len()
        );
    }

    Ok(())
}
