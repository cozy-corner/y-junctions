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
use std::time::Duration;
use tower::util::ServiceExt;

// テスト用のosm_node_id自動生成
static TEST_OSM_NODE_ID_COUNTER: AtomicI64 = AtomicI64::new(1);

async fn setup_test_db() -> PgPool {
    dotenvy::dotenv().ok();

    // TEST_DATABASE_URLとDATABASE_URLが両方設定されている場合、同じデータベースを使っていないかチェック
    if let (Ok(test_url), Ok(prod_url)) = (
        std::env::var("TEST_DATABASE_URL"),
        std::env::var("DATABASE_URL"),
    ) {
        // URLからデータベース名を抽出（最後の/以降）
        let test_db_name = test_url.split('/').next_back().unwrap_or("");
        let prod_db_name = prod_url.split('/').next_back().unwrap_or("");

        if !test_db_name.is_empty() && test_db_name == prod_db_name {
            panic!(
                "CRITICAL: TEST_DATABASE_URL and DATABASE_URL use the same database!\n\
                     Test database name: {}\n\
                     Production database name: {}\n\
                     Test URL: {}\n\
                     Production URL: {}\n\
                     These must use different database names to prevent data loss.",
                test_db_name, prod_db_name, test_url, prod_url
            );
        }
    }

    let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        let prod_url =
            std::env::var("DATABASE_URL").expect("DATABASE_URL or TEST_DATABASE_URL must be set");

        if !prod_url.ends_with("_test") && !prod_url.contains("test") {
            panic!(
                "CRITICAL: Tests are attempting to use production database!\n\
                     Set TEST_DATABASE_URL to a separate test database."
            );
        }

        prod_url
    });

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(2))
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    // マイグレーション実行（テストDB初回実行時にスキーマを作成）
    // CockroachDB用にアドバイザリロックを無効化（pg_advisory_lock非対応のため）
    sqlx::migrate!("./migrations")
        .set_locking(false)
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    // CockroachDBはRESTART IDENTITYをサポートしていないため削除
    sqlx::query("TRUNCATE TABLE y_junctions CASCADE")
        .execute(&pool)
        .await
        .expect("Failed to truncate table");

    // baidu_panoramas は y_junctions に FK を持たないため CASCADE で消えない
    sqlx::query("TRUNCATE TABLE baidu_panoramas")
        .execute(&pool)
        .await
        .expect("Failed to truncate baidu_panoramas");

    // google_streetview_coverage も同じ理由で CASCADE の対象外
    sqlx::query("TRUNCATE TABLE google_streetview_coverage")
        .execute(&pool)
        .await
        .expect("Failed to truncate google_streetview_coverage");

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
    way_1_bridge: bool,
    way_1_tunnel: bool,
    way_2_bridge: bool,
    way_2_tunnel: bool,
    way_3_bridge: bool,
    way_3_tunnel: bool,
    way_1_highway_type: String,
    way_2_highway_type: String,
    way_3_highway_type: String,
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
            way_1_bridge: false,
            way_1_tunnel: false,
            way_2_bridge: false,
            way_2_tunnel: false,
            way_3_bridge: false,
            way_3_tunnel: false,
            way_1_highway_type: "residential".to_string(),
            way_2_highway_type: "residential".to_string(),
            way_3_highway_type: "residential".to_string(),
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
            way_1_bridge: false,
            way_1_tunnel: false,
            way_2_bridge: false,
            way_2_tunnel: false,
            way_3_bridge: false,
            way_3_tunnel: false,
            way_1_highway_type: "tertiary".to_string(),
            way_2_highway_type: "tertiary".to_string(),
            way_3_highway_type: "tertiary".to_string(),
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
            way_1_bridge: false,
            way_1_tunnel: false,
            way_2_bridge: false,
            way_2_tunnel: false,
            way_3_bridge: false,
            way_3_tunnel: false,
            way_1_highway_type: "primary".to_string(),
            way_2_highway_type: "primary".to_string(),
            way_3_highway_type: "primary".to_string(),
        }
    }

    fn with_location(mut self, lat: f64, lon: f64) -> Self {
        self.lat = lat;
        self.lon = lon;
        self
    }

    fn with_bridge_tunnel(
        mut self,
        way_1_bridge: bool,
        way_1_tunnel: bool,
        way_2_bridge: bool,
        way_2_tunnel: bool,
        way_3_bridge: bool,
        way_3_tunnel: bool,
    ) -> Self {
        self.way_1_bridge = way_1_bridge;
        self.way_1_tunnel = way_1_tunnel;
        self.way_2_bridge = way_2_bridge;
        self.way_2_tunnel = way_2_tunnel;
        self.way_3_bridge = way_3_bridge;
        self.way_3_tunnel = way_3_tunnel;
        self
    }

    fn with_highway_types(mut self, way_1: &str, way_2: &str, way_3: &str) -> Self {
        self.way_1_highway_type = way_1.to_string();
        self.way_2_highway_type = way_2.to_string();
        self.way_3_highway_type = way_3.to_string();
        self
    }

    fn with_angles(mut self, angle_1: i16, angle_2: i16, angle_3: i16) -> Self {
        self.angle_1 = angle_1;
        self.angle_2 = angle_2;
        self.angle_3 = angle_3;
        // min_angle_index も実際の最小角に合わせて更新
        let min_idx = [angle_1, angle_2, angle_3]
            .iter()
            .enumerate()
            .min_by_key(|(_, &a)| a)
            .map(|(i, _)| (i + 1) as i16)
            .unwrap();
        self.min_angle_index = Some(min_idx);
        self
    }
}

