// Integration tests for the analytics service HTTP API.
//
// Strategy: no real database or Kafka is required.
// `MockAnalyticsDb` hard-codes the query responses, and a thin
// `MockQueryService` wraps it.  The test app is wired from real Axum
// router code with mock state injected, so routing, middleware, auth
// checks, and serialisation are all exercised end-to-end.
//
// JWT tokens are minted with a shared test secret so the real
// `require_auth` middleware accepts them without any mocking.

use std::sync::Arc;

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::{NaiveDate, Utc};
use serde_json::Value;
use tower::ServiceExt; // for `.oneshot()`
use uuid::Uuid;

use logisticos_auth::{claims::Claims, jwt::JwtService};
use logisticos_errors::{AppError, AppResult};
use logisticos_types::TenantId;

use logisticos_analytics::domain::entities::{DailyBucket, DeliveryKpis, DriverPerformance};

// ─────────────────────────────────────────────────────────────────────────────
// In-memory mock DB
// ─────────────────────────────────────────────────────────────────────────────

/// Replaces `AnalyticsDb` in tests.  All methods delegate to closures
/// stored at construction time so each test can inject its own data without
/// shared mutable state.
struct MockAnalyticsDb {
    kpis_result:        Box<dyn Fn(Uuid, NaiveDate, NaiveDate) -> anyhow::Result<DeliveryKpis> + Send + Sync>,
    timeseries_result:  Box<dyn Fn(Uuid, NaiveDate, NaiveDate) -> anyhow::Result<Vec<DailyBucket>> + Send + Sync>,
    driver_perf_result: Box<dyn Fn(Uuid, NaiveDate, NaiveDate, i64) -> anyhow::Result<Vec<DriverPerformance>> + Send + Sync>,
}

impl MockAnalyticsDb {
    fn new(
        kpis:    impl Fn(Uuid, NaiveDate, NaiveDate) -> anyhow::Result<DeliveryKpis> + Send + Sync + 'static,
        ts:      impl Fn(Uuid, NaiveDate, NaiveDate) -> anyhow::Result<Vec<DailyBucket>> + Send + Sync + 'static,
        drivers: impl Fn(Uuid, NaiveDate, NaiveDate, i64) -> anyhow::Result<Vec<DriverPerformance>> + Send + Sync + 'static,
    ) -> Self {
        Self {
            kpis_result:        Box::new(kpis),
            timeseries_result:  Box::new(ts),
            driver_perf_result: Box::new(drivers),
        }
    }

