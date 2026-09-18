//! Three surfaces, three trust levels.
//!
//! - **Customer** (`/v1/promotions/...`, JWT): the offers feed and "may I use
//!   this code". Tenant and account come from the token, never the request.
//! - **Admin** (`/v1/promotions/admin/...`, JWT + `campaigns:create`): the
//!   tenant's offers.
//! - **Internal** (`/v1/internal/promotions/...`, no JWT): pricing and
//!   redemption for order-intake. The gateway refuses every `/internal/` path;
//!   Istio mTLS asserts the caller inside the mesh.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use logisticos_auth::{middleware::AuthClaims, rbac::permissions};
use logisticos_errors::AppError;
use serde::Deserialize;
use uuid::Uuid;

use crate::application::{CreateOfferRequest, PriceRequest, Promotions, RedeemRequest};

pub mod health;

pub struct AppState {
    pub promotions: Arc<Promotions>,
    pub jwt: Arc<logisticos_auth::jwt::JwtService>,
}

type Shared = State<Arc<AppState>>;

pub fn router(state: Arc<AppState>) -> Router {
    // Health outside every layer: a 401 on a probe reads as a dead service.
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    let authed = Router::new()
        .route("/v1/promotions/offers", get(offers))
        .route("/v1/promotions/codes/:code/validate", post(validate))
        .route("/v1/promotions/admin/offers", get(admin_list).post(admin_create))
        .route("/v1/promotions/admin/offers/:id/deactivate", post(admin_deactivate))
        .route("/v1/promotions/admin/offers/:id/activate", post(admin_activate))
        .layer(auth_layer);

    let internal = Router::new()
        .route("/v1/internal/promotions/price", post(internal_price))
        .route("/v1/internal/promotions/redeem", post(internal_redeem))
        .route("/v1/internal/promotions/release", post(internal_release));

    Router::new()
        .merge(health::routes())
        .merge(authed.merge(internal).with_state(state))
}

// ── Customer ────────────────────────────────────────────────────────────────

/// `GET /v1/promotions/offers` — this account's offers and the month grid.
async fn offers(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.offers_for(claims.tenant_id, claims.user_id, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

/// `POST /v1/promotions/codes/:code/validate` — may I use this code today.
/// A refusal is a 200 with `ok: false` and the rule it failed: the app shows
/// it under the field, and a 4xx would read as the request being wrong.
async fn validate(
    State(s): Shared,
    claims: AuthClaims,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.validate(claims.tenant_id, claims.user_id, &code, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

// ── Admin ───────────────────────────────────────────────────────────────────

async fn admin_list(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let offers = s.promotions.list_offers(claims.tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": offers })))
}

async fn admin_create(
    State(s): Shared,
    claims: AuthClaims,
    Json(req): Json<CreateOfferRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let offer = s.promotions.create_offer(claims.tenant_id, req).await?;
    tracing::info!(tenant_id = %claims.tenant_id, user_id = %claims.user_id, code = %offer.code, "offer created");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": offer }))))
}

async fn admin_deactivate(State(s): Shared, claims: AuthClaims, Path(id): Path<Uuid>) -> Result<StatusCode, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    s.promotions.set_active(claims.tenant_id, id, false).await?;
    tracing::info!(tenant_id = %claims.tenant_id, user_id = %claims.user_id, offer_id = %id, "offer deactivated");
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_activate(State(s): Shared, claims: AuthClaims, Path(id): Path<Uuid>) -> Result<StatusCode, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    s.promotions.set_active(claims.tenant_id, id, true).await?;
    tracing::info!(tenant_id = %claims.tenant_id, user_id = %claims.user_id, offer_id = %id, "offer activated");
    Ok(StatusCode::NO_CONTENT)
}

// ── Internal (order-intake) ─────────────────────────────────────────────────

async fn internal_price(State(s): Shared, Json(req): Json<PriceRequest>) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.price(&req, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

async fn internal_redeem(State(s): Shared, Json(req): Json<RedeemRequest>) -> Result<StatusCode, AppError> {
    s.promotions.redeem(&req, Utc::now()).await?;
    tracing::info!(
        tenant_id = %req.tenant_id, account_id = %req.account_id, shipment_id = %req.shipment_id,
        code = %req.code, discount_cents = req.discount_cents, "code redeemed"
    );
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct ReleaseRequest {
    shipment_id: Uuid,
}

/// A booking that failed after its code was spent gives it back at once,
/// rather than waiting for a `shipment.cancelled` that will never come.
async fn internal_release(State(s): Shared, Json(req): Json<ReleaseRequest>) -> Result<StatusCode, AppError> {
    s.promotions.release(req.shipment_id, Utc::now()).await?;
    Ok(StatusCode::NO_CONTENT)
}
