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

/// GET /v1/marketplace/admin/listings?limit=&offset=
///
/// Pages newest-first. `count` is the size of this page (as before); `total`
/// is every listing in the tenant, and `limit`/`offset` are the window that
/// was actually applied (the service holds `limit` to 1..=100), so a client
/// can tell a full list from a cut-off one and ask for the next page.
pub async fn admin_list_listings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AdminPaginationQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let (limit, offset) = MarketplaceService::clamp_page(q.limit.unwrap_or(50), q.offset.unwrap_or(0));
    let listings = state.marketplace_svc
        .list_listings_for_tenant(claims.tenant_id, limit, offset)
        .await?;
    let total = state.marketplace_svc.count_listings_for_tenant(claims.tenant_id).await?;
    let count = listings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "listings": listings, "count": count, "total": total, "limit": limit, "offset": offset,
    }))))
}

/// GET /v1/marketplace/admin/bookings?limit=&offset=
///
/// Same paging contract as the listings route. `total` counts what the list
/// can reach: unfunded online holds are in neither.
pub async fn admin_list_bookings(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<AdminPaginationQuery>,
) -> impl IntoResponse {
    claims.require_permission(permissions::CARRIERS_MANAGE)?;
    let (limit, offset) = MarketplaceService::clamp_page(q.limit.unwrap_or(50), q.offset.unwrap_or(0));
    let bookings: Vec<_> = state.marketplace_svc
        .list_bookings_for_tenant(claims.tenant_id, limit, offset)
        .await?
        .into_iter()
        .map(MarketplaceService::apply_pii_mask)
        .collect();
    let total = state.marketplace_svc.count_bookings_for_tenant(claims.tenant_id).await?;
    let count = bookings.len();
    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({
        "bookings": bookings, "count": count, "total": total, "limit": limit, "offset": offset,
    }))))
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
    use std::sync::Arc;

    use axum::{
        body::Body,
        http::{header, Method, Request},
        middleware::Next,
        Router,
    };
    use chrono::{Duration, Utc};
    use logisticos_auth::claims::Claims;
    use logisticos_types::TenantId;
    use tower::ServiceExt;

    use crate::domain::entities::{Carrier, SizeClass, SlaCommitment, VehicleListing};
    use crate::infrastructure::adapters::AdapterRegistry;
    use crate::test_support::{app_state, Carriers, Market};

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

        let state = app_state(carriers.clone(), market.clone(), AdapterRegistry::new(), "test-secret", None);
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

    // ── Pagination ────────────────────────────────────────────────────────

    fn booking_in(tenant: Uuid, online_pending: bool) -> crate::domain::entities::MarketplaceBooking {
        use crate::domain::entities::{BookingPaymentMethod, BookingPaymentStatus, MarketplaceBooking};
        let listing = listing_in(tenant, Uuid::new_v4());
        let mut b = MarketplaceBooking::place(
            &listing, Uuid::new_v4(), Uuid::new_v4(), "CM-TEST".into(), "Ann".into(), None,
            "A".into(), "B".into(), 10.0, None, 5.0, Utc::now(),
        );
        if online_pending {
            b.payment_method = BookingPaymentMethod::Online;
            b.payment_status = BookingPaymentStatus::Pending;
        }
        b
    }

    #[tokio::test]
    async fn admin_listings_report_the_tenant_total_beyond_the_page() {
        let tenant = Uuid::new_v4();
        let f = fixture(tenant, &[permissions::CARRIERS_MANAGE]);
        {
            let mut l = f.market.listings.lock().unwrap();
            for _ in 0..5 { l.push(listing_in(tenant, Uuid::new_v4())); }
            l.push(listing_in(Uuid::new_v4(), Uuid::new_v4())); // another tenant
        }
        let (s, b) = call(f.app.clone(), get("/v1/marketplace/admin/listings?limit=2&offset=0")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["count"], 2, "count stays the size of this page");
        assert_eq!(b["total"], 5, "total is the whole tenant, not the page");
        assert_eq!(b["limit"], 2);
        assert_eq!(b["offset"], 0);

        let (_, last) = call(f.app, get("/v1/marketplace/admin/listings?limit=2&offset=4")).await;
        assert_eq!(last["count"], 1);
        assert_eq!(last["total"], 5);
        assert_eq!(last["offset"], 4);
    }

    #[tokio::test]
    async fn admin_listings_echo_the_window_actually_applied() {
        let f = fixture(Uuid::new_v4(), &[permissions::CARRIERS_MANAGE]);
        let (_, b) = call(f.app.clone(), get("/v1/marketplace/admin/listings")).await;
        assert_eq!(b["limit"], 50, "default page size");
        let (_, huge) = call(f.app, get("/v1/marketplace/admin/listings?limit=100000&offset=-9")).await;
        assert_eq!(huge["limit"], 100, "service caps the page at 100");
        assert_eq!(huge["offset"], 0);
    }

    #[tokio::test]
    async fn admin_bookings_total_excludes_unfunded_online_holds_and_other_tenants() {
        let tenant = Uuid::new_v4();
        let f = fixture(tenant, &[permissions::CARRIERS_MANAGE]);
        {
            let mut bk = f.market.bookings.lock().unwrap();
            for _ in 0..3 { bk.push(booking_in(tenant, false)); }
            bk.push(booking_in(tenant, true));              // hold not landed: not an offer
            bk.push(booking_in(Uuid::new_v4(), false));     // another tenant
        }
        let (s, b) = call(f.app, get("/v1/marketplace/admin/bookings?limit=2")).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(b["count"], 2);
        assert_eq!(b["total"], 3);
        assert_eq!(b["limit"], 2);
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
