//! HTTP route handlers for the connectors service.
//!
//! Two route groups:
//! - **Unauthenticated**: `/v1/connectors/{platform}/{tenant_id}/webhook`
//!   Secured by HMAC — no JWT. Returns 200 immediately to satisfy platform retry logic.
//! - **Authenticated**: `/v1/connectors/credentials`
//!   JWT-protected CRUD for managing per-tenant connector credentials.

use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
    routing::{delete, post},
    Router,
};
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use serde::Deserialize;
use uuid::Uuid;

use crate::application::ConnectorService;

// ── AppState ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub svc:        Arc<ConnectorService>,
    pub jwt:        Arc<logisticos_auth::jwt::JwtService>,
    pub public_url: String,  // base URL used to build webhook URLs shown to merchants
    /// `None` when this deployment runs no OmniDeliv tier. The catalog sync
    /// route then answers 501 rather than 500 — "this platform does not do
    /// that" reads differently from "this platform is broken".
    pub omnideliv:  Option<Arc<crate::infrastructure::omnideliv_client::OmniDelivClient>>,
    /// One pooled client for outbound calls to merchant storefronts. Building a
    /// `reqwest::Client` per request throws away the connection pool, which for
    /// a paginated catalog sync means a fresh TLS handshake per page.
    pub http:       reqwest::Client,
}

// ── Unauthenticated webhook handlers ─────────────────────────────────────────

/// POST /v1/connectors/shopify/{tenant_id}/webhook
async fn shopify_webhook(
    State(state):            State<AppState>,
    Path(tenant_id):         Path<Uuid>,
    headers:                 HeaderMap,
    body:                    Bytes,
) -> impl IntoResponse {
    let topic = headers
        .get("X-Shopify-Topic")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let hmac_header = headers
        .get("X-Shopify-Hmac-Sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match state.svc.handle_shopify_webhook(tenant_id, topic, &body, hmac_header).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(AppError::Unauthorized(msg)) => {
            tracing::warn!(tenant_id = %tenant_id, msg, "shopify webhook auth failed");
            StatusCode::UNAUTHORIZED.into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, tenant_id = %tenant_id, "shopify webhook error");
            // Return 200 to prevent Shopify from retrying on our internal errors.
            // Shopify will retry on 5xx — we log and swallow to avoid spam.
            StatusCode::OK.into_response()
        }
    }
}

/// POST /v1/connectors/woocommerce/{tenant_id}/webhook
async fn woocommerce_webhook(
    State(state):    State<AppState>,
    Path(tenant_id): Path<Uuid>,
    headers:         HeaderMap,
    body:            Bytes,
) -> impl IntoResponse {
    let event = headers
        .get("X-WC-Webhook-Topic")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let sig_header = headers
        .get("X-WC-Webhook-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    match state.svc.handle_woocommerce_webhook(tenant_id, event, &body, sig_header).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(AppError::Unauthorized(msg)) => {
            tracing::warn!(tenant_id = %tenant_id, msg, "woocommerce webhook auth failed");
            StatusCode::UNAUTHORIZED.into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, tenant_id = %tenant_id, "woocommerce webhook error");
            // Return 200 — WooCommerce retries on non-2xx; log and swallow internal errors.
            StatusCode::OK.into_response()
        }
    }
}

// ── Authenticated credentials management ─────────────────────────────────────

#[derive(Deserialize)]
struct UpsertCredentialsBody {
    platform:       String,
    webhook_secret: String,
    config:         serde_json::Value,
}

/// GET /v1/connectors/credentials — list all active connectors for this tenant
async fn list_credentials(
    State(state): State<AppState>,
    claims: AuthClaims,
) -> impl IntoResponse {
    let result = state.svc.list_credentials(claims.tenant_id, &state.public_url).await;
    match result {
        Ok(list) => Ok::<_, AppError>((StatusCode::OK, Json(list))),
        Err(e)   => Err(e),
    }
}

/// POST /v1/connectors/credentials — create or update connector credentials
async fn upsert_credentials(
    State(state): State<AppState>,
    claims: AuthClaims,
    Json(body): Json<UpsertCredentialsBody>,
) -> impl IntoResponse {
    use crate::domain::entities::{ConnectorCredentials, Platform};
    use chrono::Utc;

    let platform = Platform::from_str(&body.platform).ok_or_else(|| {
        AppError::Validation(format!("Unknown platform '{}'. Supported: shopify, woocommerce", body.platform))
    })?;

    let creds = ConnectorCredentials {
        id:             uuid::Uuid::new_v4(),
        tenant_id:      claims.tenant_id,
        merchant_id:    claims.user_id,
        tenant_slug:    claims.tenant_slug.clone(),
        platform,
        webhook_secret: body.webhook_secret,
        config:         body.config,
        is_active:      true,
        created_at:     Utc::now(),
    };

    state.svc.creds_repo.upsert(&creds).await?;

    let summary = crate::application::connector_service::CredentialsSummary {
        id:          creds.id,
        platform:    creds.platform.as_str().to_string(),
        is_active:   true,
        webhook_url: format!(
            "{}/v1/connectors/{}/{}/webhook",
            state.public_url.trim_end_matches('/'),
            creds.platform.as_str(),
            creds.tenant_id,
        ),
        created_at: creds.created_at,
    };

    Ok::<_, AppError>((StatusCode::CREATED, Json(summary)))
}

