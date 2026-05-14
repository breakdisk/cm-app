use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use sha2::{Digest, Sha256};

use crate::AppState;

// ── Carrier API-key authentication ────────────────────────────────────────────

/// Extractor inserted by `require_carrier_api_key` — carries the resolved
/// carrier_id so downstream handlers don't need a second DB lookup.
#[derive(Debug, Clone)]
pub struct CarrierApiKeyPrincipal {
    pub carrier_id: uuid::Uuid,
}

/// Axum middleware that validates the `X-Carrier-Api-Key` header against the
/// SHA-256 hash stored on the carrier record.
///
/// Used for inbound webhook callbacks from 3PL systems that authenticate with
/// the key generated via `POST /v1/carriers/:id/api-key`.
///
/// Mount this middleware on routes that should be accessible to carrier systems
/// rather than the JWT-authenticated portal flows:
///
/// ```rust
/// let app = Router::new()
///     .route("/v1/webhooks/carrier/:id/tracking", post(tracking_webhook))
///     .layer(axum::middleware::from_fn_with_state(
///         state.clone(),
///         require_carrier_api_key,
///     ));
/// ```
pub async fn require_carrier_api_key(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    // Extract X-Carrier-Api-Key header
    let raw_key = match req.headers().get("X-Carrier-Api-Key") {
        Some(v) => match v.to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => return error_response(StatusCode::BAD_REQUEST, "Invalid API key header encoding"),
        },
        None => return error_response(StatusCode::UNAUTHORIZED, "Missing X-Carrier-Api-Key header"),
    };

    // Extract carrier_id from path (expects /…/:carrier_id/…)
    let carrier_id: uuid::Uuid = match req
        .extensions()
        .get::<axum::extract::MatchedPath>()
        .and_then(|_| {
            // Pull carrier_id from the URI path segments.
            // We look for a UUID-shaped segment after "/carriers/".
            req.uri().path().split('/').find_map(|seg| seg.parse::<uuid::Uuid>().ok())
        }) {
        Some(id) => id,
        None => return error_response(StatusCode::BAD_REQUEST, "Could not resolve carrier_id from path"),
    };

    // Load carrier and verify key hash
    match state.carrier_svc.get(carrier_id).await {
        Err(_) => return error_response(StatusCode::UNAUTHORIZED, "Invalid carrier"),
        Ok(carrier) => {
            let expected_hash = match &carrier.api_key_hash {
                Some(h) => h.clone(),
                None => return error_response(StatusCode::UNAUTHORIZED, "No API key configured for this carrier"),
            };
            let presented_hash = {
                let mut hasher = Sha256::new();
                hasher.update(raw_key.as_bytes());
                format!("{:x}", hasher.finalize())
            };
            if presented_hash != expected_hash {
                return error_response(StatusCode::UNAUTHORIZED, "Invalid API key");
            }
            req.extensions_mut().insert(CarrierApiKeyPrincipal { carrier_id });
        }
    }

    next.run(req).await
}

fn error_response(status: StatusCode, message: &'static str) -> Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

// ── Request ID propagation ────────────────────────────────────────────────────

/// Propagates or generates a `X-Request-Id` header. Downstream handlers can
/// read it from extensions for structured log correlation.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

pub async fn propagate_request_id(mut req: Request<Body>, next: Next) -> Response {
    let id = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    req.extensions_mut().insert(RequestId(id.clone()));

    let mut response = next.run(req).await;
    if let Ok(val) = id.parse() {
        response.headers_mut().insert("X-Request-Id", val);
    }
    response
}
