use std::sync::Arc;

use axum::{extract::{Path, Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
use logisticos_auth::middleware::AuthClaims;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::ProductKey;

#[derive(Debug, Deserialize)]
pub struct OfferRequest {
    pub product:      ProductKey,
    pub external_ref: Uuid,
    pub lat:          f64,
    pub lng:          f64,
    #[serde(default = "default_radius_km")]
    pub radius_km:    f64,
    #[serde(default = "default_fanout")]
    pub fanout:       i64,
    /// What the courier earns. Declared by the offering product — field-ops
    /// stores and credits it without interpreting how it was priced.
    #[serde(default)]
    pub trip_cents:   i64,
    #[serde(default)]
    pub tip_cents:    i64,
}

fn default_radius_km() -> f64 { 5.0 }
fn default_fanout() -> i64 { 5 }

#[derive(Debug, Serialize)]
pub struct OfferResponse {
    pub assignment_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
struct EarningEntry {
    kind:         String,
    /// Signed, as stored: credits positive, payouts negative, so a client
    /// summing the list gets the balance and cannot disagree with it.
    amount_cents: i64,
    external_ref: Option<Uuid>,
    at:           chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct EarningsResponse {
    period:        String,
    balance_cents: i64,
    entries:       Vec<EarningEntry>,
}

#[derive(Debug, Deserialize)]
pub struct SupplyQuery {
    lat: f64,
    lng: f64,
    #[serde(default = "default_supply_radius")]
    radius_km: f64,
}

fn default_supply_radius() -> f64 { 5.0 }

#[derive(Debug, Serialize)]
struct SupplyResponse {
    /// Couriers who could be offered this job right now. Capped — see
    /// `supply_near`. Zero is a real answer, not an error.
    available: usize,
}

#[derive(Debug, Serialize)]
struct OfferSummary {
    assignment_id: Uuid,
    product:       String,
    external_ref:  Uuid,
    /// What the offering product declared this job pays. The courier decides
    /// with this in front of them, so it is on the list, not behind a claim.
    trip_cents:    i64,
    tip_cents:     i64,
    offered_at:    chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
struct MyOffersResponse {
    offers: Vec<OfferSummary>,
}

#[derive(Debug, Serialize)]
struct ClaimResponse {
    pub won: bool,
}

#[derive(Debug, Deserialize)]
pub struct CollectedRequest {
    pub vendor_id: Uuid,
    /// Hardware clock at the scan. SLA maths uses this rather than server
    /// receipt time, so a slow upload is not billed to the courier.
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct DeliveredRequest {
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct PositionRequest {
    pub lat:              f64,
    pub lng:              f64,
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

// Every route is namespaced under `/v1/field-ops` because this is a platform
// tier, not a product service: the same paths are reachable by more than one
// product, and a flat resource name would collide with whichever product
// claimed it first. `/v1/assignments` is already owned by dispatch and called
// in production by the driver app (`PUT /v1/assignments/:id/accept`), so an
// unprefixed `/v1/assignments/offer` resolves to dispatch at the gateway and
// never reaches this service. The prefix is also stable under every gateway
// topology Plan 11 might land — one gateway, per-product gateways, or
// host-based routing — so it does not need revisiting when that lands.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/field-ops/assignments/offer", post(offer))
        .route("/v1/field-ops/assignments/mine", get(my_offers))
        .route("/v1/field-ops/assignments/:id/claim", post(claim))
        .route("/v1/field-ops/assignments/:id/collected", post(collected))
        .route("/v1/field-ops/assignments/:id/delivered", post(delivered))
        .route("/v1/field-ops/couriers/me/earnings", get(my_earnings))
        .route("/v1/field-ops/couriers/supply", get(supply))
        .route("/v1/field-ops/couriers/:id/position", post(position))
}

// Tenant comes from the validated JWT on every handler below, never from the
// body, a path segment, or shared app state. This service has no database-level
// isolation — the tenant_id bound into each repository query is the whole of
// it — so a tenant the caller could name would be no isolation at all.

async fn offer(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<OfferRequest>,
) -> Result<Json<OfferResponse>, StatusCode> {
    let offers = st
        .dispatch
        .offer_to_nearest(
            claims.tenant_id, req.product, req.external_ref,
            req.lat, req.lng, req.radius_km, req.fanout,
            req.trip_cents, req.tip_cents,
        )
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "offer failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(OfferResponse { assignment_ids: offers.iter().map(|a| a.id).collect() }))
}

/// `GET /v1/field-ops/assignments/mine` — what this courier has been offered.
///
/// The courier is resolved from the token, never from a query parameter: an id
/// the caller could name would let one courier read another's work queue.
async fn my_offers(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<MyOffersResponse>, StatusCode> {
    let offers = st
        .dispatch
        .offers_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "listing offers failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(MyOffersResponse {
        offers: offers
            .iter()
            .map(|a| OfferSummary {
                assignment_id: a.id,
                product:       a.product.as_str().to_string(),
                external_ref:  a.external_ref,
                trip_cents:    a.trip_cents,
                tip_cents:     a.tip_cents,
                offered_at:    a.offered_at,
            })
            .collect(),
    }))
}

async fn claim(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
) -> Result<Json<ClaimResponse>, StatusCode> {
    // A lost race is 200 { won: false }, not an error status. The client needs
    // to distinguish "someone else got it" from "the request failed".
    let won = st.dispatch.claim(claims.tenant_id, claims.user_id, id).await.map_err(|e| {
        tracing::error!(err = %e, "claim failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClaimResponse { won }))
}

/// `GET /v1/field-ops/couriers/me/earnings` — this period's ledger.
///
/// The courier comes from the token. Until now the only way to see whether a
/// delivery had actually been paid was to open psql, which meant a courier
/// could not answer their own most basic question.
async fn my_earnings(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<EarningsResponse>, StatusCode> {
    let ledger = st
        .dispatch
        .earnings_for_user(claims.tenant_id, claims.user_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "earnings lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // No ledger yet is a zero, not a 404: a courier who has started a shift and
    // not yet been paid has earnings, and they are nil.
    let Some(l) = ledger else {
        return Ok(Json(EarningsResponse {
            period:        crate::application::services::current_period(),
            balance_cents: 0,
            entries:       Vec::new(),
        }));
    };

    Ok(Json(EarningsResponse {
        period:        l.period.clone(),
        balance_cents: l.balance_cents,
        entries: l
            .entries
            .iter()
            .map(|e| EarningEntry {
                kind:         format!("{:?}", e.kind).to_lowercase(),
                amount_cents: e.amount_cents,
                external_ref: e.external_ref,
                at:           e.created_at,
            })
            .collect(),
    }))
}

/// `GET /v1/field-ops/couriers/supply?lat=&lng=&radius_km=`
///
/// Read-only capacity check for a product deciding whether it can promise a
/// delivery. Deliberately a count and not a list: a consuming product has no
/// business knowing which couriers exist, only whether anyone can take the job.
async fn supply(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Query(q): Query<SupplyQuery>,
) -> Result<Json<SupplyResponse>, StatusCode> {
    let available = st
        .dispatch
        .supply_near(claims.tenant_id, q.lat, q.lng, q.radius_km)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "supply lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(SupplyResponse { available }))
}

async fn position(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(courier_id): Path<Uuid>,
    Json(req): Json<PositionRequest>,
) -> Result<StatusCode, StatusCode> {
    st.dispatch
        .record_position(claims.tenant_id, courier_id, req.lat, req.lng, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "position ingest failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::ACCEPTED)
}

async fn collected(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<CollectedRequest>,
) -> Result<StatusCode, StatusCode> {
    let found = st
        .dispatch
        .mark_collected(claims.tenant_id, id, req.vendor_id, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "collected failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // 404 rather than a cheerful 202: a milestone reported against an
    // assignment that does not exist means the app is holding a stale id, and
    // silently accepting it would lose a real collection.
    if !found {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::ACCEPTED)
}

async fn delivered(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<DeliveredRequest>,
) -> Result<StatusCode, StatusCode> {
    let found = st
        .dispatch
        .mark_delivered(claims.tenant_id, id, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "delivered failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !found {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::ACCEPTED)
}
