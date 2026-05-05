use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
use crate::api::http::AppState;

/// GET /v1/admin/withdrawal-requests
pub async fn list_withdrawal_requests(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: define BILLING_ADMIN permission in rbac.rs
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let requests = state.withdrawal_service.list_pending(claims.tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": requests })))
}

/// POST /v1/admin/withdrawal-requests/:id/approve
pub async fn approve_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: define BILLING_ADMIN permission in rbac.rs
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service.approve(id, claims.user_id, &tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}

/// POST /v1/admin/withdrawal-requests/:id/disburse
pub async fn disburse_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: define BILLING_ADMIN permission in rbac.rs
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service.disburse(id, claims.user_id, &tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}

#[derive(Deserialize)]
pub struct RejectBody { pub reason: String }

/// POST /v1/admin/withdrawal-requests/:id/reject
pub async fn reject_withdrawal(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RejectBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    // TODO: define BILLING_ADMIN permission in rbac.rs
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let req = state.withdrawal_service.reject(id, claims.user_id, body.reason, &tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": req })))
}
