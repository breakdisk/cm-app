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
    extract::{Path, State},
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
        .layer(auth_layer);

    Router::new()
        .nest("/v1/connectors", webhook_routes.merge(credential_routes))
        .with_state(state)
}
