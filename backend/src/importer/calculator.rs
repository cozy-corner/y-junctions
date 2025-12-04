use geo::{HaversineBearing, Point};

/// Calculate the bearing (azimuth) from point1 to point2
/// Returns bearing in degrees (0-360), where 0° is North, 90° is East
fn calculate_bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let point1: Point<f64> = Point::new(lon1, lat1);
    let point2: Point<f64> = Point::new(lon2, lat2);

    let bearing = point1.haversine_bearing(point2);

    // Normalize to 0-360 range
    if bearing < 0.0 {
        bearing + 360.0
    } else {
        bearing
    }
}

/// Calculate the three angles and bearings at a Y-junction
/// Returns angles and bearings in clockwise order (not sorted by angle size)
///
/// # Arguments
/// * `center_lat`, `center_lon` - Coordinates of the Y-junction node
/// * `points` - List of (lat, lon) coordinates of neighboring nodes (should be exactly 3)
///
/// # Returns
/// * `Some((angles, bearings))` if successful
///   - `angles`: [angle1, angle2, angle3] in clockwise order
///   - `bearings`: [bearing1, bearing2, bearing3] in clockwise order
///   - angle1 is between bearings[0] and bearings[1]
///   - angle2 is between bearings[1] and bearings[2]
///   - angle3 is between bearings[2] and bearings[0]
/// * `None` if input is invalid
pub fn calculate_junction_angles(
    center_lat: f64,
    center_lon: f64,
    points: &[(f64, f64)],
) -> Option<([i16; 3], [f64; 3])> {
    if points.len() != 3 {
        return None;
    }

    // Calculate bearings from center to each neighboring point
    let mut bearings: Vec<f64> = points
        .iter()
        .map(|&(lat, lon)| calculate_bearing(center_lat, center_lon, lat, lon))
        .collect();

    // Sort bearings to ensure clockwise order
    bearings.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Calculate angles between consecutive bearings (clockwise order)
    let angle1 = bearings[1] - bearings[0];
    let angle2 = bearings[2] - bearings[1];
    let angle3 = 360.0 - bearings[2] + bearings[0];

    let angles = [
        angle1.round() as i16,
        angle2.round() as i16,
        angle3.round() as i16,
    ];

    let bearings_array = [bearings[0], bearings[1], bearings[2]];

    Some((angles, bearings_array))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Arbitrary test coordinates (Tokyo area)
    const CENTER_LAT: f64 = 35.0;
    const CENTER_LON: f64 = 139.0;

    // Tolerances
    const BEARING_TOLERANCE_DEGREES: f64 = 1.0;
    const ANGLE_SUM_TOLERANCE_DEGREES: i32 = 2;
    const SHARP_ANGLE_THRESHOLD: i16 = 45;

    // Cardinal directions in degrees
    const NORTH: f64 = 0.0;
    const EAST: f64 = 90.0;
    const SOUTH: f64 = 180.0;
    const WEST: f64 = 270.0;

    // Distance offsets for test points (approximately 1km and 0.1km)
    const LAT_OFFSET_LARGE: f64 = 1.0;
    const LAT_OFFSET_SMALL: f64 = 0.001;
    const LON_OFFSET_LARGE: f64 = 1.0;
    const LON_OFFSET_SMALL: f64 = 0.001;

    #[test]
    fn test_calculate_bearing_north() {
        let bearing = calculate_bearing(
            CENTER_LAT,
            CENTER_LON,
            CENTER_LAT + LAT_OFFSET_LARGE,
            CENTER_LON,
        );
        assert!(
            (bearing - NORTH).abs() < BEARING_TOLERANCE_DEGREES,
            "Expected bearing close to {}°, got {}°",
            NORTH,
            bearing
        );
    }

    #[test]
    fn test_calculate_bearing_east() {
        let bearing = calculate_bearing(
            CENTER_LAT,
            CENTER_LON,
            CENTER_LAT,
            CENTER_LON + LON_OFFSET_LARGE,
        );
        assert!(
            (bearing - EAST).abs() < BEARING_TOLERANCE_DEGREES,
            "Expected bearing close to {}°, got {}°",
            EAST,
            bearing
        );
    }

    #[test]
    fn test_calculate_bearing_south() {
        let bearing = calculate_bearing(
            CENTER_LAT,
            CENTER_LON,
            CENTER_LAT - LAT_OFFSET_LARGE,
            CENTER_LON,
        );
        assert!(
            (bearing - SOUTH).abs() < BEARING_TOLERANCE_DEGREES,
            "Expected bearing close to {}°, got {}°",
            SOUTH,
            bearing
        );
    }

    #[test]
    fn test_calculate_bearing_west() {
        let bearing = calculate_bearing(
            CENTER_LAT,
            CENTER_LON,
            CENTER_LAT,
            CENTER_LON - LON_OFFSET_LARGE,
        );
        assert!(
            (bearing - WEST).abs() < BEARING_TOLERANCE_DEGREES,
            "Expected bearing close to {}°, got {}°",
            WEST,
            bearing
        );
    }

    #[test]
    fn test_calculate_junction_angles_sharp() {
        let center = (CENTER_LAT, CENTER_LON);

        // Create 3 points: one north, one slightly northeast, one south
        // This creates a sharp angle (< 45°) between the two northern branches
        let points = vec![
            (CENTER_LAT + LAT_OFFSET_SMALL, CENTER_LON),
            (
                CENTER_LAT + LAT_OFFSET_SMALL * 0.9,
                CENTER_LON + LON_OFFSET_SMALL * 0.1,
            ),
            (CENTER_LAT - LAT_OFFSET_SMALL, CENTER_LON),
        ];

        let result = calculate_junction_angles(center.0, center.1, &points);
        assert!(result.is_some());

        let (angles, bearings) = result.unwrap();

        // The smallest angle should be less than the sharp angle threshold
        assert!(
            angles[0] < SHARP_ANGLE_THRESHOLD,
            "Expected smallest angle < {}°, got {}°",
            SHARP_ANGLE_THRESHOLD,
            angles[0]
        );

        // Check that bearings are in valid range
        for bearing in &bearings {
            assert!(
                *bearing >= 0.0 && *bearing < 360.0,
                "Bearing {} out of range",
                bearing
            );
        }
    }

    #[test]
    fn test_calculate_junction_angles_invalid_input() {
        let center = (CENTER_LAT, CENTER_LON);
        let points = vec![
            (CENTER_LAT + LAT_OFFSET_SMALL, CENTER_LON),
            (CENTER_LAT - LAT_OFFSET_SMALL, CENTER_LON),
        ];

        let result = calculate_junction_angles(center.0, center.1, &points);
        assert!(result.is_none(), "Expected None for invalid input");
    }

    #[test]
    fn test_angles_sum_to_360() {
        let center = (CENTER_LAT, CENTER_LON);
        let points = vec![
            (CENTER_LAT + LAT_OFFSET_SMALL, CENTER_LON),
            (CENTER_LAT, CENTER_LON + LON_OFFSET_SMALL),
            (CENTER_LAT - LAT_OFFSET_SMALL, CENTER_LON - LON_OFFSET_SMALL),
        ];

        let result = calculate_junction_angles(center.0, center.1, &points);
        assert!(result.is_some());

        let (angles, _bearings) = result.unwrap();
        let sum: i16 = angles.iter().sum();

        assert!(
            (sum as i32 - 360).abs() <= ANGLE_SUM_TOLERANCE_DEGREES,
            "Expected angles to sum to 360°, got {}°",
            sum
        );
    }
}