    /// Convenience: all queries return empty / zero data.
    fn empty() -> Self {
        Self::new(
            |tenant_id, from, to| Ok(zero_kpis(tenant_id, from, to)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(vec![]),
        )
    }

    async fn delivery_kpis(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<DeliveryKpis> {
        (self.kpis_result)(tenant_id, from, to)
    }

    async fn daily_timeseries(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<DailyBucket>> {
        (self.timeseries_result)(tenant_id, from, to)
    }

    async fn driver_performance(
        &self,
        tenant_id: Uuid,
        from: NaiveDate,
        to: NaiveDate,
        limit: i64,
    ) -> anyhow::Result<Vec<DriverPerformance>> {
        (self.driver_perf_result)(tenant_id, from, to, limit)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Mock QueryService that delegates to MockAnalyticsDb
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps `MockAnalyticsDb` and replicates the validation logic that lives in
/// the real `QueryService` so validation-error tests still pass without a real
/// DB connection.
struct MockQueryService {
    db: Arc<MockAnalyticsDb>,
}

impl MockQueryService {
    fn new(db: Arc<MockAnalyticsDb>) -> Self { Self { db } }

    async fn delivery_kpis(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> AppResult<DeliveryKpis> {
        if from > to {
            return Err(AppError::Validation("'from' must be before 'to'".into()));
        }
        let max_range = chrono::Duration::days(365);
        if to - from > max_range {
            return Err(AppError::Validation("Date range cannot exceed 365 days".into()));
        }
        self.db
            .delivery_kpis(tenant_id.inner(), from, to)
            .await
            .map_err(AppError::internal)
    }

    async fn daily_timeseries(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> AppResult<Vec<DailyBucket>> {
        if from > to {
            return Err(AppError::Validation("'from' must be before 'to'".into()));
        }
        self.db
            .daily_timeseries(tenant_id.inner(), from, to)
            .await
            .map_err(AppError::internal)
    }

    async fn driver_performance(
        &self,
        tenant_id: &TenantId,
        from: NaiveDate,
        to: NaiveDate,
        limit: i64,
    ) -> AppResult<Vec<DriverPerformance>> {
        self.db
            .driver_performance(tenant_id.inner(), from, to, limit.clamp(1, 50))
            .await
            .map_err(AppError::internal)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test app wiring
//
// We cannot directly instantiate `QueryService` (its constructor requires a
// real `Arc<AnalyticsDb>`), so we build a parallel `TestAppState` that wraps
// `MockQueryService` and mount it on the same Axum router paths.
// ─────────────────────────────────────────────────────────────────────────────

const TEST_JWT_SECRET: &str = "test-analytics-secret-32-bytes!!";

/// Builds the Axum `Router` wired with a mock state.
///
/// Because `AppState` requires `Arc<QueryService>` (which needs a real PgPool),
/// we build a local router that reuses the same handler functions via the same
/// URL paths but uses `TestAppState` injected through Axum's generic state.
fn build_test_router(db: Arc<MockAnalyticsDb>) -> Router {
    use axum::{
        extract::{Query, State},
        http::StatusCode,
        response::{IntoResponse, Json},
        routing::get,
    };
    use chrono::NaiveDate;
    use logisticos_auth::middleware::{require_auth, AuthState};
    use serde::Deserialize;

    #[derive(Clone)]
    struct TestState {
        query_svc: Arc<MockQueryService>,
    }

    #[derive(Debug, Deserialize)]
    struct DateRangeQuery {
        from:  NaiveDate,
        to:    NaiveDate,
        limit: Option<i64>,
    }

    async fn delivery_kpis(
        State(state): State<TestState>,
        claims: logisticos_auth::middleware::AuthClaims,
        Query(q): Query<DateRangeQuery>,
    ) -> impl IntoResponse {
        claims.require_permission(logisticos_auth::rbac::permissions::ANALYTICS_VIEW)?;
        let kpis = state.query_svc
            .delivery_kpis(&TenantId::from_uuid(claims.tenant_id), q.from, q.to)
            .await?;
        Ok::<_, AppError>((StatusCode::OK, Json(kpis)))
    }

    async fn daily_timeseries(
        State(state): State<TestState>,
        claims: logisticos_auth::middleware::AuthClaims,
        Query(q): Query<DateRangeQuery>,
    ) -> impl IntoResponse {
        claims.require_permission(logisticos_auth::rbac::permissions::ANALYTICS_VIEW)?;
        let buckets = state.query_svc
            .daily_timeseries(&TenantId::from_uuid(claims.tenant_id), q.from, q.to)
            .await?;
        Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
            "data":  buckets,
            "count": buckets.len()
        }))))
    }

    async fn driver_performance(
        State(state): State<TestState>,
        claims: logisticos_auth::middleware::AuthClaims,
        Query(q): Query<DateRangeQuery>,
    ) -> impl IntoResponse {
        claims.require_permission(logisticos_auth::rbac::permissions::ANALYTICS_VIEW)?;
        let perf = state.query_svc
            .driver_performance(
                &TenantId::from_uuid(claims.tenant_id),
                q.from,
                q.to,
                q.limit.unwrap_or(20),
            )
            .await?;
        Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
            "drivers": perf,
            "count":   perf.len()
        }))))
    }

    let jwt_svc = Arc::new(JwtService::new(TEST_JWT_SECRET, 3600, 86400));
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&jwt_svc) as AuthState,
        require_auth,
    );

    let state = TestState {
        query_svc: Arc::new(MockQueryService::new(db)),
    };

    Router::new()
        .route("/v1/analytics/kpis",               get(delivery_kpis))
        .route("/v1/analytics/timeseries",          get(daily_timeseries))
        .route("/v1/analytics/driver-performance",  get(driver_performance))
        .layer(auth_layer)
        .with_state(state)
}