// テストヘルパー: テストデータ挿入＋(id, osm_node_id) を返す
async fn insert_test_junction_with_ids(pool: &PgPool, data: TestJunctionData) -> (i64, i64) {
    let osm_node_id = data.osm_node_id;
    let id = insert_test_junction(pool, data).await;
    (id, osm_node_id)
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
            way_1_bridge, way_1_tunnel, way_2_bridge, way_2_tunnel, way_3_bridge, way_3_tunnel,
            way_1_highway_type, way_2_highway_type, way_3_highway_type,
            created_at
        )
        VALUES (
            $1, ST_SetSRID(ST_MakePoint($2, $3), 4326), $4, $5, $6, ARRAY[$7, $8, $9],
            $10, $11, $12, $13,
            $14, $15, $16,
            $17, $18, $19,
            $20, $21, $22, $23, $24, $25,
            $26, $27, $28,
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
    .bind(data.way_1_bridge)
    .bind(data.way_1_tunnel)
    .bind(data.way_2_bridge)
    .bind(data.way_2_tunnel)
    .bind(data.way_3_bridge)
    .bind(data.way_3_tunnel)
    .bind(&data.way_1_highway_type)
    .bind(&data.way_2_highway_type)
    .bind(&data.way_3_highway_type)
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
    let json: Value = serde_json::from_slice(&body).expect("Failed to parse JSON response");

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
    assert_eq!(json["total_count"], 2); // 返された件数（パフォーマンス改善のため全体件数は返さない）
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

// ========== GET /api/junctions/node/:osm_node_id のテスト ==========

#[tokio::test]
#[serial]
async fn test_get_junction_by_osm_node_id_success() {
    let pool = setup_test_db().await;

    let data = TestJunctionData::sharp_type();
    let osm_node_id = data.osm_node_id;
    insert_test_junction(&pool, data).await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, &format!("/api/junctions/node/{}", osm_node_id)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["type"], "Feature");
    assert_eq!(json["properties"]["osm_node_id"], osm_node_id);
    assert_eq!(json["geometry"]["type"], "Point");
}

