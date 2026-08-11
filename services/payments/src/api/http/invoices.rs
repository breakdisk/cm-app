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

/// `GET /v1/invoices/tenant` — tenant-wide merchant invoice list for the
/// admin/ops console. Caller must have BILLING_MANAGE (ops-tier permission).
/// Excludes customer-facing PaymentReceipt invoices.
/// Served via the api-gateway's `/v1/invoices` → payments route rule.
pub async fn list_tenant_invoices(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let tenant_id = TenantId::from_uuid(claims.tenant_id);
    let invoices = state.invoice_service.list_for_tenant(&tenant_id).await?;
    Ok(Json(serde_json::json!({ "data": invoices })))
}


/// How much of the tenant's billing history a caller may read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BillingReadScope {
    /// Staff: any customer in the tenant.
    Tenant,
    /// End customer: their own invoices only.
    OwnOnly,
}

/// Tenant-wide read wins when a caller holds both.
pub(crate) fn scope_for(has_tenant_read: bool, has_own_read: bool) -> Option<BillingReadScope> {
    match (has_tenant_read, has_own_read) {
        (true, _) => Some(BillingReadScope::Tenant),
        (false, true) => Some(BillingReadScope::OwnOnly),
        (false, false) => None,
    }
}

fn billing_read_scope(claims: &logisticos_auth::claims::Claims) -> Result<BillingReadScope, AppError> {
    scope_for(
        claims.has_permission(logisticos_auth::rbac::permissions::BILLING_VIEW),
        claims.has_permission(logisticos_auth::rbac::permissions::BILLING_READ_OWN),
    )
    .ok_or_else(|| AppError::Forbidden { resource: "invoices".into() })
}

pub async fn get_invoice(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let scope = billing_read_scope(&claims)?;
    let invoice_id = InvoiceId::from_uuid(id);
    let invoice = state.invoice_service.get(&invoice_id).await?;

    // Tenant isolation. The lookup is `SELECT ... WHERE id = $1` with no
    // tenant predicate, and row-level security is not enabled on
    // payments.invoices, so without this check an id from another tenant
    // returns that tenant's invoice. NotFound rather than Forbidden, so the
    // response does not confirm the id exists.
    if invoice.tenant_id.inner() != claims.tenant_id {
        return Err(AppError::NotFound { resource: "Invoice", id: id.to_string() });
    }

    // A customer reading by id may only read their own.
    if scope == BillingReadScope::OwnOnly
        && invoice.customer_id.as_ref().map(|c| c.inner()) != Some(claims.user_id)
    {
        return Err(AppError::NotFound { resource: "Invoice", id: id.to_string() });
    }

    Ok(Json(serde_json::json!({ "data": invoice })))
}

/// `GET /v1/customers/:customer_id/invoices` — customer app receipt list.
/// The caller must be authenticated as the customer themselves (or an admin/billing manager).
pub async fn list_customer_invoices(
    AuthClaims(claims): AuthClaims,
    Path(customer_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // BILLING_VIEW is tenant-wide staff access; BILLING_READ_OWN is the
    // customer's own receipts. The customer role holds only the latter, so
    // requiring BILLING_VIEW here made the app's Invoices screen 403 even
    // once the gateway routed it.
    billing_read_scope(&claims)?;
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

#[cfg(test)]
mod billing_scope_tests {
    use super::*;

    #[test]
    fn no_billing_permission_is_refused() {
        assert_eq!(scope_for(false, false), None);
    }

    #[test]
    fn staff_read_is_tenant_wide_and_wins_when_both_are_held() {
        assert_eq!(scope_for(true, false), Some(BillingReadScope::Tenant));
        assert_eq!(scope_for(true, true), Some(BillingReadScope::Tenant));
    }

    /// The customer role holds only this one. Before it existed, the app's
    /// Invoices and Receipt screens got 403 from a handler that was otherwise
    /// written correctly for them -- it self-scopes by claims.user_id already.
    #[test]
    fn a_customer_gets_own_only() {
        assert_eq!(scope_for(false, true), Some(BillingReadScope::OwnOnly));
    }
}
