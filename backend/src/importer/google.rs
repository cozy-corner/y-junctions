use anyhow::Result;
use reqwest::{header, Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

const ENDPOINT: &str = "https://maps.googleapis.com/maps/api/streetview/metadata";
const API_KEY_ENV: &str = "GOOGLE_MAPS_API_KEY";

// The default search radius is 50 m, which reports OK for a panorama on the
// next street over. Narrowing to 10 m cuts most of those out — but `radius` is
// not a hard cap (see `request_metadata`), so COVERAGE_LIMIT_METERS enforces
// the real distance rule on the returned panorama.
const SEARCH_RADIUS_METERS: u32 = 10;
pub const COVERAGE_LIMIT_METERS: f64 = 10.0;

const MAX_ATTEMPTS: usize = 3;
const TRANSIENT_RETRY_SLEEP: Duration = Duration::from_millis(500);
const DEFAULT_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(5);
const MAX_RATE_LIMIT_SLEEP: Duration = Duration::from_secs(60);

// Rate budget. The documented ceiling is 30,000 queries per minute (= 500 QPS)
// for the Street View Static API. Metadata requests are free and consume no
// image quota, but the per-second limit still applies — hence OVER_QUERY_LIMIT.
//
// Each in-flight worker sleeps PACING before its own request, so one worker
// issues at most 1/PACING = 20 req/s. REQUEST_CONCURRENCY workers therefore
// cannot exceed 20 * 24 = 480 QPS however fast Google answers: a structural
// ceiling below the limit, with the OVER_QUERY_LIMIT backoff as the fallback.
const PACING: Duration = Duration::from_millis(50);
pub const REQUEST_CONCURRENCY: usize = 24;

#[derive(Debug)]
enum FetchError {
    RateLimited {
        retry_after: Option<Duration>,
    },
    /// HTTP 5xx or `UNKNOWN_ERROR`: retryable like a rate limit, but named
    /// separately so the operator isn't sent to check quotas over an outage.
    ServerError {
        status: Option<StatusCode>,
    },
    /// Bad key, missing permission, malformed request — retrying cannot help
    /// and treating it as "no coverage" would write false for every node, so
    /// abort the batch immediately.
    Fatal(anyhow::Error),
    Transient(anyhow::Error),
}

impl FetchError {
    /// How long to wait before the next attempt. Honours `Retry-After` when
    /// Google sent one, capped so a hostile value can't stall the batch.
    fn backoff_sleep(&self) -> Duration {
        match self {
            Self::RateLimited { retry_after } => retry_after
                .unwrap_or(DEFAULT_RATE_LIMIT_SLEEP)
                .min(MAX_RATE_LIMIT_SLEEP),
            _ => DEFAULT_RATE_LIMIT_SLEEP,
        }
    }

    fn into_anyhow(self) -> anyhow::Error {
        match self {
            Self::RateLimited { retry_after } => anyhow::anyhow!(
                "Street View metadata rate-limited (retry-after={}); back off before retrying the batch",
                retry_after
                    .map(|d| format!("{}s", d.as_secs()))
                    .unwrap_or_else(|| "unspecified".to_string())
            ),
            Self::ServerError { status } => anyhow::anyhow!(
                "Street View metadata server error ({}); retry once Google recovers",
                status
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "status=UNKNOWN_ERROR".to_string())
            ),
            Self::Fatal(e) | Self::Transient(e) => e,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MetadataResponse {
    status: String,
    /// Coordinate of the panorama Google actually picked. Present on `OK`.
    location: Option<LatLng>,
}

#[derive(Debug, Deserialize)]
struct LatLng {
    lat: f64,
    lng: f64,
}

/// What a `status` value means for the coverage cache.
#[derive(Debug, PartialEq, Eq)]
enum Classification {
    /// Google found a panorama. The caller still checks how far away it is.
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
    let raw = std::env::var(API_KEY_ENV)
        .map_err(|_| anyhow::anyhow!("{API_KEY_ENV} must be set in environment or .env file"))?;
    validate_api_key(&raw)
}

/// Returns the key with surrounding whitespace removed. A stray newline from
/// pasting `terraform output` would otherwise be sent percent-encoded and come
/// back as `REQUEST_DENIED`, which reads like a permissions problem.
fn validate_api_key(raw: &str) -> Result<String> {
    let key = raw.trim();
    if key.is_empty() {
        anyhow::bail!("{API_KEY_ENV} is set but empty");
    }
    Ok(key.to_string())
}

pub fn build_client() -> Result<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(10)).build()?)
}

