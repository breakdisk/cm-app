use axum::{extract::{State, Query}, Json};
use std::sync::Arc;
use serde::Deserialize;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
use crate::{api::http::AppState, application::commands::{ReconcileCodCommand, RequestWithdrawalCommand}};

pub async fn get_wallet(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let summary = state.wallet_service.summary(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": summary })))
}

#[derive(Deserialize)]
pub struct TransactionQuery { limit: Option<u32> }

pub async fn list_transactions(
    AuthClaims(claims): AuthClaims,
    Query(q): Query<TransactionQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let limit = q.limit.unwrap_or(50).min(200);
    let txns = state.wallet_service.list_transactions(&tenant_id, limit).await?;
    Ok(Json(serde_json::json!({ "data": txns })))
}

pub async fn reconcile_cod(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<ReconcileCodCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    // This endpoint is called by driver-ops/pod service via internal API key auth
    // In production, this would be protected by mTLS + service account, not user JWT
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.cod_service.reconcile_cod(&tenant_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

pub async fn request_withdrawal(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RequestWithdrawalCommand>,
) -> Result<axum::http::StatusCode, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    state.wallet_service.request_withdrawal(&tenant_id, cmd).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
