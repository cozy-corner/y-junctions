use crate::domain::Junction;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Panorama metadata fetched from Baidu's `qsdata` endpoint. Doubles as the
/// shape persisted in `y_junctions.baidu_*` columns.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaiduPanorama {
    pub panoid: String,
    pub pano_mc_x: f64,
    pub pano_mc_y: f64,
}

/// Returns true for coordinates inside Chinese mainland where Google/Bing
/// Street View are unavailable and Baidu panorama is the only viable option.
/// Hong Kong / Macao / Taiwan are excluded because Google Street View covers
/// them. Edge cases on the border degrade gracefully to "no street-view link".
pub fn is_in_china_mainland(lng: f64, lat: f64) -> bool {
    if !(73.0..=135.0).contains(&lng) || !(18.0..=54.0).contains(&lat) {
        return false;
    }

    // Hong Kong
    if (113.8..=114.5).contains(&lng) && (22.1..=22.6).contains(&lat) {
        return false;
    }
    // Macao
    if (113.5..=113.6).contains(&lng) && (22.1..=22.2).contains(&lat) {
        return false;
    }
    // Taiwan
    if (119.3..=122.0).contains(&lng) && (21.9..=25.3).contains(&lat) {
        return false;
    }

    true
}

const GCJ_A: f64 = 6378245.0;
const GCJ_EE: f64 = 0.006_693_421_622_965_943;