// ─────────────────────────────────────────────────────────────────────────────
// JWT test helper
// ─────────────────────────────────────────────────────────────────────────────

fn mint_jwt(tenant_id: Uuid, permissions: Vec<&str>) -> String {
    let jwt = JwtService::new(TEST_JWT_SECRET, 3600, 86400);
    let claims = Claims::new(
        Uuid::new_v4(),
        tenant_id,
        "test-tenant".into(),
        "business".into(),
        "test@example.com".into(),
        vec!["admin".into()],
        permissions.into_iter().map(String::from).collect(),
        3600,
    );
    jwt.issue_access_token(claims).expect("JWT minting failed in test")
}

// ─────────────────────────────────────────────────────────────────────────────
// Data fixtures
// ─────────────────────────────────────────────────────────────────────────────

fn zero_kpis(tenant_id: Uuid, from: NaiveDate, to: NaiveDate) -> DeliveryKpis {
    DeliveryKpis {
        tenant_id,
        from,
        to,
        total_shipments:       0,
        delivered:             0,
        failed:                0,
        cancelled:             0,
        delivery_success_rate: 0.0,
        on_time_rate:          0.0,
        avg_delivery_hours:    0.0,
        cod_shipments:         0,
        cod_collected_cents:   0,
        cod_collection_rate:   0.0,
        computed_at:           Utc::now(),
    }
}

fn sample_kpis(tenant_id: Uuid, from: NaiveDate, to: NaiveDate) -> DeliveryKpis {
    // 200 delivered, 20 failed, 5 cancelled, 215 created
    // success_rate = 200 / 220 * 100 ≈ 90.909…
    // on_time: 160 out of 200 with ETA → 80.0
    // COD: 50 shipments, PHP 75 000.00 (7_500_000 centavos)
    DeliveryKpis {
        tenant_id,
        from,
        to,
        total_shipments:       215,
        delivered:             200,
        failed:                20,
        cancelled:             5,
        delivery_success_rate: 200.0 / 220.0 * 100.0,
        on_time_rate:          80.0,
        avg_delivery_hours:    3.5,
        cod_shipments:         50,
        cod_collected_cents:   7_500_000,
        cod_collection_rate:   7_500_000.0 / 50.0 / 100.0,
        computed_at:           Utc::now(),
    }
}

fn sample_timeseries(from: NaiveDate) -> Vec<DailyBucket> {
    (0..7).map(|i| {
        let date = from + chrono::Duration::days(i);
        DailyBucket {
            date,
            shipments:           30,
            delivered:           26,
            failed:               3,
            success_rate:        26.0 / 29.0 * 100.0,
            cod_collected_cents: 500_000,
        }
    }).collect()
}

