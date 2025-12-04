use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AngleType {
    VerySharp,
    Sharp,
    Skewed,
    Normal,
}

impl AngleType {
    pub fn from_angles(angle_1: i16, _angle_2: i16, angle_3: i16) -> Self {
        if angle_3 > 200 {
            Self::Skewed
        } else if angle_1 < 30 {
            Self::VerySharp
        } else if angle_1 < 45 {
            Self::Sharp
        } else {
            Self::Normal
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Junction {
    pub id: i64,
    pub osm_node_id: i64,
    pub lat: f64,
    pub lon: f64,
    pub angle_1: i16,
    pub angle_2: i16,
    pub angle_3: i16,
    /// Bearings (azimuth) of the three roads from the junction node
    /// Each bearing is in degrees (0-360), where 0° is North, 90° is East
    /// Order corresponds to angle_1, angle_2, angle_3
    /// Note: f32 provides sufficient precision (~7 decimal digits) for bearing angles
    /// while optimizing storage size (REAL in PostgreSQL)
    pub bearings: Vec<f32>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

impl Junction {
    pub fn angle_type(&self) -> AngleType {
        let mut angles = [self.angle_1, self.angle_2, self.angle_3];
        angles.sort_unstable();
        AngleType::from_angles(angles[0], angles[1], angles[2])
    }

    pub fn angles(&self) -> [i16; 3] {
        [self.angle_1, self.angle_2, self.angle_3]
    }

    pub fn streetview_url(&self) -> String {
        let base_url = format!(
            "https://www.google.com/maps/@?api=1&map_action=pano&viewpoint={},{}",
            self.lat, self.lon
        );

        if self.bearings.len() == 3 {
            // angles and bearings are in clockwise order
            // angle_1 is between bearings[0] and bearings[1]
            // angle_2 is between bearings[1] and bearings[2]
            // angle_3 is between bearings[2] and bearings[0]

            // Find which angle is minimum
            let angles = [self.angle_1, self.angle_2, self.angle_3];
            let min_angle = *angles.iter().min().unwrap();

            // Determine which two bearings create the minimum angle
            let (b1, b2) = if self.angle_1 == min_angle {
                (self.bearings[0], self.bearings[1])
            } else if self.angle_2 == min_angle {
                (self.bearings[1], self.bearings[2])
            } else {
                (self.bearings[2], self.bearings[0])
            };

            // Calculate heading as the middle direction between the two roads
            let heading = if (b2 - b1).abs() > 180.0 {
                // Wrap around 360 degrees
                let avg = (b1 + b2 + 360.0) / 2.0;
                if avg >= 360.0 {
                    avg - 360.0
                } else {
                    avg
                }
            } else {
                (b1 + b2) / 2.0
            };

            return format!("{}&heading={:.0}", base_url, heading);
        }

        base_url
    }

    pub fn to_feature(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "Feature",
            "geometry": {
                "type": "Point",
                "coordinates": [self.lon, self.lat]
            },
            "properties": {
                "id": self.id,
                "osm_node_id": self.osm_node_id,
                "angles": self.angles(),
                "angle_type": self.angle_type(),
                "streetview_url": self.streetview_url()
            }
        })
    }

    pub fn to_feature_collection(junctions: Vec<Junction>, total_count: i64) -> serde_json::Value {
        let features: Vec<serde_json::Value> = junctions.iter().map(|j| j.to_feature()).collect();

        serde_json::json!({
            "type": "FeatureCollection",
            "features": features,
            "total_count": total_count
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_angle_type_sharp() {
        // angle_1 < 45
        let angle_type = AngleType::from_angles(30, 150, 180);
        assert_eq!(angle_type, AngleType::Sharp);
    }

    #[test]
    fn test_angle_type_verysharp() {
        // angle_1 < 30 AND angle_3 <= 200
        let angle_type = AngleType::from_angles(25, 120, 180);
        assert_eq!(angle_type, AngleType::VerySharp);
    }

    #[test]
    fn test_angle_type_skewed() {
        // angle_3 > 200
        let angle_type = AngleType::from_angles(50, 100, 210);
        assert_eq!(angle_type, AngleType::Skewed);
    }

    #[test]
    fn test_angle_type_normal() {
        // 上記のいずれにも当てはまらない
        let angle_type = AngleType::from_angles(60, 120, 180);
        assert_eq!(angle_type, AngleType::Normal);
    }

    #[test]
    fn test_junction_angle_type() {
        let junction = Junction {
            id: 1,
            osm_node_id: 123456,
            lat: 35.6812,
            lon: 139.7671,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
        };

        assert_eq!(junction.angle_type(), AngleType::Sharp);
    }

    #[test]
    fn test_junction_angles() {
        let junction = Junction {
            id: 1,
            osm_node_id: 123456,
            lat: 35.6812,
            lon: 139.7671,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
        };

        assert_eq!(junction.angles(), [30, 150, 180]);
    }

    #[test]
    fn test_streetview_url() {
        let junction = Junction {
            id: 1,
            osm_node_id: 123456,
            lat: 35.6812,
            lon: 139.7671,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
        };

        let url = junction.streetview_url();
        assert!(url.contains("api=1"));
        assert!(url.contains("map_action=pano"));
        assert!(url.contains("viewpoint=35.6812,139.7671"));
        assert!(url.contains("heading=25"));
    }

    #[test]
    fn test_to_feature() {
        let junction = Junction {
            id: 1,
            osm_node_id: 123456,
            lat: 35.6812,
            lon: 139.7671,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
        };

        let feature = junction.to_feature();

        assert_eq!(feature["type"], "Feature");
        assert_eq!(feature["geometry"]["type"], "Point");
        assert_eq!(feature["geometry"]["coordinates"][0], 139.7671);
        assert_eq!(feature["geometry"]["coordinates"][1], 35.6812);
        assert_eq!(feature["properties"]["id"], 1);
        assert_eq!(feature["properties"]["osm_node_id"], 123456);
        assert_eq!(feature["properties"]["angle_type"], "sharp");
        assert_eq!(
            feature["properties"]["angles"],
            serde_json::json!([30, 150, 180])
        );
    }

    #[test]
    fn test_to_feature_collection() {
        let junction1 = Junction {
            id: 1,
            osm_node_id: 123456,
            lat: 35.6812,
            lon: 139.7671,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
        };

        let junction2 = Junction {
            id: 2,
            osm_node_id: 654321,
            lat: 35.6900,
            lon: 139.7700,
            angle_1: 110,
            angle_2: 120,
            angle_3: 130,
            bearings: vec![50.0, 160.0, 280.0],
            created_at: Utc::now(),
        };

        let collection = Junction::to_feature_collection(vec![junction1, junction2], 2);

        assert_eq!(collection["type"], "FeatureCollection");
        assert_eq!(collection["total_count"], 2);
        assert_eq!(collection["features"].as_array().unwrap().len(), 2);
        assert_eq!(collection["features"][0]["properties"]["id"], 1);
        assert_eq!(collection["features"][1]["properties"]["id"], 2);
    }
}