fn gcj_transform_lat(x: f64, y: f64) -> f64 {
    let mut ret = -100.0 + 2.0 * x + 3.0 * y + 0.2 * y * y + 0.1 * x * y + 0.2 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (y * PI).sin() + 40.0 * (y / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (160.0 * (y / 12.0 * PI).sin() + 320.0 * (y * PI / 30.0).sin()) * 2.0 / 3.0;
    ret
}

fn gcj_transform_lng(x: f64, y: f64) -> f64 {
    let mut ret = 300.0 + x + 2.0 * y + 0.1 * x * x + 0.1 * x * y + 0.1 * x.abs().sqrt();
    ret += (20.0 * (6.0 * x * PI).sin() + 20.0 * (2.0 * x * PI).sin()) * 2.0 / 3.0;
    ret += (20.0 * (x * PI).sin() + 40.0 * (x / 3.0 * PI).sin()) * 2.0 / 3.0;
    ret += (150.0 * (x / 12.0 * PI).sin() + 300.0 * (x / 30.0 * PI).sin()) * 2.0 / 3.0;
    ret
}

/// WGS84 (GPS) → GCJ-02 (Chinese national standard, aka "Mars coordinates").
pub fn wgs84_to_gcj02(lng: f64, lat: f64) -> (f64, f64) {
    let x = lng - 105.0;
    let y = lat - 35.0;
    let mut d_lat = gcj_transform_lat(x, y);
    let mut d_lng = gcj_transform_lng(x, y);
    let rad_lat = lat.to_radians();
    let magic = 1.0 - GCJ_EE * rad_lat.sin() * rad_lat.sin();
    let sqrt_magic = magic.sqrt();
    d_lat = (d_lat * 180.0) / ((GCJ_A * (1.0 - GCJ_EE)) / (magic * sqrt_magic) * PI);
    d_lng = (d_lng * 180.0) / (GCJ_A / sqrt_magic * rad_lat.cos() * PI);
    (lng + d_lng, lat + d_lat)
}

/// GCJ-02 → BD-09ll (Baidu lat/lon form).
pub fn gcj02_to_bd09ll(lng: f64, lat: f64) -> (f64, f64) {
    let x_pi = PI * 3000.0 / 180.0;
    let z = (lng * lng + lat * lat).sqrt() + 0.00002 * (lat * x_pi).sin();
    let theta = lat.atan2(lng) + 0.000003 * (lng * x_pi).cos();
    (z * theta.cos() + 0.0065, z * theta.sin() + 0.006)
}

// Baidu's published lookup-table Mercator. `LLBAND` is the latitude break list;
// each row of `LL2MC` is a 10-term polynomial for (lng, lat/abs-scale).
const LL_BAND: [f64; 6] = [75.0, 60.0, 45.0, 30.0, 15.0, 0.0];
#[allow(clippy::inconsistent_digit_grouping)]
const LL2MC: [[f64; 10]; 6] = [
    [
        -0.0015702102444,
        111320.7020616939,
        1_704_480_524_535_203.0,
        -10_338_987_376_042_340.0,
        26_112_667_856_603_880.0,
        -35_149_669_176_653_700.0,
        26_595_700_718_403_920.0,
        -10_725_012_454_188_240.0,
        1_800_819_912_950_474.0,
        82.5,
    ],
    [
        0.0008277824516172526,
        111320.7020463578,
        647_795_574.6671607,
        -4_082_003_173.641316,
        10_774_905_663.51142,
        -15_171_875_531.51559,
        12_053_065_338.62167,
        -5_124_939_663.577472,
        913_311_935.9512032,
        67.5,
    ],
    [
        0.00337398766765,
        111320.7020202162,
        4_481_351.045890365,
        -23_393_751.19931662,
        79_682_215.47186455,
        -115_964_993.2797253,
        97_236_711.15602145,
        -43_661_946.33752821,
        8_477_230.501135234,
        52.5,
    ],
    [
        0.00220636496208,
        111320.7020209128,
        51751.86112841131,
        3_796_837.749470245,
        992013.7397791013,
        -1_221_952.21711287,
        1_340_652.697009075,
        -620_943.6990984312,
        144_416.9293806241,
        37.5,
    ],
    [
        -0.0003441963504368392,
        111320.7020576856,
        278.2353980772752,
        2_485_758.690035394,
        6070.750963243378,
        54821.18345352118,
        9540.606633304236,
        -2710.55326746645,
        1405.483844121726,
        22.5,
    ],
    [
        -0.0003218135878613132,
        111320.7020701615,
        0.00369383431289,
        823_725.6402795718,
        0.46104986909093,
        2351.343141331292,
        1.58060784298199,
        8.77738589078284,
        0.37238884252424,
        7.45,
    ],
];

/// BD-09ll → BD-09 Mercator (meters). Uses Baidu's published lookup tables.
pub fn bd09ll_to_bd09mc(lng: f64, lat: f64) -> (f64, f64) {
    let abs_lat = lat.abs();
    let band_index = LL_BAND
        .iter()
        .position(|&b| abs_lat >= b)
        .unwrap_or(LL_BAND.len() - 1);
    let ce = &LL2MC[band_index];

    let x_abs = ce[0] + ce[1] * lng.abs();
    let c = abs_lat / ce[9];
    let y_abs = ce[2]
        + ce[3] * c
        + ce[4] * c.powi(2)
        + ce[5] * c.powi(3)
        + ce[6] * c.powi(4)
        + ce[7] * c.powi(5)
        + ce[8] * c.powi(6);

    let x = if lng >= 0.0 { x_abs } else { -x_abs };
    let y = if lat >= 0.0 { y_abs } else { -y_abs };
    (x, y)
}

/// Convenience: WGS84 → BD-09 MC in one call.
pub fn wgs84_to_bd09mc(lng: f64, lat: f64) -> (f64, f64) {
    let (g_lng, g_lat) = wgs84_to_gcj02(lng, lat);
    let (bd_lng, bd_lat) = gcj02_to_bd09ll(g_lng, g_lat);
    bd09ll_to_bd09mc(bd_lng, bd_lat)
}

/// Compass bearing (0 = north, clockwise) from the panorama point to the
/// junction, computed directly in BD-09 MC space. For distances ≤ 10 m the
/// Mercator scale error is negligible vs. Haversine.
pub fn compute_baidu_heading(
    pano_mc_x: f64,
    pano_mc_y: f64,
    junction_mc_x: f64,
    junction_mc_y: f64,
) -> f64 {
    let dx = junction_mc_x - pano_mc_x;
    let dy = junction_mc_y - pano_mc_y;
    let bearing = dx.atan2(dy).to_degrees();
    if bearing < 0.0 {
        bearing + 360.0
    } else {
        bearing
    }
}

/// Assemble a Baidu panorama deep-link that opens the junction view with the
/// camera pointed at the Y-junction.
pub fn baidu_panorama_url(panorama: &BaiduPanorama, junction: &Junction) -> String {
    let (junction_mc_x, junction_mc_y) = wgs84_to_bd09mc(junction.lon, junction.lat);
    let heading = compute_baidu_heading(
        panorama.pano_mc_x,
        panorama.pano_mc_y,
        junction_mc_x,
        junction_mc_y,
    );

    format!(
        "https://map.baidu.com/@{:.2},{:.2},21z#panotype=street&pid={}&panoid={}&heading={:.0}&pitch=0&l=21&tn=B_NORMAL_MAP&sc=0&newmap=1&shareurl=1",
        junction_mc_x, junction_mc_y, panorama.panoid, panorama.panoid, heading
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn mk_junction(lat: f64, lon: f64) -> Junction {
        Junction {
            id: 1,
            osm_node_id: 1,
            lat,
            lon,
            angle_1: 30,
            angle_2: 150,
            angle_3: 180,
            bearings: vec![0.0, 120.0, 240.0],
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
    fn shanghai_and_beijing_are_china_mainland() {
        assert!(is_in_china_mainland(121.4737, 31.2304)); // Shanghai
        assert!(is_in_china_mainland(116.3975, 39.9087)); // Beijing
        assert!(is_in_china_mainland(113.2644, 23.1291)); // Guangzhou
    }

    #[test]
    fn tokyo_is_not_china_mainland() {
        assert!(!is_in_china_mainland(139.7671, 35.6812));
    }

    #[test]
    fn hong_kong_macao_taiwan_are_excluded() {
        assert!(!is_in_china_mainland(114.1694, 22.3193)); // Hong Kong
        assert!(!is_in_china_mainland(113.5439, 22.1987)); // Macao
        assert!(!is_in_china_mainland(121.5654, 25.0330)); // Taipei
    }

    #[test]
    fn outside_latlng_bounds_is_false() {
        assert!(!is_in_china_mainland(0.0, 0.0));
        assert!(!is_in_china_mainland(200.0, 40.0));
    }

    #[test]
    fn wgs84_to_gcj02_offset_is_small() {
        // GCJ-02 shifts WGS84 by tens of meters; in degrees this is <0.01.
        let (lng, lat) = wgs84_to_gcj02(121.4737, 31.2304);
        assert!((lng - 121.4737).abs() < 0.01);
        assert!((lat - 31.2304).abs() < 0.01);
        assert_ne!(lng, 121.4737);
        assert_ne!(lat, 31.2304);
    }

    #[test]
    fn gcj02_to_bd09ll_shifts_by_known_constants() {
        // BD-09 offsets from GCJ-02 are roughly +0.0065 lng, +0.006 lat.
        let (lng, lat) = gcj02_to_bd09ll(121.0, 31.0);
        assert!((lng - 121.0 - 0.0065).abs() < 0.01);
        assert!((lat - 31.0 - 0.006).abs() < 0.01);
    }

    #[test]
    fn bd09ll_to_bd09mc_produces_expected_magnitudes() {
        // Shanghai BD-09ll ≈ (121.48, 31.24); MC should be ~13.5M east, ~3.6M north.
        let (mc_x, mc_y) = bd09ll_to_bd09mc(121.48, 31.24);
        assert!(mc_x > 13_000_000.0 && mc_x < 14_000_000.0);
        assert!(mc_y > 3_500_000.0 && mc_y < 3_800_000.0);
    }

    #[test]
    fn wgs84_to_bd09mc_is_composition_of_stages() {
        let (mc_x, mc_y) = wgs84_to_bd09mc(121.4737, 31.2304);
        let (g_lng, g_lat) = wgs84_to_gcj02(121.4737, 31.2304);
        let (b_lng, b_lat) = gcj02_to_bd09ll(g_lng, g_lat);
        let (expected_x, expected_y) = bd09ll_to_bd09mc(b_lng, b_lat);
        assert!((mc_x - expected_x).abs() < 1e-6);
        assert!((mc_y - expected_y).abs() < 1e-6);
    }

    #[test]
    fn compute_baidu_heading_north() {
        // Junction is north of pano (+y in MC).
        let h = compute_baidu_heading(0.0, 0.0, 0.0, 100.0);
        assert!((h - 0.0).abs() < 1e-6 || (h - 360.0).abs() < 1e-6);
    }

    #[test]
    fn compute_baidu_heading_east() {
        let h = compute_baidu_heading(0.0, 0.0, 100.0, 0.0);
        assert!((h - 90.0).abs() < 1e-6);
    }

    #[test]
    fn compute_baidu_heading_south() {
        let h = compute_baidu_heading(0.0, 0.0, 0.0, -100.0);
        assert!((h - 180.0).abs() < 1e-6);
    }

    #[test]
    fn compute_baidu_heading_west() {
        let h = compute_baidu_heading(0.0, 0.0, -100.0, 0.0);
        assert!((h - 270.0).abs() < 1e-6);
    }

    #[test]
    fn baidu_panorama_url_contains_key_parts() {
        let panorama = BaiduPanorama {
            panoid: "09000300122101111452570287I".to_string(),
            pano_mc_x: 13_523_770.0,
            pano_mc_y: 3_640_859.0,
        };
        let junction = mk_junction(31.2304, 121.4737);
        let url = baidu_panorama_url(&panorama, &junction);
        assert!(url.starts_with("https://map.baidu.com/@"));
        assert!(url.contains("pid=09000300122101111452570287I"));
        assert!(url.contains("panoid=09000300122101111452570287I"));
        assert!(url.contains("panotype=street"));
        assert!(url.contains("heading="));
    }
}
