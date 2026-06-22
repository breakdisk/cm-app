use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct RunSettlementBody {
    /// Optional ISO date (YYYY-MM-DD) for the period end.
    /// Defaults to yesterday UTC when absent.
    pub period_end: Option<chrono::NaiveDate>,
}

#[derive(Serialize)]
pub struct RunSettlementResponse {
    pub settlements: Vec<serde_json::Value>,
    pub carrier_count: usize,
    pub total_disbursed_cents: i64,
}

/// POST /v1/admin/carrier-settlements/run
/// Trigger a carrier settlement run for the authenticated tenant.
pub async fn run_settlement(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RunSettlementBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);

    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let outcomes = state.carrier_settlement_service
        .run(&tenant_id, body.period_end, claims.user_id)
        .await?;

    let total_disbursed: i64 = outcomes.iter().map(|o| o.total_cents).sum();
    let count = outcomes.len();

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "carrier_count": count,
            "total_disbursed_cents": total_disbursed,
            "settlements": outcomes,
        })),
    ))
}

/// GET /v1/admin/carrier-settlements
/// List all settlement runs for the tenant (admin view).
pub async fn list_settlements(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);

    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let runs = state.carrier_settlement_service
        .list(&tenant_id)
        .await?;

    Ok(Json(serde_json::json!({ "data": runs })))
}

/// GET /v1/partner/carrier-settlements
/// Partner view of their tenant's carrier settlement history.
pub async fn list_my_settlements(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);

    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let runs = state.carrier_settlement_service
        .list(&tenant_id)
        .await?;

    Ok(Json(serde_json::json!({ "data": runs })))
}
