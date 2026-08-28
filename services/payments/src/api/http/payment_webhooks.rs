//! POST /v1/payments/webhooks/network-international — public, no JWT (the
//! gateway cannot hold a LogisticOS session). Authenticated instead by NI's
//! own webhook signature, verified inside `PaymentIntentService::handle_webhook`.
//! State changes only happen after that verification succeeds.

use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}};
use std::sync::Arc;

use crate::api::http::AppState;
use crate::application::services::payment_intent_service::WebhookError;

pub async fn network_international_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let Some(payment_intent_service) = state.payment_intent_service.as_ref() else {
        // Unconfigured deployment: there is no gateway this webhook could
        // legitimately have come from. 503 (not 400/404) tells NI this is a
        // transient deployment state, matching the internal intents route's
        // same-cause 503 — an operator wiring up NI would see both surfaces
        // recover together once the env vars are set.
        tracing::warn!(
            "network_international_webhook received but Network International is not \
             configured — returning 503"
        );
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    match payment_intent_service.handle_webhook(&headers, &body).await {
        Ok(()) => StatusCode::OK,
        // Rejected = permanent (bad signature, unknown intent) — 4xx tells NI
        // to stop retrying.
        Err(e @ WebhookError::Rejected(_)) => {
            tracing::warn!(error = ?e, "network_international_webhook rejected — will not be retried");
            StatusCode::BAD_REQUEST
        }
        // Internal = transient (DB save / Kafka publish failed) AFTER the
        // signature already verified — NI genuinely captured money, so a
        // 5xx tells it to retry rather than risk the capture being silently
        // lost (the expiry sweep only ever revisits created/pending intents,
        // never one stuck mid-capture).
        Err(e @ WebhookError::Internal(_)) => {
            tracing::error!(error = ?e, "network_international_webhook internal failure — NI should retry");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
