use anyhow::{Context, Result};
use glob::glob;
use roxmltree::Document;
use std::collections::HashMap;
use std::path::PathBuf;

fn calculate_mesh_code(lat: f64, lon: f64) -> String {
    // 基盤地図情報の標準メッシュコード計算式
    //
    // 計算方法の出典:
    // https://qiita.com/jp-96/items/528ff81814b21c6c21e4
    // 【( ..)φメモメモ】基盤地図情報数値標高モデルのファイル名について
    //
    // 緯度メッシュ番号 = INT(緯度 × 120)
    // 経度メッシュ番号 = INT((経度 - 100) × 80)
    let lat_mesh = (lat * 120.0).floor() as i32;
    let lon_mesh = ((lon - 100.0) * 80.0).floor() as i32;

    // 1次メッシュ番号 (pppp)
    let lat_1 = lat_mesh / 80;
    let lon_1 = lon_mesh / 80;
    let first_mesh = format!("{}{}", lat_1, lon_1);

    // 2次メッシュ番号 (qq)
    let lat_2 = (lat_mesh / 10) % 8;
    let lon_2 = (lon_mesh / 10) % 8;
    let second_mesh = format!("{}{}", lat_2, lon_2);

    // 3次メッシュ番号 (rr)
    let lat_3 = lat_mesh % 10;
    let lon_3 = lon_mesh % 10;
    let third_mesh = format!("{}{}", lat_3, lon_3);

    format!("{}-{}-{}", first_mesh, second_mesh, third_mesh)
}

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
        // Clamp to prevent floating point edge cases
        let x = ((lon_frac * (self.grid_width - 1) as f64)
            .round()
            .clamp(0.0, (self.grid_width - 1) as f64)) as usize;
        let y = (((1.0 - lat_frac) * (self.grid_height - 1) as f64)
            .round()
            .clamp(0.0, (self.grid_height - 1) as f64)) as usize;

        // Calculate index in flat array
        let index = y * self.grid_width + x;

        self.elevations.get(index).copied()
    }
}

/// Provides elevation data from GSI JPGIS XML files
pub struct ElevationProvider {
    /// Cache of loaded tiles, keyed by mesh code
    cache: HashMap<String, GsiTile>,
    /// Map from mesh code to XML file path
    mesh_to_file: HashMap<String, PathBuf>,
}

impl ElevationProvider {
    /// Creates a new ElevationProvider
    ///
    /// # Arguments
    /// * `data_dir` - Path to directory containing GSI .xml files (e.g., "data/gsi")
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully initialized with elevation data files
    /// * `Err(...)` - No XML files found in the specified directory
    pub fn new(data_dir: &str) -> Result<Self> {
        let pattern = format!("{}/xml/*.xml", data_dir);
        let xml_files: Vec<PathBuf> = glob(&pattern)
            .map(|paths| paths.filter_map(|p| p.ok()).collect())
            .unwrap_or_default();

        anyhow::ensure!(
            !xml_files.is_empty(),
            "No XML files found in {}. Cannot proceed with elevation import.",
            pattern
        );

        let mut mesh_to_file = HashMap::new();
        for path in xml_files {
            if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                if let Some(mesh_code) = Self::extract_mesh_code(filename) {
                    mesh_to_file.insert(mesh_code, path);
                }
            }
        }

        tracing::info!(
            "Initialized ElevationProvider: {} mesh codes indexed",
            mesh_to_file.len()
        );

        Ok(Self {
            cache: HashMap::new(),
            mesh_to_file,
        })
    }

    fn extract_mesh_code(filename: &str) -> Option<String> {
        if let Some(start) = filename.find("FG-GML-") {
            let after_prefix = &filename[start + 7..];
            let parts: Vec<&str> = after_prefix.split('-').collect();
            if parts.len() >= 3 {
                return Some(format!("{}-{}-{}", parts[0], parts[1], parts[2]));
            }
        }
        None
    }

    /// Gets elevation at a specific coordinate
    ///
    /// # Arguments
    /// * `lat` - Latitude in decimal degrees
    /// * `lon` - Longitude in decimal degrees
    ///
    /// # Returns
    /// * `Ok(Some(elevation))` - Elevation in meters
    /// * `Ok(None)` - Valid coordinate but no data available (XML parse errors are logged and skipped)
    /// * `Err(...)` - File read error
    pub fn get_elevation(&mut self, lat: f64, lon: f64) -> Result<Option<f64>> {
        let mesh_code = calculate_mesh_code(lat, lon);

        if let Some(tile) = self.cache.get(&mesh_code) {
            return Ok(tile.get_elevation(lat, lon));
        }

        if let Some(xml_path) = self.mesh_to_file.get(&mesh_code) {
            match Self::parse_xml_file(xml_path) {
                Ok(tile) => {
                    let elevation = tile.get_elevation(lat, lon);
                    self.cache.insert(mesh_code, tile);
                    return Ok(elevation);
                }
                Err(e) => {
                    tracing::warn!("Failed to parse XML {:?}: {}", xml_path, e);
                    return Ok(None);
                }
            }
        }

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

        // Allow partial data for boundary tiles (海や国境でデータが欠損している場合)
        if elevations.len() != grid_width * grid_height {
            tracing::debug!(
                "Partial elevation data in {:?}: expected {}, got {} (boundary tile)",
                xml_path,
                grid_width * grid_height,
                elevations.len()
            );
        }

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
        let provider = ElevationProvider::new("tests/fixtures/gsi").unwrap();
        assert_eq!(provider.cache.len(), 0);
        assert!(
            !provider.mesh_to_file.is_empty(),
            "Should find at least one XML file in fixtures"
        );
    }

    #[test]
    fn test_fixture_data() {
        // Deterministic test using fixture (always runs in CI)
        let mut provider = ElevationProvider::new(&get_fixture_dir()).unwrap();

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

        let mut provider = ElevationProvider::new(&data_dir).unwrap();
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

        let mut provider = ElevationProvider::new(&data_dir).unwrap();
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
        // Should error when no XML files are found
        let result = ElevationProvider::new("/tmp/nonexistent_gsi");
        assert!(result.is_err(), "Should error when no XML files found");
    }

    #[test]
    fn test_caching_behavior() {
        // Deterministic test using fixture (always runs in CI)
        let mut provider = ElevationProvider::new(&get_fixture_dir()).unwrap();

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
