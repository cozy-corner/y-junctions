use std::collections::{HashMap, HashSet};

/// Way tag information (bridge, tunnel, etc.)
#[derive(Debug, Clone, Default)]
pub struct WayTagInfo {
    pub bridge: bool,
    pub tunnel: bool,
}

/// Y-junction candidate information
#[derive(Debug, Clone)]
pub struct YJunctionCandidate {
    pub node_id: i64,
    pub connected_ways: Vec<i64>,
}

/// Y-junction with coordinate information
#[derive(Debug, Clone)]
pub struct YJunctionWithCoords {
    pub node_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub connected_ways: Vec<i64>,
}

/// Y-junction data ready for database insertion
#[derive(Debug, Clone)]
pub struct JunctionForInsert {
    pub osm_node_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub angle_1: i16,
    pub angle_2: i16,
    pub angle_3: i16,
    /// Bearings (azimuth) of the three roads from the junction node
    /// Each bearing is in degrees (0-360), where 0° is North, 90° is East
    /// Order corresponds to angle_1, angle_2, angle_3
    pub bearings: [f64; 3],

    #[allow(dead_code)]
    pub elevation: Option<f64>,
    #[allow(dead_code)]
    pub neighbor_elevations: Option<[f64; 3]>,
    #[allow(dead_code)]
    pub elevation_diffs: Option<[f64; 3]>,
    #[allow(dead_code)]
    pub min_angle_index: Option<i16>,
    #[allow(dead_code)]
    pub min_elevation_diff: Option<f64>,
    #[allow(dead_code)]
    pub max_elevation_diff: Option<f64>,

    // Way tag information for filtering
    pub way_1_bridge: bool,
    pub way_1_tunnel: bool,
    pub way_2_bridge: bool,
    pub way_2_tunnel: bool,
    pub way_3_bridge: bool,
    pub way_3_tunnel: bool,
}

impl JunctionForInsert {
    pub fn calculate_min_angle_index(angles: &[i16; 3]) -> i16 {
        let (min_idx, _) = angles
            .iter()
            .enumerate()
            .min_by_key(|(_, &angle)| angle)
            .unwrap();
        (min_idx + 1) as i16
    }

    pub fn calculate_elevation_diffs(base: f64, neighbors: &[f64; 3]) -> [f64; 3] {
        [
            (base - neighbors[0]).abs(),
            (base - neighbors[1]).abs(),
            (base - neighbors[2]).abs(),
        ]
    }

    pub fn calculate_min_max_diffs(diffs: &[f64; 3]) -> (f64, f64) {
        let min = diffs.iter().copied().fold(f64::INFINITY, f64::min);
        let max = diffs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (min, max)
    }
}

/// Node connection counter for Y-junction detection
#[derive(Debug)]
pub struct NodeConnectionCounter {
    /// Maps node_id to set of way_ids that contain this node
    node_to_ways: HashMap<i64, HashSet<i64>>,
    /// Maps way_id to list of node_ids in that way
    way_nodes: HashMap<i64, Vec<i64>>,
    /// Maps way_id to tag information (bridge, tunnel, etc.)
    way_tags: HashMap<i64, WayTagInfo>,
    /// Valid highway types for Y-junction detection
    valid_highway_types: HashSet<String>,
}

impl NodeConnectionCounter {
    pub fn new() -> Self {
        let mut valid_highway_types = HashSet::new();

        // Add common road types for Y-junction detection
        // Primary roads
        valid_highway_types.insert("motorway".to_string());
        valid_highway_types.insert("trunk".to_string());
        valid_highway_types.insert("primary".to_string());
        valid_highway_types.insert("secondary".to_string());
        valid_highway_types.insert("tertiary".to_string());

        // Local roads
        valid_highway_types.insert("residential".to_string());
        valid_highway_types.insert("unclassified".to_string());
        valid_highway_types.insert("service".to_string());

        // Links
        valid_highway_types.insert("motorway_link".to_string());
        valid_highway_types.insert("trunk_link".to_string());
        valid_highway_types.insert("primary_link".to_string());
        valid_highway_types.insert("secondary_link".to_string());
        valid_highway_types.insert("tertiary_link".to_string());

        Self {
            node_to_ways: HashMap::new(),
            way_nodes: HashMap::new(),
            way_tags: HashMap::new(),
            valid_highway_types,
        }
    }

