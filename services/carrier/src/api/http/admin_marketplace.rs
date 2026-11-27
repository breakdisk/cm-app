//! Tenant-wide marketplace routes for operators.
//!
//! `/v1/marketplace/listings` and `/v1/marketplace/bookings` resolve the
//! caller's *own* carrier from the JWT email, so they cannot serve an admin who
//! is not a carrier contact. These routes read and write across every carrier in
//! the caller's tenant instead. The tenant always comes from the validated
//! token, never from the request.
//!
//! Gated on `CARRIERS_MANAGE`, the permission the admin side already uses for
//! onboarding, activating and suspending carriers. Partners hold only
//! `CARRIERS_MANAGE_OWN`, so they cannot reach another carrier's rows here.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::application::services::{CreateListingCommand, MarketplaceService};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct AdminPaginationQuery {
    limit:  Option<i64>,
    offset: Option<i64>,
}

/// Body of `POST /v1/marketplace/admin/listings`: the carrier create body plus
/// the carrier the listing is for.
#[derive(Debug, Deserialize)]
pub struct AdminCreateListingBody {
    pub carrier_id: Uuid,
    #[serde(flatten)]
    pub listing:    CreateListingCommand,
}

/// GET /v1/marketplace/admin/listings
pub async fn admin_list_listings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AdminPaginationQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let listings = state.marketplace_svc
        .list_listings_for_tenant(claims.tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?;
    let count = listings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"listings": listings, "count": count}))))
}

/// GET /v1/marketplace/admin/bookings
pub async fn admin_list_bookings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AdminPaginationQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let bookings: Vec<_> = state.marketplace_svc
        .list_bookings_for_tenant(claims.tenant_id, q.limit.unwrap_or(50), q.offset.unwrap_or(0))
        .await?
        .into_iter()
        .map(MarketplaceService::apply_pii_mask)
        .collect();
    let count = bookings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({"bookings": bookings, "count": count}))))
}

