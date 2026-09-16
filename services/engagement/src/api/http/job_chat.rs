//! Job chat — the thread between the customer and the driver on one shipment.
//!
//! Mounted under `/v1/engagement/jobs/:shipment_id/messages`, which the gateway
//! already routes here. There is no permission gate: taking part in a job is
//! the gate, and it is answered by order-intake and driver-ops rather than by
//! anything stored in this service.

use axum::{
    extract::{Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;

use crate::{
    api::http::AppState,
    domain::entities::job_message::clean_body,
    infrastructure::db::job_chat::JobChatDb,
};

/// The caller's own token, forwarded to whichever service owns the answer.
fn bearer(headers: &HeaderMap) -> Result<String, AppError> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .ok_or(AppError::Forbidden { resource: "job chat".to_owned() })
}

/// driver-ops issues driver tokens with this role; it decides which service is
/// asked first, never whether access is granted.
fn is_driver(claims: &AuthClaims) -> bool {
    claims.roles.iter().any(|role| role == "driver")
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    /// Only messages after this instant — what an open chat polls with.
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SendRequest {
    pub body: String,
    /// The sender's own id for this message, so a retry cannot post twice.
    pub client_message_id: Option<Uuid>,
}

/// `GET /v1/engagement/jobs/:shipment_id/messages`
pub async fn list_messages(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(shipment_id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let token = bearer(&headers)?;
    let part = state
        .job_participants
        .resolve(shipment_id, claims.user_id, is_driver(&claims), &token)
        .await?;

    let db = JobChatDb::new(state.db.clone());
    let messages = db
        .list(claims.tenant_id, shipment_id, q.since, q.limit.unwrap_or(200).clamp(1, 500))
        .await
        .map_err(AppError::Internal)?;
    let unread = db
        .unread(claims.tenant_id, shipment_id, claims.user_id)
        .await
        .map_err(AppError::Internal)?;

    Ok::<_, AppError>((
        StatusCode::OK,
        Json(serde_json::json!({
            "data": {
                "role":            part.role,
                "can_send":        part.can_send,
                "shipment_status": part.shipment_status,
                "unread":          unread,
                "messages":        messages,
            }
        })),
    ))
}

/// `POST /v1/engagement/jobs/:shipment_id/messages`
pub async fn send_message(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(shipment_id): Path<Uuid>,
    Json(req): Json<SendRequest>,
) -> impl IntoResponse {
    let token = bearer(&headers)?;
    let part = state
        .job_participants
        .resolve(shipment_id, claims.user_id, is_driver(&claims), &token)
        .await?;
    if !part.can_send {
        return Err(AppError::BusinessRule(
            "This job is finished — its messages can still be read, but not added to.".to_owned(),
        ));
    }

    let body = clean_body(&req.body).map_err(|e| AppError::Validation(e.message()))?;
    let db = JobChatDb::new(state.db.clone());
    let message = db
        .insert(claims.tenant_id, shipment_id, claims.user_id, part.role, &body, req.client_message_id)
        .await
        .map_err(AppError::Internal)?;

    // Sending is also reading everything before it.
    let _ = db.mark_read(claims.tenant_id, shipment_id, claims.user_id, message.created_at).await;

    Ok::<_, AppError>((StatusCode::CREATED, Json(serde_json::json!({ "data": message }))))
}

/// `POST /v1/engagement/jobs/:shipment_id/messages/read`
pub async fn mark_read(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    let token = bearer(&headers)?;
    state
        .job_participants
        .resolve(shipment_id, claims.user_id, is_driver(&claims), &token)
        .await?;

    JobChatDb::new(state.db.clone())
        .mark_read(claims.tenant_id, shipment_id, claims.user_id, Utc::now())
        .await
        .map_err(AppError::Internal)?;

    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}

/// `GET /v1/engagement/jobs/:shipment_id/messages/unread` — the badge, which
/// must not mark anything read.
pub async fn unread_badge(
    State(state): State<AppState>,
    claims: AuthClaims,
    headers: HeaderMap,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    let token = bearer(&headers)?;
    state
        .job_participants
        .resolve(shipment_id, claims.user_id, is_driver(&claims), &token)
        .await?;

    let unread = JobChatDb::new(state.db.clone())
        .unread(claims.tenant_id, shipment_id, claims.user_id)
        .await
        .map_err(AppError::Internal)?;

    Ok::<_, AppError>((StatusCode::OK, Json(serde_json::json!({ "data": { "unread": unread } }))))
}
