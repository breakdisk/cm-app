use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::{InvoiceId, MerchantId, TenantId};
use crate::{api::http::AppState, application::commands::GenerateInvoiceCommand};

pub async fn list_invoices(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    // Merchant ID == tenant's primary merchant (1:1 in simple case)
    let merchant_id = MerchantId::from_uuid(claims.tenant_id);
    let invoices = state.invoice_service.list(&merchant_id).await?;
    Ok(Json(serde_json::json!({ "data": invoices })))
}

pub async fn get_invoice(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let invoice_id = InvoiceId::from_uuid(id);
    let invoice = state.invoice_service.get(&invoice_id).await?;
    Ok(Json(serde_json::json!({ "data": invoice })))
}

pub async fn generate_invoice(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<GenerateInvoiceCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let invoice = state.invoice_service.generate(&tenant_id, cmd).await?;
    Ok(Json(serde_json::json!({
        "data": {
            "invoice_id": invoice.id,
            "total_cents": invoice.total_due().amount,
            "due_at": invoice.due_at.to_rfc3339()
        }
    })))
}