#[tokio::test]
#[serial]
async fn test_get_junction_by_osm_node_id_not_found() {
    let pool = setup_test_db().await;
    let app = create_test_app(pool);

    let (status, json) = send_request(app, "/api/junctions/node/99999").await;

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
async fn test_get_junctions_response_includes_bearings() {
    let pool = setup_test_db().await;

    let id = insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);

    // bearings がレスポンスに含まれ、挿入した値と一致することを確認
    let bearings = &json["properties"]["bearings"];
    assert!(bearings.is_array(), "bearings should be an array");
    let arr = bearings.as_array().unwrap();
    assert_eq!(arr.len(), 3, "bearings should have 3 elements");
    // sharp_type のbearings: [10.0, 45.0, 190.0]
    assert_eq!(arr[0].as_f64().unwrap(), 10.0_f64, "bearing[0] mismatch");
    assert_eq!(arr[1].as_f64().unwrap(), 45.0_f64, "bearing[1] mismatch");
    assert_eq!(arr[2].as_f64().unwrap(), 190.0_f64, "bearing[2] mismatch");
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

#[tokio::test]
#[serial]
async fn test_get_junctions_with_max_angle_elevation_diff_filter() {
    let pool = setup_test_db().await;

    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::normal_type()).await;

    let app = create_test_app(pool);

    // max_angle_elevation_diff <= 100 でフィルタリング（全件取得）
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&max_angle_elevation_diff=100",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"].as_i64().unwrap(), 2);
    assert_eq!(json["features"].as_array().unwrap().len(), 2);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_elevation_diff_range() {
    let pool = setup_test_db().await;

    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;
    insert_test_junction(&pool, TestJunctionData::normal_type()).await;

    let app = create_test_app(pool);

    // 範囲指定: 0 <= min_angle_elevation_diff <= 100
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&min_angle_elevation_diff=0&max_angle_elevation_diff=100",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"].as_i64().unwrap(), 2);
    assert_eq!(json["features"].as_array().unwrap().len(), 2);
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_invalid_elevation_diff_range() {
    let pool = setup_test_db().await;

    let app = create_test_app(pool);

    // min > max エラー
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&min_angle_elevation_diff=10&max_angle_elevation_diff=5",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("min_angle_elevation_diff must be <= max_angle_elevation_diff"));
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_max_elevation_diff_negative() {
    let pool = setup_test_db().await;

    let app = create_test_app(pool);

    // max < 0 エラー
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&max_angle_elevation_diff=-1",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(json["error"]
        .as_str()
        .unwrap()
        .contains("max_angle_elevation_diff must be >= 0"));
}

#[tokio::test]
#[serial]
async fn test_bridge_tunnel_excluded_with_elevation_filter() {
    let pool = setup_test_db().await;

    // Insert normal junction
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    // Insert junction with bridge (should be excluded when using elevation filter)
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_bridge_tunnel(true, false, false, false, false, false),
    )
    .await;

    // Insert junction with tunnel (should be excluded when using elevation filter)
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_bridge_tunnel(false, true, false, false, false, false),
    )
    .await;

    let app = create_test_app(pool);

    // With elevation filter: bridges and tunnels should be excluded
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&min_angle_elevation_diff=0",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let features = json["features"].as_array().unwrap();
    // Only 1 junction should be returned (the normal one)
    assert_eq!(features.len(), 1);
}

#[tokio::test]
#[serial]
async fn test_bridge_tunnel_included_without_elevation_filter() {
    let pool = setup_test_db().await;

    // Insert normal junction
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    // Insert junction with bridge
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_bridge_tunnel(true, false, false, false, false, false),
    )
    .await;

    // Insert junction with tunnel
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_bridge_tunnel(false, true, false, false, false, false),
    )
    .await;

    let app = create_test_app(pool);

    // Without elevation filter: all junctions should be included
    let (status, json) = send_request(app, "/api/junctions?bbox=138.0,34.0,140.0,36.0").await;

    assert_eq!(status, StatusCode::OK);
    let features = json["features"].as_array().unwrap();
    // All 3 junctions should be returned
    assert_eq!(features.len(), 3);
}

// ========== angle_1 が最小角でないケースのテスト ==========
// 台湾カレン付近のY字路: angle_1=179, angle_2=30, angle_3=151
// 時計回りで最初の角度が 179° だが、実際の最小角は angle_2=30°（Sharp）

#[tokio::test]
#[serial]
async fn test_angle_type_filter_sharp_includes_junction_where_min_is_not_angle_1() {
    let pool = setup_test_db().await;

    // angle_1=179, angle_2=30, angle_3=151: 最小角は 30°（Sharp）
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_angles(179, 30, 151),
    )
    .await;

    let app = create_test_app(pool);

    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&angle_type=sharp",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1); // Sharp としてヒットするはず
}

#[tokio::test]
#[serial]
async fn test_angle_type_filter_normal_excludes_junction_where_min_is_not_angle_1() {
    let pool = setup_test_db().await;

    // angle_1=179, angle_2=30, angle_3=151: 最小角は 30°（Sharp）
    // Normal フィルタ（最小角 >= 45°）にはヒットしないはず
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_angles(179, 30, 151),
    )
    .await;

    let app = create_test_app(pool);

    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&angle_type=normal",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 0); // Normal としてヒットしないはず
}

// ========== Category Filter のテスト ==========