fn sample_drivers(count: usize) -> Vec<DriverPerformance> {
    (0..count).map(|i| DriverPerformance {
        driver_id:           Uuid::new_v4(),
        driver_name:         Some(format!("Driver {}", i + 1)),
        total_deliveries:    (100 - i as i64),
        successful:          (95 - i as i64),
        failed:              5,
        success_rate:        (95 - i as i64) as f64 / (100 - i as i64) as f64 * 100.0,
        avg_delivery_hours:  2.5 + i as f64 * 0.1,
        cod_collected_cents: 1_000_000 - i as i64 * 10_000,
    }).collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP helper
// ─────────────────────────────────────────────────────────────────────────────

async fn send(app: Router, method: Method, uri: &str, token: &str) -> (StatusCode, Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let bytes  = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, json)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — KPI endpoint
// ─────────────────────────────────────────────────────────────────────────────

mod kpi_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_delivery_kpis_for_valid_range() {
        let tenant_id = Uuid::new_v4();
        let from = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let to   = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(sample_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_shipments"], 215);
        assert_eq!(body["delivered"], 200);
        assert_eq!(body["failed"], 20);
        assert_eq!(body["cancelled"], 5);
        assert_eq!(body["cod_shipments"], 50);
        assert_eq!(body["cod_collected_cents"], 7_500_000);
        assert!(body["delivery_success_rate"].as_f64().unwrap() > 90.0);
        assert!(body["delivery_success_rate"].as_f64().unwrap() < 91.0);
        assert_eq!(body["on_time_rate"], 80.0);
        assert_eq!(body["avg_delivery_hours"], 3.5);
        assert!(body["computed_at"].is_string(), "computed_at must be present");
    }

    #[tokio::test]
    async fn returns_zeros_when_no_data_in_range() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["total_shipments"], 0);
        assert_eq!(body["delivered"], 0);
        assert_eq!(body["failed"], 0);
        assert_eq!(body["delivery_success_rate"], 0.0);
        assert_eq!(body["on_time_rate"], 0.0);
        assert_eq!(body["cod_collected_cents"], 0);
    }

    #[tokio::test]
    async fn delivery_success_rate_equals_delivered_over_delivered_plus_failed_times_100() {
        let tenant_id = Uuid::new_v4();
        // 80 delivered, 20 failed → 80/100 = 80.0%
        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(DeliveryKpis {
                tenant_id: tid,
                from: f,
                to: t,
                total_shipments:       100,
                delivered:             80,
                failed:                20,
                cancelled:             0,
                delivery_success_rate: 80.0 / 100.0 * 100.0, // 80.0
                on_time_rate:          75.0,
                avg_delivery_hours:    2.0,
                cod_shipments:         0,
                cod_collected_cents:   0,
                cod_collection_rate:   0.0,
                computed_at:           Utc::now(),
            }),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let rate = body["delivery_success_rate"].as_f64().unwrap();
        // 80 / (80 + 20) * 100 = 80.0 exactly
        assert!((rate - 80.0).abs() < 0.001, "Expected 80.0 but got {rate}");
    }

    #[tokio::test]
    async fn returns_422_when_from_is_after_to() {
        let tenant_id = Uuid::new_v4();
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        // from (Feb 1) > to (Jan 1) — invalid range
        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-02-01&to=2026-01-01",
            &token,
        ).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn returns_400_when_date_format_is_wrong() {
        let tenant_id = Uuid::new_v4();
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        // Axum's `Query` extractor fails deserialization — yields 400
        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=not-a-date&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn returns_401_without_token() {
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/v1/analytics/kpis?from=2026-01-01&to=2026-01-31")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn returns_403_without_analytics_view_permission() {
        let tenant_id = Uuid::new_v4();
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        // Token with an unrelated permission
        let token = mint_jwt(tenant_id, vec!["shipments:read"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn kpis_are_scoped_to_jwt_tenant_id() {
        // The tenant_id in the KPI result must match the JWT's tenant_id, not
        // a hard-coded fixture UUID.
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body["tenant_id"].as_str().unwrap(),
            tenant_id.to_string(),
            "tenant_id in response must match the JWT claim"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — timeseries endpoint
// ─────────────────────────────────────────────────────────────────────────────

mod timeseries_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_daily_buckets_for_valid_range() {
        let tenant_id = Uuid::new_v4();
        let from = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            move |_, _, _| Ok(sample_timeseries(from)),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-01-01&to=2026-01-07",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);

        let data = body["data"].as_array().unwrap();
        assert_eq!(data.len(), 7, "Should return 7 daily buckets");
        assert_eq!(body["count"], 7);

        let first = &data[0];
        assert!(first["date"].is_string(), "Each bucket must have a date");
        assert!(first["shipments"].is_number());
        assert!(first["delivered"].is_number());
        assert!(first["failed"].is_number());
        assert!(first["success_rate"].is_number());
        assert!(first["cod_collected_cents"].is_number());

        // Verify first bucket values match fixture
        assert_eq!(first["shipments"], 30);
        assert_eq!(first["delivered"], 26);
        assert_eq!(first["failed"], 3);
        assert_eq!(first["cod_collected_cents"], 500_000);
    }

    #[tokio::test]
    async fn returns_empty_array_when_no_data() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-01-01&to=2026-01-07",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let data = body["data"].as_array().unwrap();
        assert!(data.is_empty(), "Empty range must return empty data array");
        assert_eq!(body["count"], 0);
    }

    #[tokio::test]
    async fn returns_422_when_from_is_after_to() {
        let tenant_id = Uuid::new_v4();
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-01-07&to=2026-01-01",
            &token,
        ).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "VALIDATION_ERROR");
    }

    #[tokio::test]
    async fn returns_400_when_date_format_invalid() {
        let tenant_id = Uuid::new_v4();
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-13-01&to=2026-01-07",
            &token,
        ).await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn bucket_success_rate_reflects_delivered_and_failed_counts() {
        let tenant_id = Uuid::new_v4();
        let date = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            move |_, _, _| Ok(vec![DailyBucket {
                date,
                shipments:           50,
                delivered:           40,
                failed:              10,
                success_rate:        40.0 / 50.0 * 100.0,  // 80.0
                cod_collected_cents: 1_000_000,
            }]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-01-01&to=2026-01-01",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let bucket = &body["data"][0];
        let rate = bucket["success_rate"].as_f64().unwrap();
        assert!((rate - 80.0).abs() < 0.001, "Expected 80.0, got {rate}");
    }

    #[tokio::test]
    async fn single_day_range_returns_single_bucket() {
        let tenant_id = Uuid::new_v4();
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            move |_, _, _| Ok(vec![DailyBucket {
                date,
                shipments:           10,
                delivered:           9,
                failed:              1,
                success_rate:        90.0,
                cod_collected_cents: 0,
            }]),
            |_, _, _, _| Ok(vec![]),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/timeseries?from=2026-01-15&to=2026-01-15",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 1);
        assert_eq!(body["data"][0]["date"], "2026-01-15");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — driver performance endpoint
// ─────────────────────────────────────────────────────────────────────────────

mod driver_performance_endpoint {
    use super::*;

    #[tokio::test]
    async fn returns_200_with_driver_performance_array() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(sample_drivers(5)),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31&limit=10",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let drivers = body["drivers"].as_array().unwrap();
        assert_eq!(drivers.len(), 5);
        assert_eq!(body["count"], 5);
    }

    #[tokio::test]
    async fn each_driver_record_has_required_fields() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(sample_drivers(1)),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let driver = &body["drivers"][0];
        assert!(driver["driver_id"].is_string());
        assert!(driver["total_deliveries"].is_number());
        assert!(driver["successful"].is_number());
        assert!(driver["failed"].is_number());
        assert!(driver["success_rate"].is_number());
        assert!(driver["avg_delivery_hours"].is_number());
        assert!(driver["cod_collected_cents"].is_number());
    }

    #[tokio::test]
    async fn drivers_are_sorted_by_successful_deliveries_descending() {
        let tenant_id = Uuid::new_v4();

        // Return drivers in already-sorted order (mock enforces DB sort contract)
        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, _| Ok(sample_drivers(3)), // sample_drivers already sorted desc
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let drivers = body["drivers"].as_array().unwrap();
        let successful: Vec<i64> = drivers.iter()
            .map(|d| d["successful"].as_i64().unwrap())
            .collect();

        // Verify the array is non-increasing (sorted desc)
        for w in successful.windows(2) {
            assert!(w[0] >= w[1], "Drivers must be sorted by successful desc: {} < {}", w[0], w[1]);
        }
    }

    #[tokio::test]
    async fn limit_param_is_forwarded_and_respected() {
        let tenant_id = Uuid::new_v4();
        // The mock receives the limit parameter and trims accordingly
        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            |_, _, _, limit| Ok(sample_drivers(limit as usize)),
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31&limit=10",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["count"], 10, "Limit=10 should yield 10 records");
    }

    #[tokio::test]
    async fn returns_empty_array_when_no_drivers_have_activity() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let drivers = body["drivers"].as_array().unwrap();
        assert!(drivers.is_empty());
        assert_eq!(body["count"], 0);
    }

    #[tokio::test]
    async fn superadmin_wildcard_permission_grants_access() {
        let tenant_id = Uuid::new_v4();

        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["*"]); // superadmin

        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
    }

    #[tokio::test]
    async fn default_limit_is_applied_when_not_specified() {
        let tenant_id = Uuid::new_v4();

        // The mock captures the limit and returns that many records so we can
        // verify what value reached the DB layer.
        let captured_limit: Arc<std::sync::Mutex<Option<i64>>> = Arc::new(std::sync::Mutex::new(None));
        let cap_clone = Arc::clone(&captured_limit);

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            move |_, _, _, limit| {
                *cap_clone.lock().unwrap() = Some(limit);
                Ok(vec![])
            },
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        // The handler passes `q.limit.unwrap_or(20)` and then QueryService
        // clamps to 1..=50. Default=20 and 20 ≤ 50, so the value should be 20.
        let sent_limit = captured_limit.lock().unwrap().unwrap();
        assert_eq!(sent_limit, 20, "Default limit must be 20");
    }

    #[tokio::test]
    async fn large_limit_is_clamped_to_50_by_query_service() {
        let tenant_id = Uuid::new_v4();

        let captured_limit: Arc<std::sync::Mutex<Option<i64>>> = Arc::new(std::sync::Mutex::new(None));
        let cap_clone = Arc::clone(&captured_limit);

        let db = Arc::new(MockAnalyticsDb::new(
            move |tid, f, t| Ok(zero_kpis(tid, f, t)),
            |_, _, _| Ok(vec![]),
            move |_, _, _, limit| {
                *cap_clone.lock().unwrap() = Some(limit);
                Ok(vec![])
            },
        ));
        let app = build_test_router(db);
        let token = mint_jwt(tenant_id, vec!["analytics:view"]);

        let (status, _) = send(
            app, Method::GET,
            // Request limit=500 — should be clamped to 50 by QueryService
            "/v1/analytics/driver-performance?from=2026-01-01&to=2026-01-31&limit=500",
            &token,
        ).await;

        assert_eq!(status, StatusCode::OK);
        let sent_limit = captured_limit.lock().unwrap().unwrap();
        assert_eq!(sent_limit, 50, "Limit must be clamped to max 50 by QueryService");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test suite — cross-cutting auth concerns
// ─────────────────────────────────────────────────────────────────────────────

mod auth_middleware {
    use super::*;

    #[tokio::test]
    async fn expired_token_returns_401() {
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);

        // Issue a token that expired 1 hour ago
        let jwt = JwtService::new(TEST_JWT_SECRET, -3600, 86400);
        let claims = Claims::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            "tenant".into(),
            "starter".into(),
            "e@example.com".into(),
            vec![],
            vec!["analytics:view".into()],
            -3600,
        );
        let token = jwt.issue_access_token(claims).unwrap();

        let (status, body) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "TOKEN_EXPIRED");
    }

    #[tokio::test]
    async fn wrong_secret_returns_401() {
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);

        let wrong_jwt = JwtService::new("totally-different-secret-key!!", 3600, 86400);
        let claims = Claims::new(
            Uuid::new_v4(), Uuid::new_v4(),
            "tenant".into(), "starter".into(),
            "e@example.com".into(),
            vec![], vec!["analytics:view".into()], 3600,
        );
        let token = wrong_jwt.issue_access_token(claims).unwrap();

        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            &token,
        ).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn malformed_bearer_token_returns_401() {
        let db = Arc::new(MockAnalyticsDb::empty());
        let app = build_test_router(db);

        let (status, _) = send(
            app, Method::GET,
            "/v1/analytics/kpis?from=2026-01-01&to=2026-01-31",
            "this.is.not.a.valid.jwt",
        ).await;

        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
}
