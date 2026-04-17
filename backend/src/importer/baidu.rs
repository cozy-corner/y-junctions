use crate::domain::china::{self, BaiduPanorama};
use anyhow::Result;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const ENDPOINT: &str = "https://mapsv0.bdimg.com/";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";
const DISTANCE_LIMIT_METERS: f64 = 10.0;

#[derive(Debug, Deserialize)]
struct QsdataResponse {
    content: Option<QsdataContent>,
    result: Option<QsdataResult>,
}

#[derive(Debug, Deserialize)]
struct QsdataContent {
    id: String,
    x: i64,
    y: i64,
}

#[derive(Debug, Deserialize)]
struct QsdataResult {
    error: i64,
}

/// Build the default HTTP client used for panorama queries. 5s timeout with
/// a browser User-Agent (the unofficial endpoint rejects bare reqwest UAs).
pub fn build_client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(USER_AGENT)
        .build()?)
}

/// Query Baidu's `qsdata` endpoint for the nearest panorama to the given
/// WGS84 coordinate. Returns `None` when Baidu reports no coverage or when
/// the nearest panorama sits farther than `DISTANCE_LIMIT_METERS` from the
/// junction (ground meters, `cos(lat)`-corrected). One retry on transient
/// transport errors; API-level "not found" is not retried.
pub async fn fetch_nearest_panorama(
    client: &Client,
    lng: f64,
    lat: f64,
) -> Result<Option<BaiduPanorama>> {
    let (query_mc_x, query_mc_y) = china::wgs84_to_bd09mc(lng, lat);

    let mut last_err = None;
    for attempt in 0..2 {
        match request_qsdata(client, query_mc_x, query_mc_y).await {
            Ok(resp) => return Ok(process_response(resp, query_mc_x, query_mc_y, lat)),
            Err(e) => {
                last_err = Some(e);
                if attempt == 0 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }
    Err(last_err.unwrap())
}

async fn request_qsdata(client: &Client, mc_x: f64, mc_y: f64) -> Result<QsdataResponse> {
    let resp = client
        .get(ENDPOINT)
        .query(&[
            ("qt", "qsdata".to_string()),
            ("x", format!("{:.0}", mc_x)),
            ("y", format!("{:.0}", mc_y)),
            ("l", "17".to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<QsdataResponse>()
        .await?;
    Ok(resp)
}

fn process_response(
    resp: QsdataResponse,
    query_mc_x: f64,
    query_mc_y: f64,
    lat: f64,
) -> Option<BaiduPanorama> {
    if let Some(result) = resp.result {
        if result.error != 0 {
            return None;
        }
    }

    let content = resp.content?;
    let pano_mc_x = content.x as f64 / 100.0;
    let pano_mc_y = content.y as f64 / 100.0;

    // Mercator distance → ground meters via cos(lat) correction. The unofficial
    // endpoint occasionally returns a panorama from a neighbouring street; the
    // 10 m cap filters out those place-shifts.
    let dx = query_mc_x - pano_mc_x;
    let dy = query_mc_y - pano_mc_y;
    let mc_distance = (dx * dx + dy * dy).sqrt();
    let ground_distance = mc_distance * lat.to_radians().cos();
    if ground_distance > DISTANCE_LIMIT_METERS {
        return None;
    }

    Some(BaiduPanorama {
        panoid: content.id,
        pano_mc_x,
        pano_mc_y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp_with(id: &str, x: i64, y: i64, error: i64) -> QsdataResponse {
        QsdataResponse {
            content: Some(QsdataContent {
                id: id.to_string(),
                x,
                y,
            }),
            result: Some(QsdataResult { error }),
        }
    }

    #[test]
    fn api_level_error_returns_none() {
        let (mc_x, mc_y) = china::wgs84_to_bd09mc(121.4737, 31.2304);
        let resp = QsdataResponse {
            content: None,
            result: Some(QsdataResult { error: 404 }),
        };
        assert!(process_response(resp, mc_x, mc_y, 31.2304).is_none());
    }

    #[test]
    fn within_ten_meters_accepted() {
        // Place pano essentially at the query point so distance ≈ 0.
        let (mc_x, mc_y) = china::wgs84_to_bd09mc(121.4737, 31.2304);
        let resp = resp_with(
            "PANO_ID_A",
            (mc_x * 100.0).round() as i64,
            (mc_y * 100.0).round() as i64,
            0,
        );
        let out = process_response(resp, mc_x, mc_y, 31.2304);
        assert!(out.is_some());
        let p = out.unwrap();
        assert_eq!(p.panoid, "PANO_ID_A");
    }

    #[test]
    fn beyond_ten_meters_rejected() {
        let (mc_x, mc_y) = china::wgs84_to_bd09mc(121.4737, 31.2304);
        // Shift pano by ~20 m in MC (Shanghai cos(lat) ≈ 0.857 → 20 MC ≈ 17 m ground).
        let resp = resp_with(
            "PANO_ID_FAR",
            ((mc_x + 20.0) * 100.0).round() as i64,
            (mc_y * 100.0).round() as i64,
            0,
        );
        assert!(process_response(resp, mc_x, mc_y, 31.2304).is_none());
    }

    #[test]
    fn missing_content_returns_none() {
        let (mc_x, mc_y) = china::wgs84_to_bd09mc(121.4737, 31.2304);
        let resp = QsdataResponse {
            content: None,
            result: Some(QsdataResult { error: 0 }),
        };
        assert!(process_response(resp, mc_x, mc_y, 31.2304).is_none());
    }
}