    /// Check if highway type is valid for Y-junction detection
    pub fn is_valid_highway_type(&self, highway_type: &str) -> bool {
        self.valid_highway_types.contains(highway_type)
    }

    /// Add a way and its nodes to the connection counter
    pub fn add_way(
        &mut self,
        way_id: i64,
        node_ids: &[i64],
        _highway_type: &str,
        bridge: bool,
        tunnel: bool,
    ) {
        // Store way nodes
        self.way_nodes.insert(way_id, node_ids.to_vec());

        // Store way tags
        self.way_tags.insert(way_id, WayTagInfo { bridge, tunnel });

        for &node_id in node_ids {
            self.node_to_ways.entry(node_id).or_default().insert(way_id);
        }
    }

    /// Get the neighboring node IDs for a Y-junction node
    /// Returns up to 3 neighboring nodes (one per connected way)
    pub fn get_neighboring_nodes(&self, junction_node_id: i64) -> Vec<i64> {
        let mut neighbors = Vec::new();

        if let Some(way_ids) = self.node_to_ways.get(&junction_node_id) {
            for &way_id in way_ids {
                if let Some(nodes) = self.way_nodes.get(&way_id) {
                    // Find the junction node in the way's node list
                    if let Some(pos) = nodes.iter().position(|&id| id == junction_node_id) {
                        // Get the neighboring node (prefer next, fallback to previous)
                        if pos + 1 < nodes.len() {
                            neighbors.push(nodes[pos + 1]);
                        } else if pos > 0 {
                            neighbors.push(nodes[pos - 1]);
                        }
                    }
                }
            }
        }

        neighbors
    }

    /// Get neighboring nodes with their way tags in consistent order
    /// Returns a vector of (neighbor_node_id, way_tag_info) tuples
    /// This ensures that the neighbor node and its corresponding way tag are paired correctly
    pub fn get_neighbors_with_tags(&self, junction_node_id: i64) -> Vec<(i64, WayTagInfo)> {
        let mut result = Vec::new();

        if let Some(way_ids) = self.node_to_ways.get(&junction_node_id) {
            for &way_id in way_ids {
                if let Some(nodes) = self.way_nodes.get(&way_id) {
                    // Find the junction node in the way's node list
                    if let Some(pos) = nodes.iter().position(|&id| id == junction_node_id) {
                        // Get the neighboring node (prefer next, fallback to previous)
                        let neighbor_id = if pos + 1 < nodes.len() {
                            nodes[pos + 1]
                        } else if pos > 0 {
                            nodes[pos - 1]
                        } else {
                            continue;
                        };

                        // Get way tags
                        let tags = self.way_tags.get(&way_id).cloned().unwrap_or_default();

                        result.push((neighbor_id, tags));
                    }
                }
            }
        }

        result
    }

