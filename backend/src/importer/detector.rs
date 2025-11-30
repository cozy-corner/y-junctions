use std::collections::{HashMap, HashSet};

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

/// Node connection counter for Y-junction detection
#[derive(Debug)]
pub struct NodeConnectionCounter {
    /// Maps node_id to set of way_ids that contain this node
    node_to_ways: HashMap<i64, HashSet<i64>>,
    /// Maps way_id to list of node_ids in that way
    way_nodes: HashMap<i64, Vec<i64>>,
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
            valid_highway_types,
        }
    }

    /// Check if highway type is valid for Y-junction detection
    pub fn is_valid_highway_type(&self, highway_type: &str) -> bool {
        self.valid_highway_types.contains(highway_type)
    }

    /// Add a way and its nodes to the connection counter
    pub fn add_way(&mut self, way_id: i64, node_ids: &[i64]) {
        // Store way nodes
        self.way_nodes.insert(way_id, node_ids.to_vec());

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
        counter.add_way(1, &[1, 2, 3]);

        // Way 2: nodes [2, 4]
        counter.add_way(2, &[2, 4]);

        // Way 3: nodes [2, 5]
        counter.add_way(3, &[2, 5]);

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
}
