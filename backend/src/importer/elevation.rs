use anyhow::{Context, Result};
use glob::glob;
use roxmltree::Document;
use std::collections::HashMap;
use std::path::PathBuf;

/// GSI DEM tile (one XML file)
#[derive(Debug, Clone)]
struct GsiTile {
    lower_corner: (f64, f64), // (lat, lon) - Southwest corner
    upper_corner: (f64, f64), // (lat, lon) - Northeast corner
    grid_width: usize,        // Number of columns (X direction)
    grid_height: usize,       // Number of rows (Y direction)
    elevations: Vec<f64>,     // Elevation values in +x-y order
}

impl GsiTile {
    /// Check if this tile contains the given coordinate
    fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.lower_corner.0
            && lat <= self.upper_corner.0
            && lon >= self.lower_corner.1
            && lon <= self.upper_corner.1
    }

    /// Get elevation at the given coordinate
    /// Returns None if coordinate is outside tile or elevation is invalid
    fn get_elevation(&self, lat: f64, lon: f64) -> Option<f64> {
        if !self.contains(lat, lon) {
            return None;
        }

        // Calculate fractional position within tile (0.0 to 1.0)
        let lat_frac = (lat - self.lower_corner.0) / (self.upper_corner.0 - self.lower_corner.0);
        let lon_frac = (lon - self.lower_corner.1) / (self.upper_corner.1 - self.lower_corner.1);

        // Convert to grid coordinates
        // Note: GSI data is ordered +x-y (west to east, north to south)
        let x = (lon_frac * (self.grid_width - 1) as f64).round() as usize;
        let y = ((1.0 - lat_frac) * (self.grid_height - 1) as f64).round() as usize;

        // Calculate index in flat array
        let index = y * self.grid_width + x;

        self.elevations.get(index).copied()
    }
}

/// Provides elevation data from GSI JPGIS XML files
pub struct ElevationProvider {
    /// Cache of loaded tiles, keyed by XML file path
    cache: HashMap<PathBuf, GsiTile>,
    /// Directory path containing XML files
    data_dir: String,
}

impl ElevationProvider {
    /// Creates a new ElevationProvider
    ///
    /// # Arguments
    /// * `data_dir` - Path to directory containing GSI .xml files (e.g., "data/gsi")
    pub fn new(data_dir: &str) -> Self {
        tracing::info!(
            "Initialized ElevationProvider with data directory: {}",
            data_dir
        );
        Self {
            cache: HashMap::new(),
            data_dir: data_dir.to_string(),
        }
    }

    /// Gets elevation at a specific coordinate
    ///
    /// # Arguments
    /// * `lat` - Latitude in decimal degrees
    /// * `lon` - Longitude in decimal degrees
    ///
    /// # Returns
    /// * `Ok(Some(elevation))` - Elevation in meters
    /// * `Ok(None)` - Valid coordinate but no data available
    /// * `Err(...)` - File read or parse error
    pub fn get_elevation(&mut self, lat: f64, lon: f64) -> Result<Option<f64>> {
        // Find all XML files in data directory
        let pattern = format!("{}/xml/*.xml", self.data_dir);
        let xml_files = glob(&pattern).context("Failed to read XML pattern")?;

        // Try each XML file
        for entry in xml_files {
            let xml_path = entry.context("Failed to get XML path")?;

            // Check cache first
            if let Some(tile) = self.cache.get(&xml_path) {
                if let Some(elevation) = tile.get_elevation(lat, lon) {
                    return Ok(Some(elevation));
                }
                // This tile doesn't contain the coordinate, try next
                continue;
            }

            // Load and parse XML file
            match Self::parse_xml_file(&xml_path) {
                Ok(tile) => {
                    // Check if this tile contains the coordinate
                    if let Some(elevation) = tile.get_elevation(lat, lon) {
                        // Cache and return
                        self.cache.insert(xml_path.clone(), tile);
                        return Ok(Some(elevation));
                    }
                    // Cache even if not found (to skip next time)
                    self.cache.insert(xml_path, tile);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse XML {:?}: {}", xml_path, e);
                }
            }
        }

        // No tile found containing this coordinate
        Ok(None)
    }