/// POST /v1/marketplace/admin/listings
///
/// A carrier in another tenant answers exactly like one that does not exist, so
/// carrier ids cannot be probed across tenants.
pub async fn admin_create_listing(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<AdminCreateListingBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let carrier = state.carrier_svc.get(body.carrier_id).await?;
    if carrier.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "Carrier", id: body.carrier_id.to_string() });
    }
    let listing = state.marketplace_svc
        .create_listing(claims.tenant_id, carrier.id.inner(), body.listing)
        .await?;
    Ok::<_, AppError>((StatusCode::CREATED, Json(listing)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use axum::{
        body::Body,
        http::{header, Method, Request},
        middleware::Next,
        Router,
    };
    use chrono::{Duration, Utc};
    use logisticos_auth::{claims::Claims, jwt::JwtService};
    use logisticos_events::producer::KafkaProducer;
    use logisticos_types::TenantId;
    use tower::ServiceExt;

    use crate::application::services::CarrierService;
    use crate::domain::entities::{
        Carrier, CarrierId, MarketplaceBooking, SizeClass, SlaCommitment, SlaRecord, VehicleListing,
        ZoneSlaRow,
    };
    use crate::domain::repositories::{
        BreachReasonRow, CarrierRepository, MarketplaceRepository, SlaRecordRepository,
    };
    use crate::infrastructure::{external::ExternalCarrierClient, messaging::CarrierPublisher};
    use crate::infrastructure::adapters::AdapterRegistry;

    // ── Doubles ───────────────────────────────────────────────────────────

    #[derive(Default)]
    struct Carriers(Mutex<Vec<Carrier>>);

    #[async_trait]
    impl CarrierRepository for Carriers {
        async fn find_by_id(&self, id: &CarrierId) -> anyhow::Result<Option<Carrier>> {
            Ok(self.0.lock().unwrap().iter().find(|c| c.id == *id).cloned())
        }
        async fn find_by_code(&self, _t: &TenantId, _c: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
        async fn find_by_contact_email(&self, _t: &TenantId, _e: &str) -> anyhow::Result<Option<Carrier>> { Ok(None) }
        async fn list(&self, _t: &TenantId, _l: i64, _o: i64) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
        async fn list_active(&self, _t: &TenantId) -> anyhow::Result<Vec<Carrier>> { Ok(vec![]) }
        async fn save(&self, c: &Carrier) -> anyhow::Result<()> {
            self.0.lock().unwrap().push(c.clone());
            Ok(())
        }
    }

    struct NoSla;

    #[async_trait]
    impl SlaRecordRepository for NoSla {
        async fn create(&self, _r: &SlaRecord) -> anyhow::Result<()> { Ok(()) }
        async fn find_by_shipment(&self, _s: Uuid) -> anyhow::Result<Option<SlaRecord>> { Ok(None) }
        async fn save_outcome(&self, _r: &SlaRecord) -> anyhow::Result<()> { Ok(()) }
        async fn list_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<SlaRecord>> { Ok(vec![]) }
        async fn zone_summary(&self, _c: Uuid, _f: chrono::DateTime<Utc>, _t: chrono::DateTime<Utc>) -> anyhow::Result<Vec<ZoneSlaRow>> { Ok(vec![]) }
        async fn breach_reasons(&self, _c: Uuid, _f: chrono::DateTime<Utc>, _t: chrono::DateTime<Utc>) -> anyhow::Result<Vec<BreachReasonRow>> { Ok(vec![]) }
    }

    #[derive(Default)]
    struct Market {
        listings: Mutex<Vec<VehicleListing>>,
        bookings: Mutex<Vec<MarketplaceBooking>>,
    }

    #[async_trait]
    impl MarketplaceRepository for Market {
        async fn create_listing(&self, l: &VehicleListing) -> anyhow::Result<()> {
            self.listings.lock().unwrap().push(l.clone());
            Ok(())
        }
        async fn find_listing_by_id(&self, _id: Uuid) -> anyhow::Result<Option<VehicleListing>> { Ok(None) }
        async fn list_listings_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<VehicleListing>> { Ok(vec![]) }
        async fn list_listings_by_tenant(&self, t: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<VehicleListing>> {
            Ok(self.listings.lock().unwrap().iter().filter(|l| l.tenant_id == t).cloned().collect())
        }
        async fn update_listing(&self, _l: &VehicleListing) -> anyhow::Result<()> { Ok(()) }
        async fn delete_listing(&self, _id: Uuid) -> anyhow::Result<bool> { Ok(true) }
        async fn find_available_listings(
            &self, _t: Uuid, _w: f32, _s: Option<&str>, _at: chrono::DateTime<Utc>, _l: i64,
        ) -> anyhow::Result<Vec<VehicleListing>> { Ok(vec![]) }
        async fn create_booking(&self, _b: &MarketplaceBooking) -> anyhow::Result<()> { Ok(()) }
        async fn save_booking_payment_reference(&self, _b: &MarketplaceBooking) -> anyhow::Result<()> { Ok(()) }
        async fn list_bookings_by_booker(&self, _t: Uuid, _u: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
        async fn list_pending_bookings(&self, _l: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
        async fn find_booking_by_id(&self, _id: Uuid) -> anyhow::Result<Option<MarketplaceBooking>> { Ok(None) }
        async fn list_bookings_by_carrier(&self, _c: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> { Ok(vec![]) }
        async fn list_bookings_by_tenant(&self, t: Uuid, _l: i64, _o: i64) -> anyhow::Result<Vec<MarketplaceBooking>> {
            Ok(self.bookings.lock().unwrap().iter().filter(|b| b.tenant_id == t).cloned().collect())
        }
        async fn save_booking(&self, _b: &MarketplaceBooking) -> anyhow::Result<()> { Ok(()) }
    }

    fn noop_kafka() -> Arc<KafkaProducer> {
        let cluster = rdkafka::mocking::MockCluster::new(1).expect("mock kafka cluster");
        let brokers = cluster.bootstrap_servers();
        Box::leak(Box::new(cluster));
        Arc::new(KafkaProducer::new(&brokers).expect("noop kafka producer"))
    }

    fn carrier_in(tenant: Uuid) -> Carrier {
        Carrier::new(
            TenantId::from_uuid(tenant), "Acme".into(), "ACME".into(), "ops@acme.test".into(),
            SlaCommitment { on_time_target_pct: 90.0, max_delivery_days: 2, penalty_per_breach: 100 },
        )
    }

    fn listing_in(tenant: Uuid, carrier: Uuid) -> VehicleListing {
        VehicleListing::new(
            tenant, carrier, "ABC-123".into(), SizeClass::Van, 1000.0, None, 5000, 200, None,
            "Metro".into(), Utc::now(), Utc::now() + Duration::hours(8), 15,
        )
    }

    struct Fixture {
        app:      Router,
        market:   Arc<Market>,
        carriers: Arc<Carriers>,
        /// A carrier that belongs to the caller's tenant.
        carrier:  Carrier,
    }

    /// The real router over in-memory repos, with the caller's claims injected
    /// the way `require_auth` would.
    fn fixture(tenant: Uuid, perms: &[&str]) -> Fixture {
        let carriers = Arc::new(Carriers::default());
        let carrier = carrier_in(tenant);
        carriers.0.lock().unwrap().push(carrier.clone());
        let market = Arc::new(Market::default());

        let kafka = noop_kafka();
        let state = AppState {
            carrier_svc:      Arc::new(CarrierService::new(
                carriers.clone(),
                Arc::new(NoSla),
                Arc::new(CarrierPublisher::new(kafka.clone())),
                Arc::new(ExternalCarrierClient::new(None)),
            )),
            marketplace_svc:  Arc::new(MarketplaceService::new(market.clone(), kafka)),
            jwt:              Arc::new(JwtService::new("test-secret", 300, 600)),
            adapter_registry: Arc::new(AdapterRegistry::new()),
            storage:          None,
            booking_repo:     None,
        };
        let claims = Claims::new(
            Uuid::new_v4(), tenant, "t".into(), "enterprise".into(), "admin@t.test".into(),
            vec![], perms.iter().map(|p| p.to_string()).collect(), 300,
        );
        let app = crate::api::http::router()
            .layer(axum::middleware::from_fn(move |mut req: Request<Body>, next: Next| {
                let claims = claims.clone();
                async move {
                    req.extensions_mut().insert(claims);
                    next.run(req).await
                }
            }))
            .with_state(state);
        Fixture { app, market, carriers, carrier }
    }

    async fn call(app: Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let r = app.oneshot(req).await.unwrap();
        let s = r.status();
        let b = axum::body::to_bytes(r.into_body(), usize::MAX).await.unwrap();
        (s, serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null))
    }

    fn get(uri: &str) -> Request<Body> {
        Request::builder().method(Method::GET).uri(uri).body(Body::empty()).unwrap()
    }

    fn post_listing(carrier_id: Uuid) -> Request<Body> {
        let body = serde_json::json!({
            "carrier_id": carrier_id,
            "vehicle_plate": "XYZ-999", "size_class": "van", "max_weight_kg": 900.0,
            "base_price_cents": 5000, "per_km_cents": 200,
            "service_area_label": "Metro",
            "idle_from": Utc::now(), "idle_until": Utc::now() + Duration::hours(6),
        });
        Request::builder().method(Method::POST).uri("/v1/marketplace/admin/listings")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())).unwrap()
    }

    // ── Tenant scoping ────────────────────────────────────────────────────

    #[tokio::test]
    async fn admin_lists_every_carriers_listings_in_the_tenant_and_no_other() {
        let tenant = Uuid::new_v4();
        let f = fixture(tenant, &[permissions::CARRIERS_MANAGE]);
        {
            let mut l = f.market.listings.lock().unwrap();
            l.push(listing_in(tenant, f.carrier.id.inner()));
            l.push(listing_in(tenant, Uuid::new_v4()));        // other carrier, same tenant
            l.push(listing_in(Uuid::new_v4(), Uuid::new_v4())); // another tenant
        }
        let (s, b) = call(f.app, get("/v1/marketplace/admin/listings")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["count"], 2);
        assert!(b["listings"].as_array().unwrap().iter()
            .all(|l| l["tenant_id"] == tenant.to_string()));
    }

    #[tokio::test]
    async fn admin_bookings_route_answers_ok_and_is_tenant_scoped_empty_when_none() {
        let f = fixture(Uuid::new_v4(), &[permissions::CARRIERS_MANAGE]);
        let (s, b) = call(f.app, get("/v1/marketplace/admin/bookings")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["count"], 0);
    }

    #[tokio::test]
    async fn create_for_a_carrier_in_this_tenant_succeeds_and_stamps_the_tenant() {
        let tenant = Uuid::new_v4();
        let f = fixture(tenant, &[permissions::CARRIERS_MANAGE]);
        let (s, b) = call(f.app, post_listing(f.carrier.id.inner())).await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(b["tenant_id"], tenant.to_string());
        assert_eq!(b["carrier_id"], f.carrier.id.inner().to_string());
        assert_eq!(f.market.listings.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_for_a_foreign_carrier_reads_as_missing_and_writes_nothing() {
        let f = fixture(Uuid::new_v4(), &[permissions::CARRIERS_MANAGE]);
        let foreign = carrier_in(Uuid::new_v4());
        f.carriers.0.lock().unwrap().push(foreign.clone());

        let (s_foreign, b_foreign) = call(f.app.clone(), post_listing(foreign.id.inner())).await;
        let (s_missing, b_missing) = call(f.app, post_listing(Uuid::new_v4())).await;

        assert_eq!(s_foreign, StatusCode::NOT_FOUND);
        assert_eq!(s_missing, StatusCode::NOT_FOUND);
        assert_eq!(b_foreign["error"]["code"], b_missing["error"]["code"]);
        assert!(f.market.listings.lock().unwrap().is_empty());
    }

    // ── Permission gate ───────────────────────────────────────────────────

    #[tokio::test]
    async fn carriers_read_alone_cannot_use_the_admin_routes() {
        let f = fixture(Uuid::new_v4(), &[permissions::CARRIERS_READ]);
        let (s1, _) = call(f.app.clone(), get("/v1/marketplace/admin/listings")).await;
        let (s2, _) = call(f.app.clone(), get("/v1/marketplace/admin/bookings")).await;
        let (s3, _) = call(f.app, post_listing(f.carrier.id.inner())).await;
        assert_eq!((s1, s2, s3), (StatusCode::FORBIDDEN, StatusCode::FORBIDDEN, StatusCode::FORBIDDEN));
        assert!(f.market.listings.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_partner_with_manage_own_cannot_use_the_admin_routes() {
        let f = fixture(
            Uuid::new_v4(),
            &[permissions::CARRIERS_READ, permissions::CARRIERS_MANAGE_OWN],
        );
        let (s, _) = call(f.app, get("/v1/marketplace/admin/listings")).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }
}
