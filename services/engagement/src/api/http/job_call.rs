//! Masked calling between the two people on a job.
//!
//! `POST /v1/engagement/jobs/:shipment_id/call` rings the caller's own phone
//! and, when they answer, dials the other party showing the platform's number.
//! Neither app is ever given the other side's line.
//!
//! Who may ask is the same question as the chat thread's, answered the same
//! way — by order-intake and driver-ops, with the caller's own token.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    api::http::{job_chat::{bearer, is_driver}, AppState},
    infrastructure::masked_calls::CallOutcome,
};

/// `POST /v1/engagement/jobs/:shipment_id/call`
pub async fn start_call(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    let token = bearer(&headers)?;
    let part = state
        .job_participants
        .resolve(shipment_id, claims.user_id, is_driver(&claims), &token)
        .await?;

    if !part.can_send {
        return Err(AppError::BusinessRule(
            "This job is finished — calls on it are closed.".to_owned(),
        ));
    }

    // Asked before the numbers are fetched: a deployment with no voice number
    // has no reason to go looking for either line.
    if !state.masked_calls.configured() {
        return Ok::<_, AppError>((
            StatusCode::OK,
            Json(serde_json::json!({ "data": { "bridged": false, "reason": "not_configured" } })),
        ));
    }

    let contacts = state.job_participants.contacts(shipment_id, part.role, &token).await?;
    let outcome = state
        .masked_calls
        .bridge(contacts.mine.as_deref(), contacts.theirs.as_deref())
        .await?;

    let body = match outcome {
        CallOutcome::Bridged { call_sid } => serde_json::json!({
            "bridged":       true,
            "call_sid":      call_sid,
            // Shown to the person who asked, so the incoming call is recognisable.
            "masked_number": state.masked_calls.masked_number(),
        }),
        CallOutcome::NotConfigured => serde_json::json!({ "bridged": false, "reason": "not_configured" }),
        CallOutcome::NoNumber => serde_json::json!({ "bridged": false, "reason": "no_number" }),
    };

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": body }))))
}