/// POST /v1/connectors/catalog/sync — pull this merchant's products into their
/// OmniDeliv storefront.
///
/// Explicitly triggered rather than scheduled. A cron would need a scheduler,
/// a per-tenant cadence and a backfill story, and none of that is worth
/// inventing before a single merchant has synced once; a "Sync now" button and
/// a later cron calling this same route are the same endpoint.
///
/// It confirms nothing. Everything it writes lands unconfirmed and shows up in
/// the vendor's console as needing a human — which is the whole reason the
/// ingest port exists rather than each adapter writing to the catalog directly.
async fn sync_catalog(
    State(state): State<AppState>,
    claims: AuthClaims,
    Query(q): Query<SyncQuery>,
) -> impl IntoResponse {
    let client = state.omnideliv.as_ref().ok_or_else(|| AppError::Validation(
        "this deployment has no OmniDeliv tier configured (OMNIDELIV__INTERNAL_URL)".into(),
    ))?;

    // Which shop to pull from. Resolved rather than defaulted: a merchant with
    // both connectors and no `platform` would otherwise silently get whichever
    // one this code happened to try first, and a Woo menu would quietly
    // overwrite a Shopify one every sync.
    let creds = match q.platform.as_deref() {
        Some(p) => {
            let p = p.to_lowercase();
            if p != "shopify" && p != "woocommerce" {
                return Err(AppError::Validation(format!(
                    "cannot sync a catalog from '{p}' — supported: shopify, woocommerce"
                )));
            }
            state.svc.creds_repo.find(claims.tenant_id, &p).await?.ok_or_else(|| {
                AppError::Validation(format!("no active {p} connector for this tenant"))
            })?
        }
        None => {
            let mut linked: Vec<_> = state
                .svc
                .creds_repo
                .list_for_tenant(claims.tenant_id)
                .await?
                .into_iter()
                .filter(|c| {
                    c.omnideliv_vendor_id().is_some()
                        && matches!(c.platform.as_str(), "shopify" | "woocommerce")
                })
                .collect();

            match linked.len() {
                0 => return Err(AppError::Validation(
                    "no shop is linked to an OmniDeliv store — connect Shopify or \
                     WooCommerce and set `omnideliv_vendor_id` on it".into(),
                )),
                1 => linked.remove(0),
                _ => return Err(AppError::Validation(
                    "more than one shop is linked to a store — say which with \
                     ?platform=shopify or ?platform=woocommerce".into(),
                )),
            }
        }
    };

    // The association a person has to make: which storefront this shop's menu
    // belongs to. A parcel merchant has a connector and no storefront, and that
    // is a normal state rather than a failure.
    let vendor_id = creds.omnideliv_vendor_id().ok_or_else(|| AppError::Validation(
        "this connector is not linked to an OmniDeliv store — set \
         `omnideliv_vendor_id` in its config".into(),
    ))?;

    let platform = creds.platform.as_str();
    // Each adapter's only job is to produce these. Everything about what a sync
    // may overwrite — and what it may never assert — lives in omnideliv.
    let (items, deferred, unpriced) = match platform {
        "shopify" => {
            let items = crate::adapters::shopify_catalog::fetch_products(&state.http, &creds).await?;
            (items, 0usize, 0usize)
        }
        "woocommerce" => {
            let m = crate::adapters::woocommerce_catalog::fetch_products(&state.http, &creds).await?;
            (m.items, m.deferred_variable, m.unpriced)
        }
        other => return Err(AppError::Validation(format!(
            "the {other} connector has no catalog adapter"
        ))),
    };
    let fetched = items.len();

    let report = client
        .ingest_catalog(claims.tenant_id, &claims.tenant_slug, vendor_id, platform, &items)
        .await?;

    tracing::info!(
        tenant_id = %claims.tenant_id, vendor_id = %vendor_id, platform, fetched,
        created = report.created, updated = report.updated, rejected = report.rejected,
        deferred, unpriced,
        "catalog sync complete",
    );

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "platform": platform,
            "fetched":  fetched,
            "created":  report.created,
            "updated":  report.updated,
            "rejected": report.rejected,
            // Reported, never merely logged. A sync that dropped rows must not
            // be able to look like a complete one.
            "deferred": deferred,
            "unpriced": unpriced,
            // Said out loud because it is the surprising part: a merchant who
            // syncs 200 items and expects to be selling needs to know why
            // nothing is orderable yet.
            "confirmed": 0,
            "next_step": "Open your Storefront and confirm stock — imported items \
                          are substituted until a person confirms them.",
        })),
    ))
}

#[derive(Deserialize)]
struct SyncQuery {
    platform: Option<String>,
}

/// DELETE /v1/connectors/credentials/:platform — revoke a connector
async fn delete_credentials(
    State(state):    State<AppState>,
    claims: AuthClaims,
    Path(platform):  Path<String>,
) -> impl IntoResponse {
    state.svc.delete_credentials(claims.tenant_id, &platform).await?;
    Ok::<_, AppError>(StatusCode::NO_CONTENT)
}

// ── Router assembly ───────────────────────────────────────────────────────────

pub fn router(state: AppState) -> Router {
    let auth_layer = axum::middleware::from_fn_with_state(
        Arc::clone(&state.jwt),
        logisticos_auth::middleware::require_auth,
    );

    // Webhook routes — no JWT, HMAC is the security boundary
    let webhook_routes = Router::new()
        .route("/shopify/:tenant_id/webhook",     post(shopify_webhook))
        .route("/woocommerce/:tenant_id/webhook", post(woocommerce_webhook));

    // Credential management routes — JWT required
    let credential_routes = Router::new()
        .route("/credentials",           post(upsert_credentials).get(list_credentials))
        .route("/credentials/:platform", delete(delete_credentials))
        .route("/catalog/sync",          post(sync_catalog))
        .layer(auth_layer);

    Router::new()
        .nest("/v1/connectors", webhook_routes.merge(credential_routes))
        .with_state(state)
}
