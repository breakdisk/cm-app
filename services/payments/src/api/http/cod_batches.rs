//! Internal (mTLS-gated) routes for COD remittance batch lifecycle.
//! Called by an ops tool or a scheduled job — no JWT is required because the
//! API-gateway/Istio mesh enforces caller identity for `/v1/internal/*`.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;

use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::TenantId;
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

#[derive(Deserialize)]
pub struct ListRemittancesQuery {
    /// Required: the merchant whose batches to fetch.
    merchant_id: Uuid,
    limit: Option<u32>,
}

/// Protected (JWT): GET /v1/cod/remittances?merchant_id=<uuid>[&limit=50]
///
/// Lists COD remittance batches for a merchant within the authenticated tenant.
pub async fn list_remittances(
    AuthClaims(claims): AuthClaims,
    Query(q): Query<ListRemittancesQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let limit     = q.limit.unwrap_or(50).min(200);

    let batches = state.cod_remittance_service
        .list_batches_for_merchant(&tenant_id, q.merchant_id, limit)
        .await?;

    let data: Vec<_> = batches.iter().map(render_batch).collect();
    Ok(Json(serde_json::json!({ "data": data, "total": data.len() })))
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
