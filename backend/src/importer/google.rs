use anyhow::Result;
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

const ENDPOINT: &str = "https://maps.googleapis.com/maps/api/streetview/metadata";
const API_KEY_ENV: &str = "GOOGLE_MAPS_API_KEY";

// The default search radius is 50 m, which reports OK for a panorama on the
// next street over. 10 m makes `OK` mean "there is a panorama at this
// junction", which is what the map needs.
const SEARCH_RADIUS_METERS: u32 = 10;

const MAX_ATTEMPTS: usize = 3;
const TRANSIENT_RETRY_SLEEP: Duration = Duration::from_millis(500);
const DEFAULT_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(5);
const MAX_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(60);

// Sequential requests already cap out at a few per second, well under the
// metadata QPS limit; this is just a courtesy floor between calls.
const PACING: Duration = Duration::from_millis(50);

#[derive(Debug)]
enum FetchError {
    RateLimited {
        retry_after: Option<Duration>,
    },
    /// Bad key, missing permission, malformed request — retrying cannot help
    /// and treating it as "no coverage" would write false for every node, so
    /// abort the batch immediately.
    Fatal(anyhow::Error),
    Transient(anyhow::Error),
}

impl FetchError {
    fn into_anyhow(self) -> anyhow::Error {
        match self {
            Self::RateLimited { retry_after } => anyhow::anyhow!(
                "Street View metadata rate-limited (retry-after={}); back off before retrying the batch",
                retry_after
                    .map(|d| format!("{}s", d.as_secs()))
                    .unwrap_or_else(|| "unspecified".to_string())
            ),
            Self::Fatal(e) | Self::Transient(e) => e,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MetadataResponse {
    status: String,
}

/// What a `status` value means for the coverage cache.
#[derive(Debug, PartialEq, Eq)]
enum Classification {
    /// A panorama exists within `SEARCH_RADIUS_METERS`.
    Covered,
    /// Queried successfully, no panorama nearby. Only this writes `false`.
    Absent,
    Transient,
    Fatal,
}

/// Read the Street View API key. Erroring out here (rather than sending
/// keyless requests, which come back `REQUEST_DENIED`) keeps a forgotten
/// `.env` from looking like a genuine coverage answer.
pub fn api_key_from_env() -> Result<String> {
    let key = std::env::var(API_KEY_ENV)
        .map_err(|_| anyhow::anyhow!("{API_KEY_ENV} must be set in environment or .env file"))?;
    if key.trim().is_empty() {
        anyhow::bail!("{API_KEY_ENV} is set but empty");
    }
    Ok(key)
}

pub fn build_client() -> Result<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(10)).build()?)
}

/// Sleep between successive `fetch_coverage` calls (not before the first).
pub async fn pace_next_request() {
    tokio::time::sleep(PACING).await;
}

/// Ask the Street View Static metadata endpoint whether a panorama exists
/// within 10 m of the coordinate. Metadata requests are free — no imagery is
/// fetched. Retries transient failures up to `MAX_ATTEMPTS`, backs off on
/// `OVER_QUERY_LIMIT`/429/5xx, and fails fast on `REQUEST_DENIED` so a broken
/// key never gets recorded as "no coverage".
pub async fn fetch_coverage(client: &Client, api_key: &str, lng: f64, lat: f64) -> Result<bool> {
    let mut last_err: Option<FetchError> = None;

    for attempt in 0..MAX_ATTEMPTS {
        match request_metadata(client, api_key, lng, lat).await {
            Ok(covered) => return Ok(covered),
            Err(FetchError::Fatal(e)) => return Err(e),
            Err(FetchError::RateLimited { retry_after }) => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(FetchError::RateLimited { retry_after }.into_anyhow());
                }
                let sleep_dur = retry_after
                    .unwrap_or(DEFAULT_RATE_LIMIT_SLEEP)
                    .min(MAX_RATE_LIMIT_SLEEP);
                tracing::warn!(
                    "Street View metadata rate-limited; sleeping {:?} before retry {}/{}",
                    sleep_dur,
                    attempt + 2,
                    MAX_ATTEMPTS
                );
                tokio::time::sleep(sleep_dur).await;
                last_err = Some(FetchError::RateLimited { retry_after });
            }
            Err(FetchError::Transient(e)) => {
                last_err = Some(FetchError::Transient(e));
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(TRANSIENT_RETRY_SLEEP).await;
                }
            }
        }
    }

    Err(last_err.unwrap().into_anyhow())
}

