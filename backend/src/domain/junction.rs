use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AngleType {
    Sharp,
    Even,
    Skewed,
    Normal,
}

impl AngleType {
    pub fn from_angles(angle_1: i16, _angle_2: i16, angle_3: i16) -> Self {
        if angle_1 < 45 {
            Self::Sharp
        } else if angle_1 >= 100 && angle_3 <= 140 {
            Self::Even
        } else if angle_3 > 200 {
            Self::Skewed
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
    pub road_types: Vec<String>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
}

impl Junction {
    pub fn angle_type(&self) -> AngleType {
        AngleType::from_angles(self.angle_1, self.angle_2, self.angle_3)
    }

    pub fn angles(&self) -> [i16; 3] {
        [self.angle_1, self.angle_2, self.angle_3]
    }

    pub fn streetview_url(&self) -> String {
        // 最初の角度を使って向き（heading）を計算
        // 簡易実装: デフォルトの向きを使用
        let heading = 210;
        format!(
            "https://www.google.com/maps/@{},{},3a,75y,{}h,90t",
            self.lat, self.lon, heading
        )
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
                "road_types": self.road_types,
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
    fn test_angle_type_even() {
        // angle_1 >= 100 AND angle_3 <= 140
        let angle_type = AngleType::from_angles(110, 120, 130);
        assert_eq!(angle_type, AngleType::Even);
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
            road_types: vec!["residential".to_string()],
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
            road_types: vec!["residential".to_string()],
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
            road_types: vec!["residential".to_string()],
            created_at: Utc::now(),
        };

        let url = junction.streetview_url();
        assert!(url.contains("35.6812"));
        assert!(url.contains("139.7671"));
        assert!(url.starts_with("https://www.google.com/maps/@"));
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
            road_types: vec!["residential".to_string(), "tertiary".to_string()],
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
            road_types: vec!["residential".to_string()],
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
            road_types: vec!["tertiary".to_string()],
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