#[tokio::test]
#[serial]
async fn test_get_junctions_with_single_category_filter() {
    let pool = setup_test_db().await;

    // highway category (motorway)
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("motorway", "motorway", "motorway"),
    )
    .await;

    // major category (primary)
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("primary", "primary", "primary"),
    )
    .await;

    // local category (residential) - デフォルト
    insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    let app = create_test_app(pool);

    // category=highway でフィルタリング
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&category=highway",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1); // motorway のみ
    let features = json["features"].as_array().unwrap();
    assert_eq!(features.len(), 1);
    assert_eq!(features[0]["properties"]["way_1_category"], "highway");
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_multiple_categories_filter() {
    let pool = setup_test_db().await;

    // highway category
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("motorway", "trunk", "motorway_link"),
    )
    .await;

    // major category
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("primary", "secondary", "tertiary"),
    )
    .await;

    // local category
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("residential", "unclassified", "service"),
    )
    .await;

    // pedestrian category
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("steps", "pedestrian", "path"),
    )
    .await;

    let app = create_test_app(pool);

    // category=highway&category=major でフィルタリング（OR条件）
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&category=highway&category=major",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 2); // highway + major
}

#[tokio::test]
#[serial]
async fn test_get_junctions_with_category_and_angle_type_filter() {
    let pool = setup_test_db().await;

    // sharp + highway
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("motorway", "motorway", "trunk"),
    )
    .await;

    // verysharp + highway
    insert_test_junction(
        &pool,
        TestJunctionData::verysharp_type().with_highway_types("motorway", "trunk", "trunk_link"),
    )
    .await;

    // sharp + major
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("primary", "secondary", "tertiary"),
    )
    .await;

    let app = create_test_app(pool);

    // category=highway AND angle_type=sharp で複合フィルタリング
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&category=highway&angle_type=sharp",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1); // sharp + highway のみ
}

#[tokio::test]
#[serial]
async fn test_get_junctions_category_response_fields() {
    let pool = setup_test_db().await;

    let id = insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("motorway", "primary", "residential"),
    )
    .await;

    let app = create_test_app(pool);

    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);

    // highway_type と category がレスポンスに含まれることを確認
    let properties = &json["properties"];
    assert_eq!(properties["way_1_highway_type"], "motorway");
    assert_eq!(properties["way_2_highway_type"], "primary");
    assert_eq!(properties["way_3_highway_type"], "residential");
    assert_eq!(properties["way_1_category"], "highway");
    assert_eq!(properties["way_2_category"], "major");
    assert_eq!(properties["way_3_category"], "local");
}

#[tokio::test]
#[serial]
async fn test_get_junctions_category_or_condition() {
    let pool = setup_test_db().await;

    // 1本だけhighwayカテゴリの道路を含むY字路（残り2本はlocal）
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("motorway", "residential", "residential"),
    )
    .await;

    // 全てlocalカテゴリ
    insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_highway_types("residential", "unclassified", "service"),
    )
    .await;

    let app = create_test_app(pool);

    // category=highway で検索（OR条件: 3本のうち1本でもhighwayならヒット）
    let (status, json) = send_request(
        app,
        "/api/junctions?bbox=138.0,34.0,140.0,36.0&category=highway",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["total_count"], 1); // 1本でもhighway を含むY字路のみ
}

// ========== baidu_repository: direct repository layer ==========

use y_junction_backend::db::baidu_repository;
use y_junction_backend::domain::china::BaiduPanorama;

/// Shanghai center — confirmed inside `is_in_china_mainland`.
const SHANGHAI_LAT: f64 = 31.2304;
const SHANGHAI_LON: f64 = 121.4737;

#[tokio::test]
#[serial]
async fn test_baidu_find_by_osm_node_ids_empty_input() {
    let pool = setup_test_db().await;
    let result = baidu_repository::find_by_osm_node_ids(&pool, &[])
        .await
        .expect("query failed");
    assert!(result.is_empty());
}

