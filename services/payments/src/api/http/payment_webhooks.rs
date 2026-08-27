//! POST /v1/payments/webhooks/network-international — public, no JWT (the
//! gateway cannot hold a LogisticOS session). Authenticated instead by NI's
//! own webhook signature, verified inside `PaymentIntentService::handle_webhook`.
//! State changes only happen after that verification succeeds.

use axum::{body::Bytes, extract::State, http::{HeaderMap, StatusCode}};
use std::sync::Arc;

use crate::api::http::AppState;

pub async fn network_international_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    match state.payment_intent_service.handle_webhook(&headers, &body).await {
        Ok(()) => StatusCode::OK,
        Err(e) => {
            // Signature failures and processing errors both return 4xx so
            // NI's own retry policy redelivers — never silently swallow a webhook.
            tracing::warn!(error = %e, "network_international_webhook rejected or failed");
            StatusCode::BAD_REQUEST
        }
    }
}
