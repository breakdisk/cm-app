use axum::{extract::{Path, State, Query}, Json};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
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
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
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
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    // NOTE: cmd.bank_account_id is not persisted — WithdrawalRequest entity does not yet
    // carry a bank_account_id field. Track in: future WithdrawalRequest schema iteration.
    let req = state.withdrawal_service
        .request(&tenant_id, cmd.amount_cents, claims.user_id).await?;
    let summary = state.wallet_service.summary(&tenant_id).await?;
    Ok(Json(serde_json::json!({
        "withdrawal_request_id": req.id,
        "status":                "pending",
        "reserved_centavos":     summary.reserved_centavos,
        "available_centavos":    summary.available_centavos,
    })))
}

/// Internal (no JWT): GET /v1/internal/cod/balance/:merchant_id
///
/// Returns aggregated COD balance for a merchant. Called by the AI layer's
/// `get_cod_balance` MCP tool within the service mesh (Istio mTLS gates identity).
pub async fn get_cod_balance_internal(
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    axum::extract::RawQuery(raw_query): axum::extract::RawQuery,
) -> Result<Json<serde_json::Value>, AppError> {
    // tenant_id is required; passed as a query param by the AI tool.
    let tenant_id_str = raw_query
        .as_deref()
        .and_then(|q| q.split('&').find(|p| p.starts_with("tenant_id=")))
        .and_then(|p| p.strip_prefix("tenant_id="))
        .unwrap_or("");

    let tenant_id = tenant_id_str
        .parse::<Uuid>()
        .map(TenantId::from_uuid)
        .map_err(|_| AppError::Validation("tenant_id query param is required and must be a UUID".into()))?;

    let balance = state.cod_service.cod_balance(&tenant_id, merchant_id).await?;

    Ok(Json(serde_json::json!({
        "merchant_id":     merchant_id,
        "pending_cents":   balance.pending_cents,
        "remitted_cents":  balance.remitted_cents,
        "total_cents":     balance.total_cents,
        "currency":        "PHP",
    })))
}
