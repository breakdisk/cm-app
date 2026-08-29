//! The tenant's own plan: what is on sale, what they are on, and how to change
//! it.
//!
//! There is deliberately no route here that sets a tier. `billing:subscribe`
//! reaches the catalogue, the checkout and the cancel button; the tier itself
//! moves only when a payment is captured, through a mesh-internal call that no
//! tenant credential can make. Adding a "set my tier" endpoint gated on this
//! permission would recreate exactly the free self-upgrade that keeps
//! `tenants:manage` ungranted.

use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, Json};
use serde::Deserialize;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::rbac::permissions;
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::application::services::subscription_service::SubscriptionError;
use crate::domain::entities::BillingInterval;

/// Falls back to USD, which is the currency the published pricing page quotes
/// and the currency migration 0019 seeds. A deployment billing in something
/// else seeds its own plan rows and passes `?currency=`.
const DEFAULT_CURRENCY: &str = "USD";

fn map_err(e: SubscriptionError) -> AppError {
    match e {
        SubscriptionError::NotSelfServe(m) => AppError::BusinessRule(m),
        SubscriptionError::NoPlan { .. }   => AppError::NotFound {
            resource: "SubscriptionPlan", id: e_to_id(&e),
        },
        SubscriptionError::PaymentsUnavailable => AppError::ServiceUnavailable(
            "Online card payment is not configured for this deployment — a plan \
             cannot be purchased here".into(),
        ),
        SubscriptionError::NoSubscription => AppError::NotFound {
            resource: "Subscription", id: "current".into(),
        },
        SubscriptionError::Rejected(m) => AppError::BusinessRule(m),
        SubscriptionError::Other(inner) => AppError::Internal(inner),
    }
}

fn e_to_id(e: &SubscriptionError) -> String {
    match e {
        SubscriptionError::NoPlan { tier, interval, currency } =>
            format!("{tier}/{interval}/{currency}"),
        _ => String::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct CurrencyQuery { currency: Option<String> }

/// `GET /v1/subscriptions/plans` — what a tenant can buy.
///
/// Readable by any authenticated caller: the same numbers are on the public
/// pricing page, and gating them would only stop the portal rendering them.
pub async fn list_plans(
    State(state): State<Arc<AppState>>,
    _claims: AuthClaims,
    Query(q): Query<CurrencyQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let svc = state.subscription_service.as_ref();
    let currency = q.currency.as_deref().unwrap_or(DEFAULT_CURRENCY);
    let plans = svc.list_plans(currency).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": { "plans": plans, "currency": currency } })))
}

/// `GET /v1/subscriptions/me` — the caller's tenant's current plan.
///
/// `null` for a tenant that has never subscribed, which is the Starter case and
/// not an error. `effective_tier` is reported alongside `tier` because the two
/// differ in exactly the states that matter: a lapsed subscription still
/// records what was bought while entitling the tenant to nothing.
pub async fn get_current(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::BILLING_SUBSCRIBE)?;
    let sub = state.subscription_service
        .current(claims.tenant_id)
        .await
        .map_err(AppError::Internal)?;

    let data = match sub {
        None => serde_json::json!(null),
        Some(s) => serde_json::json!({
            "id":                   s.id,
            "tier":                 s.tier,
            "effective_tier":       s.effective_tier(),
            "status":               s.status,
            "currency":             s.currency,
            "amount_cents":         s.amount_cents,
            "current_period_start": s.current_period_start,
            "current_period_end":   s.current_period_end,
            "cancelled_at":         s.cancelled_at,
            // Surfaced rather than hidden: it is the difference between "you
            // have paid and we are still granting it" and "something is wrong",
            // and support cannot diagnose the second without seeing it.
            "entitlement_synced":   s.tier_synced_at.is_some(),
        }),
    };
    Ok(Json(serde_json::json!({ "data": data })))
}

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    /// `growth` or `business`. `starter` and `enterprise` are refused by name —
    /// one is free and one is quoted by hand.
    pub tier:     String,
    /// `monthly` or `annual`.
    pub interval: String,
    #[serde(default)]
    pub currency: Option<String>,
}

/// `POST /v1/subscriptions/checkout` — buy or change a plan.
///
/// Returns a hosted card page and changes nothing about the tenant's
/// entitlement. A tenant who abandons the page keeps exactly what they had.
pub async fn checkout(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
    Json(req): Json<CheckoutRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    claims.require_permission(permissions::BILLING_SUBSCRIBE)?;

    let interval = BillingInterval::parse(&req.interval).ok_or_else(|| {
        AppError::Validation(format!(
            "unknown billing interval {:?} — expected \"monthly\" or \"annual\"", req.interval
        ))
    })?;
    let currency = req.currency.as_deref().unwrap_or(DEFAULT_CURRENCY);

    // Tenant from the validated token, never the body: a caller-supplied tenant
    // would let one tenant buy a plan that lands on another's account.
    let out = state.subscription_service
        .checkout(claims.tenant_id, &req.tier, interval, currency)
        .await
        .map_err(map_err)?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "data": {
            "subscription_id": out.subscription.id,
            "tier":            out.subscription.tier,
            "amount_cents":    out.subscription.amount_cents,
            "currency":        out.subscription.currency,
            "checkout_url":    out.checkout_url,
        }})),
    ))
}

/// `POST /v1/subscriptions/me/cancel` — stop at the end of the paid period.
///
/// Not a refund and not an immediate downgrade: the tenant keeps the tier they
/// bought until it runs out, and the sweep lapses it then.
pub async fn cancel(
    State(state): State<Arc<AppState>>,
    claims: AuthClaims,
) -> Result<Json<serde_json::Value>, AppError> {
    claims.require_permission(permissions::BILLING_SUBSCRIBE)?;
    let sub = state.subscription_service
        .cancel(claims.tenant_id)
        .await
        .map_err(map_err)?;

    Ok(Json(serde_json::json!({ "data": {
        "status":             sub.status,
        "current_period_end": sub.current_period_end,
        "tier_until_then":    sub.effective_tier(),
    }})))
}