    /// Find all nodes that have exactly 3 way connections (Y-junction candidates)
    pub fn find_y_junction_candidates(&self) -> Vec<YJunctionCandidate> {
        self.node_to_ways
            .iter()
            .filter_map(|(&node_id, way_ids)| {
                if way_ids.len() == 3 {
                    Some(YJunctionCandidate {
                        node_id,
                        connected_ways: way_ids.iter().copied().collect(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the number of unique nodes tracked
    pub fn node_count(&self) -> usize {
        self.node_to_ways.len()
    }

    /// Get the number of ways a specific node is connected to
    pub fn get_connection_count(&self, node_id: i64) -> usize {
        self.node_to_ways
            .get(&node_id)
            .map(|ways| ways.len())
            .unwrap_or(0)
    }

    /// Get tag information for connected ways of a junction node
    /// Returns a vector of WayTagInfo for each connected way (should be 3 for Y-junctions)
    pub fn get_connected_way_tags(&self, junction_node_id: i64) -> Vec<WayTagInfo> {
        if let Some(way_ids) = self.node_to_ways.get(&junction_node_id) {
            way_ids
                .iter()
                .filter_map(|way_id| self.way_tags.get(way_id).cloned())
                .collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for NodeConnectionCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_connection_counter() {
        let mut counter = NodeConnectionCounter::new();

        // Way 1: nodes [1, 2, 3]
        counter.add_way(1, &[1, 2, 3], "residential", false, false);

        // Way 2: nodes [2, 4]
        counter.add_way(2, &[2, 4], "tertiary", false, false);

        // Way 3: nodes [2, 5]
        counter.add_way(3, &[2, 5], "primary", false, false);

        assert_eq!(counter.get_connection_count(1), 1); // Node 1: 1 way
        assert_eq!(counter.get_connection_count(2), 3); // Node 2: 3 ways (Y-junction)
        assert_eq!(counter.get_connection_count(3), 1); // Node 3: 1 way
        assert_eq!(counter.get_connection_count(4), 1); // Node 4: 1 way
        assert_eq!(counter.get_connection_count(5), 1); // Node 5: 1 way

        let candidates = counter.find_y_junction_candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].node_id, 2);
        assert_eq!(candidates[0].connected_ways.len(), 3);
    }

    #[test]
    fn test_valid_highway_types() {
        let counter = NodeConnectionCounter::new();

        assert!(counter.is_valid_highway_type("residential"));
        assert!(counter.is_valid_highway_type("tertiary"));
        assert!(counter.is_valid_highway_type("primary"));
        assert!(counter.is_valid_highway_type("motorway"));

        assert!(!counter.is_valid_highway_type("footway"));
        assert!(!counter.is_valid_highway_type("cycleway"));
        assert!(!counter.is_valid_highway_type("path"));
    }

    #[test]
    fn test_calculate_min_angle_index() {
        assert_eq!(
            JunctionForInsert::calculate_min_angle_index(&[30, 150, 180]),
            1
        );
        assert_eq!(
            JunctionForInsert::calculate_min_angle_index(&[150, 30, 180]),
            2
        );
        assert_eq!(
            JunctionForInsert::calculate_min_angle_index(&[150, 180, 30]),
            3
        );
        assert_eq!(
            JunctionForInsert::calculate_min_angle_index(&[120, 120, 120]),
            1
        );
    }

    #[test]
    fn test_calculate_elevation_diffs() {
        let base = 100.0;
        let neighbors = [95.0, 105.0, 100.0];
        let diffs = JunctionForInsert::calculate_elevation_diffs(base, &neighbors);
        assert_eq!(diffs, [5.0, 5.0, 0.0]);

        let neighbors2 = [110.0, 90.0, 85.0];
        let diffs2 = JunctionForInsert::calculate_elevation_diffs(base, &neighbors2);
        assert_eq!(diffs2, [10.0, 10.0, 15.0]);
    }

    #[test]
    fn test_calculate_min_max_diffs() {
        let diffs = [5.0, 10.0, 15.0];
        let (min, max) = JunctionForInsert::calculate_min_max_diffs(&diffs);
        assert_eq!(min, 5.0);
        assert_eq!(max, 15.0);

        let diffs2 = [0.0, 0.0, 0.0];
        let (min2, max2) = JunctionForInsert::calculate_min_max_diffs(&diffs2);
        assert_eq!(min2, 0.0);
        assert_eq!(max2, 0.0);
    }

    #[test]
    fn test_way_tags_storage() {
        let mut counter = NodeConnectionCounter::new();

        // Add ways with different bridge/tunnel tags
        counter.add_way(1, &[1, 2], "primary", true, false); // bridge
        counter.add_way(2, &[2, 3], "secondary", false, true); // tunnel
        counter.add_way(3, &[3, 4], "tertiary", false, false); // neither

        // Verify tags are stored correctly
        assert!(counter.way_tags.get(&1).unwrap().bridge);
        assert!(!counter.way_tags.get(&1).unwrap().tunnel);

        assert!(!counter.way_tags.get(&2).unwrap().bridge);
        assert!(counter.way_tags.get(&2).unwrap().tunnel);

        assert!(!counter.way_tags.get(&3).unwrap().bridge);
        assert!(!counter.way_tags.get(&3).unwrap().tunnel);
    }

    #[test]
    fn test_get_connected_way_tags() {
        let mut counter = NodeConnectionCounter::new();

        // Create a Y-junction at node 2
        counter.add_way(1, &[1, 2], "primary", true, false); // bridge
        counter.add_way(2, &[2, 3], "secondary", false, true); // tunnel
        counter.add_way(3, &[2, 4], "tertiary", false, false); // neither

        let tags = counter.get_connected_way_tags(2);

        // Should return 3 tags (order may vary)
        assert_eq!(tags.len(), 3);

        // Check that all tags are present
        let has_bridge = tags.iter().any(|t| t.bridge && !t.tunnel);
        let has_tunnel = tags.iter().any(|t| !t.bridge && t.tunnel);
        let has_neither = tags.iter().any(|t| !t.bridge && !t.tunnel);

        assert!(has_bridge, "Should have a bridge way");
        assert!(has_tunnel, "Should have a tunnel way");
        assert!(has_neither, "Should have a normal way");
    }

    #[test]
    fn test_get_neighbors_with_tags() {
        let mut counter = NodeConnectionCounter::new();

        // Create a Y-junction at node 2 with specific neighbor nodes
        counter.add_way(1, &[10, 2], "primary", true, false); // way 1: bridge, neighbor 10
        counter.add_way(2, &[2, 3], "secondary", false, true); // way 2: tunnel, neighbor 3
        counter.add_way(3, &[2, 30], "tertiary", false, false); // way 3: neither, neighbor 30

        let data = counter.get_neighbors_with_tags(2);

        // Should return 3 (neighbor_id, tag) pairs
        assert_eq!(data.len(), 3, "Should have 3 neighbor-tag pairs");

        // Verify that neighbor IDs are present
        let neighbor_ids: Vec<i64> = data.iter().map(|(id, _)| *id).collect();
        assert!(
            neighbor_ids.contains(&10),
            "Should contain neighbor node 10"
        );
        assert!(neighbor_ids.contains(&3), "Should contain neighbor node 3");
        assert!(
            neighbor_ids.contains(&30),
            "Should contain neighbor node 30"
        );

        // Verify that neighbor and tag are correctly paired
        // Find the pair with neighbor 10 (from way 1 with bridge)
        let pair_10 = data.iter().find(|(id, _)| *id == 10).unwrap();
        assert!(
            pair_10.1.bridge && !pair_10.1.tunnel,
            "Neighbor 10 should be paired with bridge tag"
        );

        // Find the pair with neighbor 3 (from way 2 with tunnel)
        let pair_3 = data.iter().find(|(id, _)| *id == 3).unwrap();
        assert!(
            !pair_3.1.bridge && pair_3.1.tunnel,
            "Neighbor 3 should be paired with tunnel tag"
        );

        // Find the pair with neighbor 30 (from way 3 with neither)
        let pair_30 = data.iter().find(|(id, _)| *id == 30).unwrap();
        assert!(
            !pair_30.1.bridge && !pair_30.1.tunnel,
            "Neighbor 30 should be paired with neither tag"
        );
    }
}
