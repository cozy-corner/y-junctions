use crate::db::{baidu_repository, google_repository};
use crate::domain::china::{self, BaiduPanorama};
use crate::domain::Junction;
use sqlx::PgPool;

/// Replace `streetview_url` on each feature with a region-appropriate URL.
/// Junctions inside mainland China get a Baidu deep-link; everywhere else
/// keeps the existing Google URL. Junctions known to have no panorama are
/// dropped from the response — map markers that open to a broken/empty link
/// are worse than not showing the marker at all. That covers mainland-China
/// junctions without a Baidu panorama, and elsewhere the ones Google's
/// metadata reported as uncovered. Never-queried junctions stay on the map:
/// absence of an answer is not an answer.
pub async fn enrich_collection(
    pool: &PgPool,
    junctions: Vec<Junction>,
) -> Result<serde_json::Value, sqlx::Error> {
    let osm_node_ids: Vec<i64> = junctions.iter().map(|j| j.osm_node_id).collect();
    let baidu_map = baidu_repository::find_by_osm_node_ids(pool, &osm_node_ids).await?;

    // Only non-China junctions consult the Google cache: inside the mainland
    // the Baidu panorama decides, and no coverage row is ever written there.
    let google_ids: Vec<i64> = junctions
        .iter()
        .filter(|j| !china::is_in_china_mainland(j.lon, j.lat))
        .map(|j| j.osm_node_id)
        .collect();
    let coverage_map = google_repository::find_coverage_by_osm_node_ids(pool, &google_ids).await?;

    let features: Vec<serde_json::Value> = junctions
        .iter()
        .filter_map(|j| {
            let baidu = baidu_map.get(&j.osm_node_id);
            let coverage = coverage_map.get(&j.osm_node_id).copied();
            let confirmed_no_panorama = if china::is_in_china_mainland(j.lon, j.lat) {
                baidu.is_none()
            } else {
                coverage == Some(false)
            };
            if confirmed_no_panorama {
                return None;
            }
            let mut feature = j.to_feature();
            feature["properties"]["streetview_url"] =
                serde_json::Value::String(build_url(j, baidu, coverage));
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
///
/// Unlike the collection, an uncovered junction is still returned — these
/// endpoints back direct links, so 404-ing a junction that genuinely exists
/// would be wrong. It comes back with an empty `streetview_url` instead, which
/// the popup already treats as "no button" (`JunctionPopup.tsx`).
pub async fn enrich_feature(
    pool: &PgPool,
    junction: Junction,
) -> Result<serde_json::Value, sqlx::Error> {
    let baidu_map = baidu_repository::find_by_osm_node_ids(pool, &[junction.osm_node_id]).await?;
    let coverage = if china::is_in_china_mainland(junction.lon, junction.lat) {
        None
    } else {
        google_repository::find_coverage_by_osm_node_ids(pool, &[junction.osm_node_id])
            .await?
            .get(&junction.osm_node_id)
            .copied()
    };
    let mut feature = junction.to_feature();
    feature["properties"]["streetview_url"] = serde_json::Value::String(build_url(
        &junction,
        baidu_map.get(&junction.osm_node_id),
        coverage,
    ));
    Ok(feature)
}

/// The single branching point between Google / Baidu / empty. Every other
/// file in this crate stays agnostic to the region policy.
///
/// `google_coverage` is the cached metadata answer for non-China junctions:
/// `Some(false)` means Google confirmed there is no panorama, so emit an empty
/// URL rather than a link that opens onto nothing. `None` (never queried) and
/// `Some(true)` both keep the generated Google URL.
fn build_url(
    junction: &Junction,
    baidu: Option<&BaiduPanorama>,
    google_coverage: Option<bool>,
) -> String {
    if china::is_in_china_mainland(junction.lon, junction.lat) {
        baidu
            .map(|b| china::baidu_panorama_url(b, junction))
            .unwrap_or_default()
    } else if google_coverage == Some(false) {
        String::new()
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
        let url = build_url(&j, None, None);
        assert!(url.contains("google.com/maps"));
    }

    #[test]
    fn china_without_panorama_is_empty() {
        let j = mk_junction(2, 31.2304, 121.4737);
        let url = build_url(&j, None, None);
        assert_eq!(url, "");
    }

    #[test]
    fn non_china_uncovered_is_empty() {
        let j = mk_junction(4, 35.6812, 139.7671);
        assert_eq!(build_url(&j, None, Some(false)), "");
    }

    #[test]
    fn non_china_covered_uses_google_url() {
        let j = mk_junction(5, 35.6812, 139.7671);
        assert!(build_url(&j, None, Some(true)).contains("google.com/maps"));
    }

    #[test]
    fn china_ignores_google_coverage() {
        // A coverage row must never reach a mainland junction, but if one did,
        // the Baidu branch still owns the decision.
        let j = mk_junction(6, 31.2304, 121.4737);
        let pano = BaiduPanorama {
            panoid: "PANO_TEST".to_string(),
            pano_mc_x: 13_523_770.0,
            pano_mc_y: 3_640_859.0,
        };
        assert!(build_url(&j, Some(&pano), Some(false)).contains("map.baidu.com"));
    }

    #[test]
    fn china_with_panorama_uses_baidu_url() {
        let j = mk_junction(3, 31.2304, 121.4737);
        let pano = BaiduPanorama {
            panoid: "PANO_TEST".to_string(),
            pano_mc_x: 13_523_770.0,
            pano_mc_y: 3_640_859.0,
        };
        let url = build_url(&j, Some(&pano), None);
        assert!(url.contains("map.baidu.com"));
        assert!(url.contains("PANO_TEST"));
    }
}