/// Throttle one worker ahead of its own `fetch_coverage` call. Called per
/// request rather than between requests, so the rate ceiling holds no matter
/// how the concurrent lookups interleave.
pub async fn pace_request() {
    tokio::time::sleep(PACING).await;
}

/// Coverage verdict for one junction. `Absent` and `TooFar` both mean
/// `has_coverage = false`; they are kept apart so the batch can report how
/// often our own distance rule — rather than Google — did the excluding.
#[derive(Debug, PartialEq)]
pub enum Coverage {
    Covered,
    /// Google reported no panorama near the coordinate.
    Absent,
    /// Google returned a panorama, but farther than `COVERAGE_LIMIT_METERS`.
    TooFar {
        distance_meters: f64,
    },
}

impl Coverage {
    pub fn has_coverage(&self) -> bool {
        matches!(self, Self::Covered)
    }
}

/// Ask the Street View Static metadata endpoint whether a panorama exists
/// within 10 m of the coordinate. Metadata requests are free — no imagery is
/// fetched. Retries transient failures up to `MAX_ATTEMPTS`, backs off on
/// `OVER_QUERY_LIMIT`/429/5xx, and fails fast on `REQUEST_DENIED` so a broken
/// key never gets recorded as "no coverage".
pub async fn fetch_coverage(
    client: &Client,
    api_key: &str,
    lng: f64,
    lat: f64,
) -> Result<Coverage> {
    let mut last_err: Option<FetchError> = None;

    for attempt in 0..MAX_ATTEMPTS {
        match request_metadata(client, api_key, lng, lat).await {
            Ok(coverage) => return Ok(coverage),
            Err(FetchError::Fatal(e)) => return Err(e),
            Err(err @ (FetchError::RateLimited { .. } | FetchError::ServerError { .. })) => {
                if attempt + 1 == MAX_ATTEMPTS {
                    return Err(err.into_anyhow());
                }
                let sleep_dur = err.backoff_sleep();
                tracing::warn!(
                    "{:?}; sleeping {:?} before retry {}/{}",
                    err,
                    sleep_dur,
                    attempt + 2,
                    MAX_ATTEMPTS
                );
                tokio::time::sleep(sleep_dur).await;
                last_err = Some(err);
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
) -> std::result::Result<Coverage, FetchError> {
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
    if status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = resp
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(parse_retry_after);
        return Err(FetchError::RateLimited { retry_after });
    }
    if status.is_server_error() {
        return Err(FetchError::ServerError {
            status: Some(status),
        });
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
        Classification::Covered => {
            // `radius` narrows the search but is not a hard cap: measured
            // against the live API, a query inside the Imperial Palace grounds
            // returns OK at radius=10 with a panorama 483 m away. Re-check the
            // distance ourselves so `true` really means "panorama at this
            // junction" (same guard as baidu.rs).
            let Some(loc) = body.location else {
                return Err(FetchError::Fatal(anyhow::anyhow!(
                    "Street View metadata returned OK without a location; cannot verify distance"
                )));
            };
            let distance = ground_distance_meters(lat, lng, loc.lat, loc.lng);
            if distance > COVERAGE_LIMIT_METERS {
                return Ok(Coverage::TooFar {
                    distance_meters: distance,
                });
            }
            Ok(Coverage::Covered)
        }
        Classification::Absent => Ok(Coverage::Absent),
        Classification::Transient if body.status == "OVER_QUERY_LIMIT" => {
            Err(FetchError::RateLimited { retry_after: None })
        }
        Classification::Transient => Err(FetchError::ServerError { status: None }),
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
        // The only status that means "no panorama here": documented as "no
        // panorama could be found near the provided location". NOT_FOUND is
        // *not* a synonym — it means the address string in `location` couldn't
        // be found, which cannot happen when we only ever send coordinates, so
        // it falls through to Fatal rather than writing a sticky false.
        "ZERO_RESULTS" => Classification::Absent,
        // Documented as "exceeded your daily quota or per-second quota" and
        // "couldn't be processed due to a server error" — both worth a retry,
        // and neither ever writes has_coverage = false.
        "OVER_QUERY_LIMIT" | "UNKNOWN_ERROR" => Classification::Transient,
        // REQUEST_DENIED / INVALID_REQUEST and anything undocumented: never
        // guess "no coverage" from a status we do not understand.
        _ => Classification::Fatal,
    }
}

/// Flat-earth distance in metres between two nearby WGS84 coordinates. Good
/// to well under a metre at the ~10 m scale this is compared against.
fn ground_distance_meters(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    const METERS_PER_DEGREE: f64 = 111_320.0;
    let dy = (lat2 - lat1) * METERS_PER_DEGREE;
    let dx = (lng2 - lng1) * METERS_PER_DEGREE * lat1.to_radians().cos();
    (dx * dx + dy * dy).sqrt()
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
    fn only_zero_results_means_absent() {
        assert_eq!(classify_status("ZERO_RESULTS"), Classification::Absent);
        // NOT_FOUND is about an unresolvable address string, which this batch
        // never sends; recording it as "no coverage" would be a sticky lie.
        assert_eq!(classify_status("NOT_FOUND"), Classification::Fatal);
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
    fn distance_is_zero_for_identical_coordinates() {
        assert!(ground_distance_meters(35.6595, 139.7005, 35.6595, 139.7005) < 1e-9);
    }

    #[test]
    fn distance_matches_known_offsets() {
        // 0.0001° of latitude ≈ 11.1 m anywhere.
        let d = ground_distance_meters(35.6595, 139.7005, 35.6596, 139.7005);
        assert!((11.0..=11.3).contains(&d), "got {d}");
        // Same longitude delta shrinks by cos(lat) ≈ 0.813 at Tokyo.
        let d = ground_distance_meters(35.6595, 139.7005, 35.6595, 139.7006);
        assert!((8.9..=9.2).contains(&d), "got {d}");
    }

    #[test]
    fn far_panorama_exceeds_the_coverage_limit() {
        // The measured Imperial Palace case: OK at radius=10, panorama 483 m
        // away. Must land on the uncovered side of the limit.
        let d = ground_distance_meters(35.6852, 139.7528, 35.68095, 139.75);
        assert!(
            d > COVERAGE_LIMIT_METERS,
            "expected far panorama to exceed limit, got {d}"
        );
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
    fn only_covered_counts_as_coverage() {
        assert!(Coverage::Covered.has_coverage());
        assert!(!Coverage::Absent.has_coverage());
        assert!(!Coverage::TooFar {
            distance_meters: 483.2
        }
        .has_coverage());
    }

    #[test]
    fn api_key_rejects_blank_values() {
        assert!(validate_api_key("").is_err());
        assert!(validate_api_key("   ").is_err());
        assert!(validate_api_key("\n").is_err());
    }

    #[test]
    fn api_key_is_trimmed() {
        // Pasting `terraform output` easily brings a trailing newline along.
        assert_eq!(validate_api_key("AIza-test\n").unwrap(), "AIza-test");
        assert_eq!(validate_api_key("  AIza-test  ").unwrap(), "AIza-test");
    }
}
