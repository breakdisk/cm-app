//! Internal (mTLS-gated) routes for COD remittance batch lifecycle.
//! Called by an ops tool or a scheduled job — no JWT is required because the
//! API-gateway/Istio mesh enforces caller identity for `/v1/internal/*`.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::AppError;
use crate::{
    api::http::AppState,
    application::commands::{ConfirmCodBatchCommand, CreateCodBatchCommand},
    domain::entities::{CodBatchStatus, CodRemittanceBatch},
};

/// `POST /v1/internal/cod/batches`
/// Body: `{ tenant_id, merchant_id, cutoff_date }`
/// Response: 201 Created + batch JSON (status=created). 409 if nothing to batch.
pub async fn create_batch(
    State(state): State<Arc<AppState>>,
    Json(cmd):    Json<CreateCodBatchCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let batch = state.cod_remittance_service.create_batch(cmd).await?;
    Ok((StatusCode::CREATED, Json(render_batch(&batch))))
}

/// `POST /v1/internal/cod/batches/:id/confirm`
/// Marks the batch paid, credits the merchant wallet net-of-fee,
/// flips member COD rows to `remitted`, emits `cod.remitted`.
/// Idempotent — confirming an already-paid batch returns 200.
pub async fn confirm_batch(
    State(state): State<Arc<AppState>>,
    Path(batch_id): Path<Uuid>,
    Json(mut cmd): Json<ConfirmCodBatchCommand>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Path is authoritative for batch_id — avoid body/path mismatch.
    cmd.batch_id = batch_id;
    let batch = state.cod_remittance_service.confirm_batch(cmd).await?;
    Ok((StatusCode::OK, Json(render_batch(&batch))))
}

fn render_batch(b: &CodRemittanceBatch) -> serde_json::Value {
    let status_str = match b.status {
        CodBatchStatus::Created => "created",
        CodBatchStatus::Paid    => "paid",
        CodBatchStatus::Failed  => "failed",
    };
    serde_json::json!({
        "id":                 b.id,
        "tenant_id":          b.tenant_id.inner(),
        "merchant_id":        b.merchant_id.inner(),
        "cutoff_date":        b.cutoff_date.to_string(),
        "currency":           format!("{:?}", b.currency),
        "cod_count":          b.cod_count,
        "gross_cents":        b.gross_cents,
        "platform_fee_cents": b.platform_fee_cents,
        "net_cents":          b.net_cents,
        "status":             status_str,
        "failure_reason":     b.failure_reason,
        "created_at":         b.created_at.to_rfc3339(),
        "paid_at":            b.paid_at.map(|t| t.to_rfc3339()),
    })
}