async fn request_metadata(
    client: &Client,
    api_key: &str,
    lng: f64,
    lat: f64,
) -> std::result::Result<bool, FetchError> {
    let resp = client
        .get(ENDPOINT)
        .query(&[
            ("location", format!("{lat},{lng}")),
            ("radius", SEARCH_RADIUS_METERS.to_string()),
            // Skip indoor collections; only street-level panoramas count.
            ("source", "outdoor".to_string()),
            ("key", api_key.to_string()),
        ])
        .send()
        .await
        // Errors carry the request URL, which holds the API key — strip it
        // before it reaches a log line.
        .map_err(|e| FetchError::Transient(e.without_url().into()))?;

    let status = resp.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        let retry_after = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        return Err(FetchError::RateLimited { retry_after });
    }
    if let Err(e) = resp.error_for_status_ref() {
        return Err(FetchError::Fatal(
            anyhow::Error::new(e.without_url()).context("Street View metadata request rejected"),
        ));
    }

    let body: MetadataResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Transient(e.without_url().into()))?;

    match classify_status(&body.status) {
        Classification::Covered => Ok(true),
        Classification::Absent => Ok(false),
        Classification::Transient => Err(FetchError::RateLimited { retry_after: None }),
        Classification::Fatal => Err(FetchError::Fatal(anyhow::anyhow!(
            "Street View metadata returned status={} — check {API_KEY_ENV} and API enablement",
            body.status
        ))),
    }
}

/// Map a documented `status` value onto a cache decision.
/// <https://developers.google.com/maps/documentation/streetview/metadata>
fn classify_status(status: &str) -> Classification {
    match status {
        "OK" => Classification::Covered,
        "ZERO_RESULTS" | "NOT_FOUND" => Classification::Absent,
        // Documented as "exceeded your daily quota or per-second quota" and
        // "couldn't be processed due to a server error" — both worth a retry,
        // and neither ever writes has_coverage = false.
        "OVER_QUERY_LIMIT" | "UNKNOWN_ERROR" => Classification::Transient,
        // REQUEST_DENIED / INVALID_REQUEST and anything undocumented: never
        // guess "no coverage" from a status we do not understand.
        _ => Classification::Fatal,
    }
}

/// Parse the `delta-seconds` form of `Retry-After` (e.g. `"30"`). The
/// HTTP-date form is intentionally unhandled; callers fall back to
/// `DEFAULT_RATE_LIMIT_SLEEP` on None.
fn parse_retry_after(value: &str) -> Option<Duration> {
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_means_covered() {
        assert_eq!(classify_status("OK"), Classification::Covered);
    }

    #[test]
    fn zero_results_and_not_found_mean_absent() {
        assert_eq!(classify_status("ZERO_RESULTS"), Classification::Absent);
        assert_eq!(classify_status("NOT_FOUND"), Classification::Absent);
    }

    #[test]
    fn quota_and_server_errors_are_transient() {
        assert_eq!(
            classify_status("OVER_QUERY_LIMIT"),
            Classification::Transient
        );
        assert_eq!(classify_status("UNKNOWN_ERROR"), Classification::Transient);
    }

    #[test]
    fn denied_and_undocumented_statuses_are_fatal() {
        assert_eq!(classify_status("REQUEST_DENIED"), Classification::Fatal);
        assert_eq!(classify_status("INVALID_REQUEST"), Classification::Fatal);
        assert_eq!(classify_status(""), Classification::Fatal);
        // Lower-case is not a documented spelling; must not read as absent.
        assert_eq!(classify_status("zero_results"), Classification::Fatal);
    }

    #[test]
    fn status_parses_from_metadata_json() {
        let body: MetadataResponse = serde_json::from_str(
            r#"{"copyright":"© Google","date":"2021-05","location":{"lat":35.0,"lng":139.0},
                "pano_id":"abc","status":"OK"}"#,
        )
        .expect("metadata body should deserialize");
        assert_eq!(classify_status(&body.status), Classification::Covered);
    }

    #[test]
    fn zero_results_body_has_only_status() {
        let body: MetadataResponse =
            serde_json::from_str(r#"{"status":"ZERO_RESULTS"}"#).expect("should deserialize");
        assert_eq!(classify_status(&body.status), Classification::Absent);
    }

    #[test]
    fn retry_after_parses_integer_seconds() {
        assert_eq!(parse_retry_after("30"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("  15  "), Some(Duration::from_secs(15)));
    }

    #[test]
    fn retry_after_rejects_non_numeric() {
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("-5"), None);
    }

    #[test]
    fn api_key_from_env_rejects_missing_and_empty() {
        // Serialized within this test only: no other test touches this var.
        let saved = std::env::var(API_KEY_ENV).ok();

        std::env::remove_var(API_KEY_ENV);
        assert!(api_key_from_env().is_err(), "missing key must error");

        std::env::set_var(API_KEY_ENV, "   ");
        assert!(api_key_from_env().is_err(), "blank key must error");

        std::env::set_var(API_KEY_ENV, "test-key");
        assert_eq!(api_key_from_env().unwrap(), "test-key");

        match saved {
            Some(v) => std::env::set_var(API_KEY_ENV, v),
            None => std::env::remove_var(API_KEY_ENV),
        }
    }
}
