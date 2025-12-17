use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::Value;
use serial_test::serial;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::sync::atomic::{AtomicI64, Ordering};
use tower::util::ServiceExt;

// テスト用のosm_node_id自動生成
static TEST_OSM_NODE_ID_COUNTER: AtomicI64 = AtomicI64::new(1);

// テストヘルパー: テスト用DBセットアップ
async fn setup_test_db() -> PgPool {
    dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    sqlx::query("TRUNCATE TABLE y_junctions RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate table");

    pool
}

// テスト用データ構造
struct TestJunctionData {
    osm_node_id: i64,
    lat: f64,
    lon: f64,
    angle_1: i16,
    angle_2: i16,
    angle_3: i16,
    bearings: [f32; 3],
    elevation: Option<f64>,
    neighbor_elevations: Option<[f64; 3]>,
    elevation_diffs: Option<[f64; 3]>,
    min_angle_index: Option<i16>,
    min_elevation_diff: Option<f64>,
    max_elevation_diff: Option<f64>,
}

impl TestJunctionData {
    fn sharp_type() -> Self {
        Self {
            osm_node_id: TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            lat: 35.0,
            lon: 139.0,
            angle_1: 35,
            angle_2: 145,
            angle_3: 180,
            bearings: [10.0, 45.0, 190.0],
            elevation: Some(100.0),
            neighbor_elevations: Some([95.0, 105.0, 100.0]),
            elevation_diffs: Some([5.0, 5.0, 0.0]),
            min_angle_index: Some(1),
            min_elevation_diff: Some(0.0),
            max_elevation_diff: Some(5.0),
        }
    }

    fn verysharp_type() -> Self {
        Self {
            osm_node_id: TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            lat: 35.0,
            lon: 139.0,
            angle_1: 20,
            angle_2: 140,
            angle_3: 200,
            bearings: [5.0, 25.0, 165.0],
            elevation: Some(50.0),
            neighbor_elevations: Some([45.0, 55.0, 50.0]),
            elevation_diffs: Some([5.0, 5.0, 0.0]),
            min_angle_index: Some(1),
            min_elevation_diff: Some(0.0),
            max_elevation_diff: Some(5.0),
        }
    }

    fn normal_type() -> Self {
        Self {
            osm_node_id: TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            lat: 35.0,
            lon: 139.0,
            angle_1: 60,
            angle_2: 150,
            angle_3: 150,
            bearings: [30.0, 90.0, 240.0],
            elevation: Some(200.0),
            neighbor_elevations: Some([190.0, 210.0, 200.0]),
            elevation_diffs: Some([10.0, 10.0, 0.0]),
            min_angle_index: Some(1),
            min_elevation_diff: Some(0.0),
            max_elevation_diff: Some(10.0),
        }
    }

    fn with_location(mut self, lat: f64, lon: f64) -> Self {
        self.lat = lat;
        self.lon = lon;
        self
    }
}

// テストヘルパー: テストデータ挿入
async fn insert_test_junction(pool: &PgPool, data: TestJunctionData) -> i64 {
    let rec = sqlx::query_as::<_, (i64,)>(
        r#"
        INSERT INTO y_junctions (
            osm_node_id, location, angle_1, angle_2, angle_3, bearings,
            elevation, neighbor_elevation_1, neighbor_elevation_2, neighbor_elevation_3,
            elevation_diff_1, elevation_diff_2, elevation_diff_3,
            min_angle_index, min_elevation_diff, max_elevation_diff,
            created_at
        )
        VALUES (
            $1, ST_SetSRID(ST_MakePoint($2, $3), 4326), $4, $5, $6, ARRAY[$7, $8, $9],
            $10, $11, $12, $13,
            $14, $15, $16,
            $17, $18, $19,
            NOW()
        )
        RETURNING id
        "#,
    )
    .bind(data.osm_node_id)
    .bind(data.lon)
    .bind(data.lat)
    .bind(data.angle_1)
    .bind(data.angle_2)
    .bind(data.angle_3)
    .bind(data.bearings[0])
    .bind(data.bearings[1])
    .bind(data.bearings[2])
    .bind(data.elevation)
    .bind(data.neighbor_elevations.map(|e| e[0]))
    .bind(data.neighbor_elevations.map(|e| e[1]))
    .bind(data.neighbor_elevations.map(|e| e[2]))
    .bind(data.elevation_diffs.map(|e| e[0]))
    .bind(data.elevation_diffs.map(|e| e[1]))
    .bind(data.elevation_diffs.map(|e| e[2]))
    .bind(data.min_angle_index)
    .bind(data.min_elevation_diff)
    .bind(data.max_elevation_diff)
    .fetch_one(pool)
    .await
    .expect("Failed to insert test junction");

    rec.0
}

// テストヘルパー: アプリケーションのRouterを作成
fn create_test_app(pool: PgPool) -> Router {
    y_junction_backend::api::routes::create_router(pool)
}

// テストヘルパー: HTTPリクエストを送信してレスポンスを取得
async fn send_request(app: Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    (status, json)
}

// ========== GET /api/junctions のテスト（正常系） ==========

