use axum::{extract::{Path, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::{CustomerId, InvoiceId, MerchantId, TenantId};
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

/// `GET /v1/customers/:customer_id/invoices` — customer app receipt list.
/// The caller must be authenticated as the customer themselves (or an admin/billing manager).
pub async fn list_customer_invoices(
    AuthClaims(claims): AuthClaims,
    Path(customer_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    // Customers may only fetch their own receipts.
    // Callers with BILLING_MANAGE (admins, ops) may fetch any customer's receipts.
    let has_manage = claims.has_permission(logisticos_auth::rbac::permissions::BILLING_MANAGE);
    if !has_manage && claims.user_id != customer_id {
        return Err(AppError::Forbidden {
            resource: "invoices for another customer".into(),
        });
    }
    let cid = CustomerId::from_uuid(customer_id);
    let invoices = state.invoice_service.list_for_customer(&cid).await?;
    Ok(Json(serde_json::json!({ "data": invoices })))
}

/// `POST /v1/invoices/:id/resend`
///
/// Re-sends the invoice to the customer (or merchant) via the `invoice.generated`
/// Kafka event.  The engagement engine picks this up and delivers the email/SMS.
///
/// Customers may only resend their own receipts (BILLING_VIEW is sufficient).
pub async fn resend_invoice(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let invoice_id = InvoiceId::from_uuid(id);
    state.invoice_service.resend(&invoice_id, claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "sent": true } })))
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
