use axum::{extract::State, Json, http::StatusCode};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::{commands::RunBillingCommand, services::BillingRunOutcome},
};

/// `POST /v1/internal/billing/run`
/// Internal (mTLS-gated) endpoint kicked by the billing cron or ops tooling.
/// Idempotent on (tenant, merchant, period).
pub async fn run_billing(
    State(state): State<Arc<AppState>>,
    Json(cmd):    Json<RunBillingCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let (run, outcome) = state.billing_service.run_monthly(cmd).await?;

    let status = match outcome {
        BillingRunOutcome::Issued         => StatusCode::CREATED,
        BillingRunOutcome::AlreadyExisted => StatusCode::OK,
        BillingRunOutcome::NoShipments    => StatusCode::OK,
    };
    let outcome_str = match outcome {
        BillingRunOutcome::Issued         => "issued",
        BillingRunOutcome::AlreadyExisted => "already_existed",
        BillingRunOutcome::NoShipments    => "no_shipments",
    };

    Ok((
        status,
        Json(serde_json::json!({
            "outcome":        outcome_str,
            "run_id":         run.id,
            "invoice_id":     run.invoice_id.map(|i| i.inner()),
            "period_start":   run.period_start.to_string(),
            "period_end":     run.period_end.to_string(),
            "shipment_count": run.shipment_count,
            "total_cents":    run.total_cents,
        })),
    ))
}

/// POST /v1/admin/billing/run
pub async fn run_billing_admin(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<crate::application::commands::AdminRunBillingCommand>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), AppError> {
    // BILLING_MANAGE is the admin-level billing permission (covers billing:reconcile).
    // BILLING_ADMIN is not yet defined in rbac.rs; update this when it is.
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);
    let (run, outcome) = state.billing_service.run_for_merchant(&tenant_id, cmd).await?;

    use crate::application::services::BillingRunOutcome;
    let outcome_str = match outcome {
        BillingRunOutcome::Issued         => "issued",
        BillingRunOutcome::AlreadyExisted => "already_existed",
        BillingRunOutcome::NoShipments    => "no_shipments",
    };
    let status = match outcome {
        BillingRunOutcome::Issued         => axum::http::StatusCode::CREATED,
        _                                 => axum::http::StatusCode::OK,
    };
    Ok((status, Json(serde_json::json!({
        "outcome":        outcome_str,
        "run_id":         run.id,
        "invoice_id":     run.invoice_id.map(|i| i.inner()),
        "period_start":   run.period_start.to_string(),
        "period_end":     run.period_end.to_string(),
        "shipment_count": run.shipment_count,
        "total_cents":    run.total_cents,
    }))))
}