    /// Parses a GSI JPGIS XML file and extracts elevation data
    fn parse_xml_file(xml_path: &PathBuf) -> Result<GsiTile> {
        let xml_content = std::fs::read_to_string(xml_path)
            .context(format!("Failed to read XML file: {:?}", xml_path))?;

        let doc = Document::parse(&xml_content).context("Failed to parse XML")?;

        // Find <gml:Envelope> for coordinate bounds
        let envelope = doc
            .descendants()
            .find(|n| n.has_tag_name("Envelope"))
            .context("No Envelope element found")?;

        let lower_corner_text = envelope
            .descendants()
            .find(|n| n.has_tag_name("lowerCorner"))
            .and_then(|n| n.text())
            .context("No lowerCorner found")?;

        let upper_corner_text = envelope
            .descendants()
            .find(|n| n.has_tag_name("upperCorner"))
            .and_then(|n| n.text())
            .context("No upperCorner found")?;

        // Parse coordinates (format: "lat lon")
        let lower: Vec<f64> = lower_corner_text
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();
        let upper: Vec<f64> = upper_corner_text
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();

        anyhow::ensure!(lower.len() == 2, "Invalid lowerCorner format");
        anyhow::ensure!(upper.len() == 2, "Invalid upperCorner format");

        let lower_corner = (lower[0], lower[1]); // (lat, lon)
        let upper_corner = (upper[0], upper[1]); // (lat, lon)

        // Find <gml:Grid> for grid dimensions
        let grid = doc
            .descendants()
            .find(|n| n.has_tag_name("Grid"))
            .context("No Grid element found")?;

        let high_text = grid
            .descendants()
            .find(|n| n.has_tag_name("high"))
            .and_then(|n| n.text())
            .context("No high element found")?;

        // Parse grid size (format: "x y")
        let high: Vec<usize> = high_text
            .split_whitespace()
            .filter_map(|s| s.parse().ok())
            .collect();

        anyhow::ensure!(high.len() == 2, "Invalid high format");

        let grid_width = high[0] + 1; // high is max index, so add 1 for count
        let grid_height = high[1] + 1;

        // Find <gml:tupleList> for elevation data
        let tuple_list = doc
            .descendants()
            .find(|n| n.has_tag_name("tupleList"))
            .and_then(|n| n.text())
            .context("No tupleList found")?;

        // Parse elevation values (format: "地表面,elevation\n...")
        let elevations: Vec<f64> = tuple_list
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() == 2 {
                    parts[1].parse().ok()
                } else {
                    None
                }
            })
            .collect();

        anyhow::ensure!(
            elevations.len() == grid_width * grid_height,
            "Elevation count mismatch: expected {}, got {}",
            grid_width * grid_height,
            elevations.len()
        );

        Ok(GsiTile {
            lower_corner,
            upper_corner,
            grid_width,
            grid_height,
            elevations,
        })
    }

    /// Returns statistics about cache usage
    pub fn cache_stats(&self) -> (usize, usize) {
        let loaded_files = self.cache.len();
        let capacity = self.cache.capacity();
        (loaded_files, capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test constants
    const TEST_LAT_FUJI: f64 = 35.3606;
    const TEST_LON_FUJI: f64 = 138.7274;
    const EXPECTED_ELEVATION_FUJI: f64 = 3776.0;
    const ELEVATION_TOLERANCE: f64 = 100.0;

    const TEST_LAT_TOKYO: f64 = 35.6812;
    const TEST_LON_TOKYO: f64 = 139.7671;
    const EXPECTED_ELEVATION_TOKYO: f64 = 3.0;
    const ELEVATION_TOLERANCE_LOW: f64 = 20.0;

    /// Get fixture directory (always available in repo)
    fn get_fixture_dir() -> String {
        "tests/fixtures/gsi".to_string()
    }

    /// Get optional real data directory
    fn get_real_data_dir() -> Option<String> {
        let gsi_dir = "data/gsi";
        if std::path::Path::new(gsi_dir).exists() {
            Some(gsi_dir.to_string())
        } else {
            None
        }
    }

    #[test]
    fn test_new_provider() {
        let provider = ElevationProvider::new("tests/fixtures/gsi");
        assert_eq!(provider.data_dir, "tests/fixtures/gsi");
        assert_eq!(provider.cache.len(), 0);
    }

    #[test]
    fn test_fixture_data() {
        // Deterministic test using fixture (always runs in CI)
        let mut provider = ElevationProvider::new(&get_fixture_dir());

        // Test coordinates within fixture bounds (35.0-35.01, 138.0-138.01)
        let result = provider.get_elevation(35.005, 138.005);
        assert!(result.is_ok(), "Should successfully query fixture data");

        if let Ok(Some(elevation)) = result {
            // Fixture has elevations in 100-144m range
            assert!(
                (100.0..=150.0).contains(&elevation),
                "Fixture elevation should be in expected range, got {}m",
                elevation
            );
        } else {
            panic!("Should get elevation from fixture");
        }
    }

    #[test]
    fn test_get_elevation_fuji() {
        // Optional test with real GSI data (skipped in CI)
        let Some(data_dir) = get_real_data_dir() else {
            eprintln!("Skipping test: Real GSI data not available (data/gsi)");
            return;
        };

        let mut provider = ElevationProvider::new(&data_dir);
        let result = provider.get_elevation(TEST_LAT_FUJI, TEST_LON_FUJI);

        assert!(result.is_ok());
        if let Ok(Some(elevation)) = result {
            assert!(
                (elevation - EXPECTED_ELEVATION_FUJI).abs() < ELEVATION_TOLERANCE,
                "Mt. Fuji elevation should be ~{}m, got {}m",
                EXPECTED_ELEVATION_FUJI,
                elevation
            );
        }
    }

    #[test]
    fn test_get_elevation_tokyo() {
        // Optional test with real GSI data (skipped in CI)
        let Some(data_dir) = get_real_data_dir() else {
            eprintln!("Skipping test: Real GSI data not available (data/gsi)");
            return;
        };

        let mut provider = ElevationProvider::new(&data_dir);
        let result = provider.get_elevation(TEST_LAT_TOKYO, TEST_LON_TOKYO);

        assert!(result.is_ok());
        if let Ok(Some(elevation)) = result {
            assert!(
                (elevation - EXPECTED_ELEVATION_TOKYO).abs() < ELEVATION_TOLERANCE_LOW,
                "Tokyo Station elevation should be ~{}m, got {}m",
                EXPECTED_ELEVATION_TOKYO,
                elevation
            );
        }
    }

    #[test]
    fn test_missing_data() {
        let mut provider = ElevationProvider::new("/tmp/nonexistent_gsi");
        let result = provider.get_elevation(35.0, 139.0);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);
    }

    #[test]
    fn test_caching_behavior() {
        // Deterministic test using fixture (always runs in CI)
        let mut provider = ElevationProvider::new(&get_fixture_dir());

        // First query - should parse XML and cache
        let _ = provider.get_elevation(35.005, 138.005);
        let (files_1, _) = provider.cache_stats();

        // Second query - should use cache, no additional files
        let _ = provider.get_elevation(35.005, 138.005);
        let (files_2, _) = provider.cache_stats();

        assert_eq!(files_1, files_2, "Cache size should not increase");
        assert!(files_1 > 0, "At least one file should be cached");
    }
}
