use axum::{extract::{State, Query}, Json};
use serde::Deserialize;
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterPushTokenRequest {
    pub token: String,
    pub platform: String, // "ios" | "android" | "web"
    pub app: String,      // "customer" | "driver"
    pub device_id: Option<String>,
}

pub async fn register_push_token(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterPushTokenRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.token.trim().is_empty() {
        return Err(AppError::Validation("token is required".into()));
    }
    if !matches!(req.platform.as_str(), "ios" | "android" | "web") {
        return Err(AppError::Validation("platform must be ios|android|web".into()));
    }
    if !matches!(req.app.as_str(), "customer" | "driver") {
        return Err(AppError::Validation("app must be customer|driver".into()));
    }

    state.push_token_repo
        .upsert(
            claims.tenant_id,
            claims.user_id,
            req.token.trim(),
            &req.platform,
            &req.app,
            req.device_id.as_deref(),
        )
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({ "data": { "registered": true } })))
}

#[derive(Debug, Deserialize)]
pub struct DeletePushTokenRequest {
    pub token: String,
}

pub async fn delete_push_token(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeletePushTokenRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    state.push_token_repo
        .delete(claims.tenant_id, req.token.trim())
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": { "deleted": true } })))
}

#[derive(Debug, Deserialize)]
pub struct ListPushTokensQuery {
    pub app: String,
    /// By user id — the app's own pushes.
    #[serde(default)]
    pub user_id: Option<uuid::Uuid>,
    /// Or by the email/phone of a login in this tenant — campaign pushes,
    /// which address a CDP profile rather than a user.
    #[serde(default)]
    pub tenant_id: Option<uuid::Uuid>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
}

/// Internal endpoint — called by engagement service to fetch push tokens for
/// a user before dispatching notifications. Not exposed through the API gateway;
/// protected by Docker network isolation (same overlay network only).
pub async fn list_push_tokens_internal(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListPushTokensQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !matches!(q.app.as_str(), "customer" | "driver") {
        return Err(AppError::Validation("app must be customer|driver".into()));
    }
    let email = q.email.as_deref().map(str::trim).filter(|e| e.contains('@'));
    let phones = q.phone.as_deref().map(crate::api::http::users::phone_candidates).unwrap_or_default();
    let tokens = match (q.user_id, q.tenant_id) {
        (Some(user_id), _) => state.push_token_repo.list_by_user(user_id, &q.app).await,
        (None, Some(tenant_id)) if email.is_some() || !phones.is_empty() => {
            state.push_token_repo.list_by_contact(tenant_id, &q.app, email, &phones).await
        }
        _ => return Err(AppError::Validation("user_id, or tenant_id with an email or phone".into())),
    }
    .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": { "tokens": tokens } })))
}