#[tokio::test]
#[serial]
async fn test_baidu_find_by_osm_node_ids_skips_rows_without_panoid() {
    let pool = setup_test_db().await;
    let (_, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    let result = baidu_repository::find_by_osm_node_ids(&pool, &[osm_node_id])
        .await
        .expect("query failed");
    assert!(
        result.is_empty(),
        "junction without panoid must not appear in the map"
    );
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_update_and_find_roundtrip() {
    let pool = setup_test_db().await;
    let (_, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    let pano = BaiduPanorama {
        panoid: "TEST_PANOID_001".to_string(),
        pano_mc_x: 13_523_770.0,
        pano_mc_y: 3_640_859.0,
    };
    let updated = baidu_repository::bulk_update_baidu(&pool, &[(osm_node_id, pano.clone())])
        .await
        .expect("bulk update failed");
    assert_eq!(updated, 1);

    let result = baidu_repository::find_by_osm_node_ids(&pool, &[osm_node_id])
        .await
        .expect("query failed");
    assert_eq!(result.len(), 1);
    let got = result
        .get(&osm_node_id)
        .expect("osm_node_id missing from map");
    assert_eq!(got, &pano);
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_update_overwrites_existing() {
    let pool = setup_test_db().await;
    let (_, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    let first = BaiduPanorama {
        panoid: "OLD_PANOID".to_string(),
        pano_mc_x: 13_000_000.0,
        pano_mc_y: 3_600_000.0,
    };
    baidu_repository::bulk_update_baidu(&pool, &[(osm_node_id, first)])
        .await
        .unwrap();

    let second = BaiduPanorama {
        panoid: "NEW_PANOID".to_string(),
        pano_mc_x: 13_523_770.0,
        pano_mc_y: 3_640_859.0,
    };
    baidu_repository::bulk_update_baidu(&pool, &[(osm_node_id, second.clone())])
        .await
        .unwrap();

    let result = baidu_repository::find_by_osm_node_ids(&pool, &[osm_node_id])
        .await
        .unwrap();
    assert_eq!(result.get(&osm_node_id), Some(&second));
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_update_empty_is_noop() {
    let pool = setup_test_db().await;
    let updated = baidu_repository::bulk_update_baidu(&pool, &[])
        .await
        .expect("bulk update failed");
    assert_eq!(updated, 0);
}

#[tokio::test]
#[serial]
async fn test_baidu_find_all_for_refresh_returns_every_row() {
    let pool = setup_test_db().await;
    let (id_with, osm_with) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;
    let id_without = insert_test_junction(
        &pool,
        TestJunctionData::normal_type().with_location(SHANGHAI_LAT, SHANGHAI_LON + 0.001),
    )
    .await;
    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_with,
            BaiduPanorama {
                panoid: "EXISTING".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .unwrap();

    let all = baidu_repository::find_all_for_refresh(&pool)
        .await
        .expect("query failed");
    let ids: Vec<i64> = all.iter().map(|j| j.id).collect();
    assert!(ids.contains(&id_with));
    assert!(ids.contains(&id_without));
}

#[tokio::test]
#[serial]
async fn test_baidu_find_without_panoid_returns_only_null_rows() {
    let pool = setup_test_db().await;
    let (id_with, osm_with) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;
    let id_without = insert_test_junction(
        &pool,
        TestJunctionData::normal_type().with_location(SHANGHAI_LAT, SHANGHAI_LON + 0.001),
    )
    .await;

    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_with,
            BaiduPanorama {
                panoid: "HAS_PANOID".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .unwrap();

    let pending = baidu_repository::find_without_baidu_panoid(&pool)
        .await
        .expect("query failed");
    let pending_ids: Vec<i64> = pending.iter().map(|j| j.id).collect();
    assert!(pending_ids.contains(&id_without));
    assert!(!pending_ids.contains(&id_with));
}

#[tokio::test]
#[serial]
async fn test_baidu_find_without_panoid_skips_tombstoned_rows() {
    let pool = setup_test_db().await;
    let id_fresh = insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;
    let (id_tombstoned, osm_tombstoned) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::normal_type().with_location(SHANGHAI_LAT, SHANGHAI_LON + 0.001),
    )
    .await;

    let marked = baidu_repository::bulk_mark_queried(&pool, &[osm_tombstoned])
        .await
        .expect("bulk mark failed");
    assert_eq!(marked, 1);

    let pending = baidu_repository::find_without_baidu_panoid(&pool)
        .await
        .expect("query failed");
    let pending_ids: Vec<i64> = pending.iter().map(|j| j.id).collect();
    assert!(pending_ids.contains(&id_fresh));
    assert!(
        !pending_ids.contains(&id_tombstoned),
        "queried-but-no-coverage row must not be re-queried"
    );
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_mark_queried_empty_is_noop() {
    let pool = setup_test_db().await;
    let marked = baidu_repository::bulk_mark_queried(&pool, &[])
        .await
        .expect("bulk mark failed");
    assert_eq!(marked, 0);
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_mark_queried_skips_rows_with_panoid() {
    let pool = setup_test_db().await;
    let (_, osm_with_panoid) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;
    let (_, osm_without) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::normal_type().with_location(SHANGHAI_LAT, SHANGHAI_LON + 0.001),
    )
    .await;

    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_with_panoid,
            BaiduPanorama {
                panoid: "EXISTING".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .unwrap();

    let original_queried_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT queried_at FROM baidu_panoramas WHERE osm_node_id = $1")
            .bind(osm_with_panoid)
            .fetch_one(&pool)
            .await
            .expect("query failed");

    let marked = baidu_repository::bulk_mark_queried(&pool, &[osm_with_panoid, osm_without])
        .await
        .expect("bulk mark failed");
    assert_eq!(marked, 1, "only the panoid-less row should be tombstoned");

    let after_queried_at: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT queried_at FROM baidu_panoramas WHERE osm_node_id = $1")
            .bind(osm_with_panoid)
            .fetch_one(&pool)
            .await
            .expect("query failed");
    assert_eq!(
        original_queried_at, after_queried_at,
        "bulk_mark_queried must not touch queried_at on rows that already have a panoid"
    );
}

#[tokio::test]
#[serial]
async fn test_baidu_bulk_update_stamps_queried_at() {
    let pool = setup_test_db().await;
    let (_, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_node_id,
            BaiduPanorama {
                panoid: "STAMPED".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .unwrap();

    let queried_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT queried_at FROM baidu_panoramas WHERE osm_node_id = $1")
            .bind(osm_node_id)
            .fetch_one(&pool)
            .await
            .expect("query failed");
    assert!(
        queried_at.is_some(),
        "queried_at must be set after successful bulk_update_baidu"
    );
}

// ========== streetview_url region dispatch via handler ==========

#[tokio::test]
#[serial]
async fn test_china_junction_with_baidu_returns_baidu_url() {
    let pool = setup_test_db().await;
    let (id, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;
    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_node_id,
            BaiduPanorama {
                panoid: "SHANGHAI_TEST".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .unwrap();

    let app = create_test_app(pool);
    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);
    let url = json["properties"]["streetview_url"].as_str().unwrap();
    assert!(
        url.contains("map.baidu.com"),
        "expected baidu URL, got: {url}"
    );
    assert!(url.contains("SHANGHAI_TEST"));
}

#[tokio::test]
#[serial]
async fn test_china_junction_without_baidu_returns_empty_streetview_url() {
    let pool = setup_test_db().await;
    let id = insert_test_junction(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    let app = create_test_app(pool);
    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["properties"]["streetview_url"], "");
}

#[tokio::test]
#[serial]
async fn test_non_china_junction_still_uses_google_url() {
    // Existing Tokyo-latitude fixtures must not regress to an empty URL.
    let pool = setup_test_db().await;
    let id = insert_test_junction(&pool, TestJunctionData::sharp_type()).await;

    let app = create_test_app(pool);
    let (status, json) = send_request(app, &format!("/api/junctions/{}", id)).await;

    assert_eq!(status, StatusCode::OK);
    let url = json["properties"]["streetview_url"].as_str().unwrap();
    assert!(
        url.contains("google.com/maps"),
        "expected google URL, got: {url}"
    );
}

// ========== google_repository: coverage cache ==========

use y_junction_backend::db::google_repository;

#[tokio::test]
#[serial]
async fn test_google_find_coverage_empty_input() {
    let pool = setup_test_db().await;
    let result = google_repository::find_coverage_by_osm_node_ids(&pool, &[])
        .await
        .expect("query failed");
    assert!(result.is_empty());
}

#[tokio::test]
#[serial]
async fn test_google_find_coverage_distinguishes_three_states() {
    let pool = setup_test_db().await;
    let (_, osm_covered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::sharp_type()).await;
    let (_, osm_uncovered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::normal_type()).await;
    let (_, osm_unqueried) =
        insert_test_junction_with_ids(&pool, TestJunctionData::verysharp_type()).await;

    google_repository::upsert_coverage(&pool, &[(osm_covered, true), (osm_uncovered, false)])
        .await
        .expect("upsert failed");

    let result = google_repository::find_coverage_by_osm_node_ids(
        &pool,
        &[osm_covered, osm_uncovered, osm_unqueried],
    )
    .await
    .expect("query failed");

    assert_eq!(result.get(&osm_covered), Some(&true));
    assert_eq!(result.get(&osm_uncovered), Some(&false));
    assert_eq!(
        result.get(&osm_unqueried),
        None,
        "never-queried node must be absent from the map, not false"
    );
}

#[tokio::test]
#[serial]
async fn test_google_upsert_empty_is_noop() {
    let pool = setup_test_db().await;
    let written = google_repository::upsert_coverage(&pool, &[])
        .await
        .expect("upsert failed");
    assert_eq!(written, 0);
}

#[tokio::test]
#[serial]
async fn test_google_upsert_overwrites_false_with_true() {
    let pool = setup_test_db().await;
    let (_, osm_node_id) =
        insert_test_junction_with_ids(&pool, TestJunctionData::sharp_type()).await;

    google_repository::upsert_coverage(&pool, &[(osm_node_id, false)])
        .await
        .expect("first upsert failed");
    google_repository::upsert_coverage(&pool, &[(osm_node_id, true)])
        .await
        .expect("second upsert failed");

    let result = google_repository::find_coverage_by_osm_node_ids(&pool, &[osm_node_id])
        .await
        .expect("query failed");
    assert_eq!(
        result.get(&osm_node_id),
        Some(&true),
        "--refresh must be able to flip false to true"
    );
}

#[tokio::test]
#[serial]
async fn test_google_find_uncovered_nodes_skips_queried_nodes() {
    let pool = setup_test_db().await;
    let (_, osm_covered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::sharp_type()).await;
    let (_, osm_uncovered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::normal_type()).await;
    let (_, osm_unqueried) =
        insert_test_junction_with_ids(&pool, TestJunctionData::verysharp_type()).await;

    google_repository::upsert_coverage(&pool, &[(osm_covered, true), (osm_uncovered, false)])
        .await
        .expect("upsert failed");

    let pending = google_repository::find_uncovered_nodes(&pool, false)
        .await
        .expect("query failed");
    let ids: Vec<i64> = pending.iter().map(|c| c.osm_node_id).collect();

    assert!(ids.contains(&osm_unqueried));
    assert!(!ids.contains(&osm_covered));
    assert!(
        !ids.contains(&osm_uncovered),
        "without --refresh, confirmed-uncovered nodes must not be re-queried"
    );
}

#[tokio::test]
#[serial]
async fn test_google_find_uncovered_nodes_refresh_includes_false_rows() {
    let pool = setup_test_db().await;
    let (_, osm_covered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::sharp_type()).await;
    let (_, osm_uncovered) =
        insert_test_junction_with_ids(&pool, TestJunctionData::normal_type()).await;
    let (_, osm_unqueried) =
        insert_test_junction_with_ids(&pool, TestJunctionData::verysharp_type()).await;

    google_repository::upsert_coverage(&pool, &[(osm_covered, true), (osm_uncovered, false)])
        .await
        .expect("upsert failed");

    let pending = google_repository::find_uncovered_nodes(&pool, true)
        .await
        .expect("query failed");
    let ids: Vec<i64> = pending.iter().map(|c| c.osm_node_id).collect();

    assert!(
        ids.contains(&osm_uncovered),
        "--refresh must re-query false rows"
    );
    assert!(ids.contains(&osm_unqueried));
    assert!(
        !ids.contains(&osm_covered),
        "--refresh must not re-query nodes already known to be covered"
    );
}

#[tokio::test]
#[serial]
async fn test_google_find_uncovered_nodes_returns_coordinates() {
    // The China filter runs on these coordinates in-process, so they must
    // survive the round trip.
    let pool = setup_test_db().await;
    let (_, osm_node_id) = insert_test_junction_with_ids(
        &pool,
        TestJunctionData::sharp_type().with_location(SHANGHAI_LAT, SHANGHAI_LON),
    )
    .await;

    let pending = google_repository::find_uncovered_nodes(&pool, false)
        .await
        .expect("query failed");
    let found = pending
        .iter()
        .find(|c| c.osm_node_id == osm_node_id)
        .expect("inserted junction missing from candidates");

    assert!((found.lat - SHANGHAI_LAT).abs() < 1e-9);
    assert!((found.lon - SHANGHAI_LON).abs() < 1e-9);
}

// ========== streetview_enricher: Google coverage exclusion ==========

use y_junction_backend::api::streetview_enricher;
use y_junction_backend::domain::Junction;

const TOKYO_LAT: f64 = 35.6812;
const TOKYO_LON: f64 = 139.7671;

fn mk_enricher_junction(osm_node_id: i64, lat: f64, lon: f64) -> Junction {
    Junction {
        id: osm_node_id,
        osm_node_id,
        lat,
        lon,
        angle_1: 35,
        angle_2: 145,
        angle_3: 180,
        bearings: vec![10.0, 45.0, 190.0],
        created_at: chrono::Utc::now(),
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

fn feature_ids(collection: &Value) -> Vec<i64> {
    collection["features"]
        .as_array()
        .expect("features array")
        .iter()
        .map(|f| {
            f["properties"]["osm_node_id"]
                .as_i64()
                .expect("osm_node_id")
        })
        .collect()
}

#[tokio::test]
#[serial]
async fn test_enrich_collection_drops_only_uncovered_non_china() {
    let pool = setup_test_db().await;

    let uncovered = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let covered = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let unqueried = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);

    google_repository::upsert_coverage(&pool, &[(uncovered, false), (covered, true)])
        .await
        .expect("upsert failed");

    let junctions = vec![
        mk_enricher_junction(uncovered, TOKYO_LAT, TOKYO_LON),
        mk_enricher_junction(covered, TOKYO_LAT, TOKYO_LON),
        mk_enricher_junction(unqueried, TOKYO_LAT, TOKYO_LON),
    ];

    let result = streetview_enricher::enrich_collection(&pool, junctions)
        .await
        .expect("enrich failed");

    let ids = feature_ids(&result);
    assert!(
        !ids.contains(&uncovered),
        "uncovered junction should be dropped, got {ids:?}"
    );
    // A junction we never asked Google about is not the same as one Google
    // said no to; keeping it is the whole point of the 3-state cache.
    assert!(
        ids.contains(&covered) && ids.contains(&unqueried),
        "{ids:?}"
    );
    assert_eq!(result["total_count"].as_i64(), Some(2));
}

#[tokio::test]
#[serial]
async fn test_enrich_collection_keeps_china_junction_regardless_of_coverage() {
    let pool = setup_test_db().await;

    let osm_node_id = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    // A stray coverage row for a mainland junction must not exclude it — the
    // Baidu panorama owns that decision.
    google_repository::upsert_coverage(&pool, &[(osm_node_id, false)])
        .await
        .expect("upsert failed");
    baidu_repository::bulk_update_baidu(
        &pool,
        &[(
            osm_node_id,
            y_junction_backend::domain::china::BaiduPanorama {
                panoid: "PANO_TEST".to_string(),
                pano_mc_x: 13_523_770.0,
                pano_mc_y: 3_640_859.0,
            },
        )],
    )
    .await
    .expect("baidu upsert failed");

    let junctions = vec![mk_enricher_junction(
        osm_node_id,
        SHANGHAI_LAT,
        SHANGHAI_LON,
    )];

    let result = streetview_enricher::enrich_collection(&pool, junctions)
        .await
        .expect("enrich failed");

    assert_eq!(feature_ids(&result), vec![osm_node_id]);
}

#[tokio::test]
#[serial]
async fn test_enrich_feature_returns_uncovered_junction_with_empty_url() {
    let pool = setup_test_db().await;

    let osm_node_id = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    google_repository::upsert_coverage(&pool, &[(osm_node_id, false)])
        .await
        .expect("upsert failed");

    let feature = streetview_enricher::enrich_feature(
        &pool,
        mk_enricher_junction(osm_node_id, TOKYO_LAT, TOKYO_LON),
    )
    .await
    .expect("enrich failed");

    // Direct links must not 404 a junction that exists; the empty URL is what
    // suppresses the popup button.
    assert_eq!(
        feature["properties"]["osm_node_id"].as_i64(),
        Some(osm_node_id)
    );
    assert_eq!(feature["properties"]["streetview_url"].as_str(), Some(""));
}

#[tokio::test]
#[serial]
async fn test_enrich_feature_covered_and_unqueried_keep_google_url() {
    let pool = setup_test_db().await;

    let covered = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let unqueried = TEST_OSM_NODE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    google_repository::upsert_coverage(&pool, &[(covered, true)])
        .await
        .expect("upsert failed");

    for osm_node_id in [covered, unqueried] {
        let feature = streetview_enricher::enrich_feature(
            &pool,
            mk_enricher_junction(osm_node_id, TOKYO_LAT, TOKYO_LON),
        )
        .await
        .expect("enrich failed");

        let url = feature["properties"]["streetview_url"]
            .as_str()
            .expect("streetview_url");
        assert!(url.contains("google.com/maps"), "{osm_node_id}: {url}");
    }
}
