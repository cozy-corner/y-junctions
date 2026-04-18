use crate::db::baidu_repository;
use crate::domain::china::{self, BaiduPanorama};
use crate::domain::Junction;
use sqlx::PgPool;

/// Replace `streetview_url` on each feature with a region-appropriate URL.
/// Junctions inside mainland China get a Baidu deep-link; everywhere else
/// keeps the existing Google URL. Mainland-China junctions without a Baidu
/// panorama are dropped from the response — map markers that open to a
/// broken/empty link are worse than not showing the marker at all.
pub async fn enrich_collection(
    pool: &PgPool,
    junctions: Vec<Junction>,
) -> Result<serde_json::Value, sqlx::Error> {
    let ids: Vec<i64> = junctions.iter().map(|j| j.id).collect();
    let baidu_map = baidu_repository::find_by_junction_ids(pool, &ids).await?;

    let features: Vec<serde_json::Value> = junctions
        .iter()
        .filter_map(|j| {
            let baidu = baidu_map.get(&j.id);
            if china::is_in_china_mainland(j.lon, j.lat) && baidu.is_none() {
                return None;
            }
            let mut feature = j.to_feature();
            feature["properties"]["streetview_url"] =
                serde_json::Value::String(build_url(j, baidu));
            Some(feature)
        })
        .collect();

    Ok(serde_json::json!({
        "type": "FeatureCollection",
        "features": features,
        "total_count": features.len() as i64,
    }))
}

/// Single-feature variant used by the `/junctions/:id` and
/// `/junctions/node/:osm_node_id` endpoints.
pub async fn enrich_feature(
    pool: &PgPool,
    junction: Junction,
) -> Result<serde_json::Value, sqlx::Error> {
    let baidu_map = baidu_repository::find_by_junction_ids(pool, &[junction.id]).await?;
    let mut feature = junction.to_feature();
    feature["properties"]["streetview_url"] =
        serde_json::Value::String(build_url(&junction, baidu_map.get(&junction.id)));
    Ok(feature)
}

/// The single branching point between Google / Baidu / empty. Every other
/// file in this crate stays agnostic to the region policy.
fn build_url(junction: &Junction, baidu: Option<&BaiduPanorama>) -> String {
    if china::is_in_china_mainland(junction.lon, junction.lat) {
        baidu
            .map(|b| china::baidu_panorama_url(b, junction))
            .unwrap_or_default()
    } else {
        junction.streetview_url()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn mk_junction(id: i64, lat: f64, lon: f64) -> Junction {
        Junction {
            id,
            osm_node_id: id,
            lat,
            lon,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![10.0, 40.0, 190.0],
            created_at: Utc::now(),
            elevation: None,
            min_elevation_diff: None,
            max_elevation_diff: None,
            min_angle_elevation_diff: None,
            way_1_highway_type: None,
            way_2_highway_type: None,
            way_3_highway_type: None,
            way_1_category: None,
            way_2_category: None,
            way_3_category: None,
        }
    }

    #[test]
    fn non_china_uses_google_url() {
        let j = mk_junction(1, 35.6812, 139.7671);
        let url = build_url(&j, None);
        assert!(url.contains("google.com/maps"));
    }

    #[test]
    fn china_without_panorama_is_empty() {
        let j = mk_junction(2, 31.2304, 121.4737);
        let url = build_url(&j, None);
        assert_eq!(url, "");
    }

    #[test]
    fn china_with_panorama_uses_baidu_url() {
        let j = mk_junction(3, 31.2304, 121.4737);
        let pano = BaiduPanorama {
            panoid: "PANO_TEST".to_string(),
            pano_mc_x: 13_523_770.0,
            pano_mc_y: 3_640_859.0,
        };
        let url = build_url(&j, Some(&pano));
        assert!(url.contains("map.baidu.com"));
        assert!(url.contains("PANO_TEST"));
    }
}
