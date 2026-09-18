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

use crate::application::{
    CommitRequest, CreateCorporateRequest, CreateOfferRequest, GrantCreditRequest, PriceRequest, Promotions,
};
use crate::domain::tier::NewTier;

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
        .route("/v1/promotions/loyalty/me", get(loyalty))
        .route("/v1/promotions/credits/me", get(credits))
        .route("/v1/promotions/referrals/me", get(referrals))
        .route("/v1/promotions/referrals/claim", post(claim_referral))
        .route("/v1/promotions/corporate/me", get(corporate).post(link_corporate).delete(unlink_corporate))
        .route("/v1/promotions/admin/tiers", get(admin_tiers).put(admin_replace_tiers))
        .route("/v1/promotions/admin/corporate-accounts", get(admin_corporates).post(admin_create_corporate))
        .route("/v1/promotions/admin/credits", post(admin_grant_credit))
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

/// `GET /v1/promotions/loyalty/me` — tier, progress and the ladder. Gated
/// by the tenant's plan: without `loyalty_program` it reads as disabled.
async fn loyalty(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let enabled = claims.has_feature("loyalty_program");
    let view = s.promotions.loyalty(claims.tenant_id, claims.user_id, enabled, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

/// `GET /v1/promotions/credits/me` — balance and history. Credit is a
/// non-cash entitlement: there is no route that withdraws or tops it up.
async fn credits(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.credit(claims.tenant_id, claims.user_id, claims.currency.as_deref()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

async fn referrals(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.referrals(claims.tenant_id, claims.user_id, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

#[derive(Deserialize)]
struct CodeBody {
    #[serde(default)]
    code: Option<String>,
}

/// `POST /v1/promotions/referrals/claim` — a new account enters a friend's
/// code. A refusal is a 200 with the rule it failed.
async fn claim_referral(State(s): Shared, claims: AuthClaims, Json(body): Json<CodeBody>) -> Result<Json<serde_json::Value>, AppError> {
    let code = body.code.unwrap_or_default();
    let view = s.promotions.claim_referral(claims.tenant_id, claims.user_id, &code).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

async fn corporate(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    let view = s.promotions.corporate(claims.tenant_id, claims.user_id, &claims.email).await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

/// `POST /v1/promotions/corporate/me` — link by company code, or with no
/// code, to the firm matching this account's work email.
async fn link_corporate(State(s): Shared, claims: AuthClaims, Json(body): Json<CodeBody>) -> Result<Json<serde_json::Value>, AppError> {
    let view = s
        .promotions
        .link_corporate(claims.tenant_id, claims.user_id, body.code.as_deref(), &claims.email)
        .await?;
    Ok(Json(serde_json::json!({ "data": view })))
}

async fn unlink_corporate(State(s): Shared, claims: AuthClaims) -> Result<StatusCode, AppError> {
    s.promotions.unlink_corporate(claims.tenant_id, claims.user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Admin ───────────────────────────────────────────────────────────────────

async fn admin_tiers(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let view = s.promotions.loyalty(claims.tenant_id, claims.user_id, true, Utc::now()).await?;
    Ok(Json(serde_json::json!({ "data": view.ladder })))
}

/// `PUT /v1/promotions/admin/tiers` — the ladder, replaced whole.
async fn admin_replace_tiers(State(s): Shared, claims: AuthClaims, Json(tiers): Json<Vec<NewTier>>) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let ladder = s.promotions.replace_ladder(claims.tenant_id, tiers).await?;
    tracing::info!(tenant_id = %claims.tenant_id, user_id = %claims.user_id, tiers = ladder.len(), "loyalty ladder replaced");
    Ok(Json(serde_json::json!({ "data": ladder })))
}

async fn admin_corporates(State(s): Shared, claims: AuthClaims) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    Ok(Json(serde_json::json!({ "data": s.promotions.list_corporates(claims.tenant_id).await? })))
}

async fn admin_create_corporate(
    State(s): Shared,
    claims: AuthClaims,
    Json(req): Json<CreateCorporateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    claims.require_permission(permissions::CAMPAIGNS_CREATE)?;
    let corp = s.promotions.create_corporate(claims.tenant_id, req).await?;
    tracing::info!(tenant_id = %claims.tenant_id, user_id = %claims.user_id, code = %corp.code, "corporate rate created");
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": corp }))))
}

/// `POST /v1/promotions/admin/credits` — goodwill credit. Money-shaped, so
/// it needs the billing permission, not the campaign one.
async fn admin_grant_credit(State(s): Shared, claims: AuthClaims, Json(req): Json<GrantCreditRequest>) -> Result<StatusCode, AppError> {
    claims.require_permission(permissions::BILLING_ADMIN)?;
    let (account, amount) = (req.account_id, req.amount_cents);
    s.promotions.grant_credit(claims.tenant_id, req).await?;
    tracing::info!(tenant_id = %claims.tenant_id, granted_by = %claims.user_id, %account, amount, "credit granted");
    Ok(StatusCode::NO_CONTENT)
}

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

async fn internal_redeem(State(s): Shared, Json(req): Json<CommitRequest>) -> Result<StatusCode, AppError> {
    s.promotions.commit(&req, Utc::now()).await?;
    let total: i64 = req.lines.iter().map(|l| l.amount_cents).sum();
    tracing::info!(
        tenant_id = %req.tenant_id, account_id = %req.account_id, shipment_id = %req.shipment_id,
        code = ?req.code, discount_cents = total, lines = req.lines.len(), "booking discounts committed"
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