#[tokio::test]
#[serial]
async fn test_get_junctions_with_bbox() {
    let pool = setup_test_db().await;

    // bbox範囲内のデータ
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_location(35.0, 139.0),
    )
    .await;
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_location(36.0, 140.0),
    )
    .await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/junctions?bbox=139.0,35.0,140.0,36.0").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["type"], "FeatureCollection");
    assert_eq!(json["total_count"], 2);
    assert_eq!(json["features"].as_array().unwrap().len(), 2);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_angle_type_filter() {
    let pool = setup_test_db().await;

    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::verysharp_type()).await;

    let app = create_test_app(pool);

    // angle_type=sharp でフィルタリング
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&angle_type=sharp",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_min_angle_filter() {
    let pool = setup_test_db().await;

    // angle_1 = 30
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    // angle_1 = 60
    insert_test_junction(&pool, TestJunctionData::normal_type()).await;

    let app = create_test_app(pool);

    // min_angle_lt=50 でフィルタリング（angle_1 < 50）
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&min_angle_lt=50",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_limit() {
    let pool = setup_test_db().await;

    // 3件挿入
    for _ in 0..3 {
        insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    }

    let app = create_test_app(pool);

    // limit=2 で制限
    let (status, json) =
        send_request(app, "/api/junctions?bbox=138.0,34.0,140.0,36.0&limit=2").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 3); // 全体件数
    assert_eq!(json["features"].as_array().unwrap().len(), 2); // 取得件数
}

// ========== GET /api/junctions のテスト（異常系） ==========

#[tokio::test]
#[serial]
async fn test_get_junctions_invalid_bbox_format() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    // bbox のフォーマットが不正（3つのパラメータしかない）
    let (status, json) = send_request(app, "/api/junctions?bbox=139.76,35.68,139.77").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        json["error"],
        "bbox must be in format: min_lon,min_lat,max_lon,max_lat"
    );
}

#[tokio::test]
#[serial]
async fn test_get_junctions_invalid_bbox_range() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    // bbox の範囲が不正（min_lon >= max_lon）
    let (status, json) = send_request(app, "/api/junctions?bbox=140.0,35.0,139.0,36.0").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "Invalid bbox range");
}

#[tokio::test]
#[serial]
async fn test_get_junctions_bbox_out_of_range() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    // bbox が有効範囲外（lon > 180）
    let (status, json) = send_request(app, "/api/junctions?bbox=181.0,35.0,182.0,36.0").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bbox out of valid range");
}

#[tokio::test]
#[serial]
async fn test_get_junctions_invalid_angle_type() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    // angle_type が不正
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=139.0,35.0,140.0,36.0&angle_type=invalid",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "Invalid angle_type");
}

#[tokio::test]
#[serial]
async fn test_get_junctions_invalid_limit() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    // limit が負の数
    let (status, json) =
        send_request(app, "/api/junctions?bbox=139.0,35.0,140.0,36.0&limit=-1").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "limit must be a positive integer");
}

// ========== GET /api/junctions/:id のテスト ==========

#[tokio::test]
#[serial]
async fn test_get_junction_by_id_success() {
    let pool = setup_test_db().await;

    let id = insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["type"], "Feature");
    assert_eq!(json["properties"]["angles"][0], 35);
}

#[tokio::test]
#[serial]
async fn test_get_junction_by_id_not_found() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/junctions/99999").await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "Resource not found");
}

// ========== GET /api/stats のテスト ==========

#[tokio::test]
#[serial]
async fn test_get_stats_with_data() {
    let pool = setup_test_db().await;

    // sharp タイプ × 2
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    // verysharp タイプ × 1
    insert_test_junction(&pool, TestJunctionData::verysharp_type()).await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 3);
    assert!(json["by_type"].is_object());
}

#[tokio::test]
#[serial]
async fn test_get_stats_no_data() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/stats").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 0);
    assert!(json["by_type"].is_object());
}

// ========== エラーレスポンスフォーマットのテスト ==========

#[tokio::test]
#[serial]
async fn test_error_response_format() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/junctions?bbox=invalid").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"].is_string());
    assert!(!json["error"].as_str().unwrap().is_empty());
}

// ========== 最小角の高低差フィルタのテスト ==========

#[tokio::test]
#[serial]
async fn test_get_junctions_with_min_angle_elevation_diff_filter() {
    let pool = setup_test_db().await;

    // min_angle_elevation_diff は GENERATED カラムなので、テストデータ挿入後にDBで計算される
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::normal_type()).await;

    let app = create_test_app(pool);

    // min_angle_elevation_diff >= 0 でフィルタリング（全件取得）
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&min_angle_elevation_diff=0",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"].as_i64().unwrap(), 2);
    assert_eq!(json["features"].as_array().unwrap().len(), 2);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_response_includes_elevation_data() {
    let pool = setup_test_db().await;

    let id = insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);

    // 標高データがレスポンスに含まれることを確認
    let properties = &json["properties"];
    assert_eq!(properties["elevation"], 100.0);
    // min_elevation_diff, max_elevation_diff もレスポンスに含まれる（表示用）
    assert_eq!(properties["min_elevation_diff"], 0.0);
    assert_eq!(properties["max_elevation_diff"], 5.0);
    // min_angle_elevation_diff は GENERATED カラムなので、DBで計算される
    assert!(properties["min_angle_elevation_diff"].is_number());
}

#[tokio::test]
#[serial]
async fn test_get_junctions_combined_filters_with_elevation() {
    let pool = setup_test_db().await;

    insert_test_junction(&pool, TestJunctionData::verysharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::normal_type()).await;

    let app = create_test_app(pool);

    // angle_type=sharp AND min_angle_elevation_diff=0 で複合フィルタリング
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&angle_type=sharp&min_angle_elevation_diff=0",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1); // sharp タイプが1件
}
