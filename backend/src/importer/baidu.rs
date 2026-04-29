use crate::domain::china::{self, BaiduPanorama};
use anyhow::Result;
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

const ENDPOINT: &str = "https://mapsv0.bdimg.com/";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36";
const DISTANCE_LIMIT_METERS: f64 = 10.0;

const MAX_ATTEMPTS: usize = 3;
const TRANSIENT_RETRY_SLEEP: Duration = Duration::from_millis(500);
const DEFAULT_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(5);
const MAX_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(60);

// Evenly-spaced requests are the most obvious bot signal, so pace inter-
// request sleeps uniformly within this window (≈6.7–12.5 req/s effective,
// mean ~115 ms).
const PACING_MIN: Duration = Duration::from_millis(80);
const PACING_MAX: Duration = Duration::from_millis(150);

#[derive(Debug)]
enum FetchError {
    RateLimited { retry_after: Option<Duration> },
    Other(anyhow::Error),
}

impl FetchError {
    fn into_anyhow(self) -> anyhow::Error {
        match self {
            Self::RateLimited { retry_after } => anyhow::anyhow!(
                "Baidu rate-limited (retry-after={}); back off before retrying the batch",
                retry_after
                    .map(|d| format!("{}s", d.as_secs()))
                    .unwrap_or_else(|| "unspecified".to_string())
            ),
            Self::Other(e) => e,
        }
    }
}

impl<E: Into<anyhow::Error>> From<E> for FetchError {
    fn from(e: E) -> Self {
        Self::Other(e.into())
    }
}

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

/// Sleep a jittered amount before issuing the next panorama request. Call
/// this between successive `fetch_nearest_panorama` invocations (not before
/// the first one).
pub async fn pace_next_request() {
    tokio::time::sleep(pacing_sleep()).await;
}

fn pacing_sleep() -> Duration {
    use rand::RngExt;
    let range_ms = (PACING_MAX - PACING_MIN).as_millis() as u64;
    let extra_ms = rand::rng().random_range(0..=range_ms);
    PACING_MIN + Duration::from_millis(extra_ms)
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
/// junction (ground meters, `cos(lat)`-corrected). Retries up to
/// `MAX_ATTEMPTS` on transient errors; on 429/503 sleeps for `Retry-After`
/// (capped) before the next attempt, and aborts the batch on the final
/// rate-limit hit so operators can back off before re-running.
pub async fn fetch_nearest_panorama(
    client: &Client,
    lng: f64,
    lat: f64,
) -> Result<Option<BaiduPanorama>> {
    let (query_mc_x, query_mc_y) = china::wgs84_to_bd09mc(lng, lat);

    let mut last_err: Option<FetchError> = None;
    for attempt in 0..MAX_ATTEMPTS {
        match request_qsdata(client, query_mc_x, query_mc_y).await {
            Ok(resp) => return Ok(process_response(resp, query_mc_x, query_mc_y, lat)),
            Err(FetchError::RateLimited { retry_after }) => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(FetchError::RateLimited { retry_after }.into_anyhow());
                }
                let sleep_dur = retry_after
                    .unwrap_or(DEFAULT_RATE_LIMIT_SLEEP)
                    .min(MAX_RATE_LIMIT_SLEEP);
                tracing::warn!(
                    "Baidu qsdata rate-limited; sleeping {:?} before retry {}/{}",
                    sleep_dur,
                    attempt + 2,
                    MAX_ATTEMPTS
                );
                tokio::time::sleep(sleep_dur).await;
                last_err = Some(FetchError::RateLimited { retry_after });
            }
            Err(FetchError::Other(e)) => {
                last_err = Some(FetchError::Other(e));
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(TRANSIENT_RETRY_SLEEP).await;
                }
            }
        }
    }
    Err(last_err.unwrap().into_anyhow())
}

async fn request_qsdata(
    client: &Client,
    mc_x: f64,
    mc_y: f64,
) -> std::result::Result<QsdataResponse, FetchError> {
    let resp = client
        .get(ENDPOINT)
        .query(&[
            ("qt", "qsdata".to_string()),
            ("x", format!("{:.0}", mc_x)),
            ("y", format!("{:.0}", mc_y)),
            ("l", "17".to_string()),
        ])
        .send()
        .await?;

    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE {
        let retry_after = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        return Err(FetchError::RateLimited { retry_after });
    }

    let resp = resp.error_for_status()?;
    Ok(resp.json::<QsdataResponse>().await?)
}

/// Parse an HTTP `Retry-After` header value. Supports the `delta-seconds`
/// form (e.g. `"30"`); the HTTP-date form is intentionally not parsed —
/// callers fall back to `DEFAULT_RATE_LIMIT_SLEEP` when this returns None.
fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
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

    #[test]
    fn retry_after_parses_integer_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  15  "), Some(Duration::from_secs(15)));
        assert_eq!(parse_retry_after("0"), Some(Duration::from_secs(0)));
    }

    #[test]
    fn retry_after_rejects_non_numeric() {
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("-5"), None);
        assert_eq!(parse_retry_after("3.5"), None);
    }

    #[test]
    fn pacing_sleep_stays_within_bounds() {
        for _ in 0..1000 {
            let d = pacing_sleep();
            assert!(d >= PACING_MIN, "sleep {:?} below min {:?}", d, PACING_MIN);
            assert!(d <= PACING_MAX, "sleep {:?} above max {:?}", d, PACING_MAX);
        }
    }
}
