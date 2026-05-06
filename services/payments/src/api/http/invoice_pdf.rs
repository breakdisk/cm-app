use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use logisticos_types::InvoiceId;
use tera::Context;
use crate::api::http::AppState;

/// GET /v1/invoices/:id/pdf
pub async fn download_invoice_pdf(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<uuid::Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);

    let invoice_id = InvoiceId::from_uuid(id);
    let invoice = state.invoice_service.get(&invoice_id).await?;

    let mut ctx = Context::new();
    ctx.insert("tenant_name",        &claims.tenant_id.to_string());
    ctx.insert("invoice_number",     &invoice.invoice_number.to_string());
    ctx.insert("merchant_id",        &invoice.merchant_id.inner().to_string());
    let status_str = match invoice.status {
        crate::domain::entities::InvoiceStatus::Draft     => "draft",
        crate::domain::entities::InvoiceStatus::Issued    => "issued",
        crate::domain::entities::InvoiceStatus::Paid      => "paid",
        crate::domain::entities::InvoiceStatus::Overdue   => "overdue",
        crate::domain::entities::InvoiceStatus::Disputed  => "disputed",
        crate::domain::entities::InvoiceStatus::Cancelled => "cancelled",
    };
    ctx.insert("status", &status_str);
    ctx.insert("issued_at",          &invoice.issued_at.format("%Y-%m-%d").to_string());
    ctx.insert("due_at",             &invoice.due_at.format("%Y-%m-%d").to_string());
    ctx.insert("period_start",       &invoice.billing_period.start.to_string());
    ctx.insert("period_end",         &invoice.billing_period.end.to_string());
    ctx.insert("payment_terms_days", &15i32);

    let line_items: Vec<serde_json::Value> = invoice.line_items.iter().map(|i| {
        serde_json::json!({
            "awb":            i.awb.as_ref().map(|a| a.as_str().to_string()),
            "description":    i.description,
            "quantity":       i.quantity,
            "unit_price_php": format!("{:.2}", i.unit_price.amount as f64 / 100.0),
            "net_php":        format!("{:.2}", i.net().amount as f64 / 100.0),
        })
    }).collect();
    ctx.insert("line_items", &line_items);

    let adjustments: Vec<serde_json::Value> = invoice.adjustments.iter().map(|a| {
        serde_json::json!({
            "awb":        a.awb.as_ref().map(|x| x.as_str().to_string()),
            "reason":     a.reason,
            "amount_php": format!("{:.2}", a.amount.amount as f64 / 100.0),
        })
    }).collect();
    ctx.insert("adjustments", &adjustments);
    ctx.insert("subtotal_php", &format!("{:.2}", invoice.subtotal().amount as f64 / 100.0));
    ctx.insert("vat_php",      &format!("{:.2}", invoice.vat_amount().amount as f64 / 100.0));
    ctx.insert("total_php",    &format!("{:.2}", invoice.total_due().amount as f64 / 100.0));

    let renderer = state.pdf_renderer.as_ref()
        .ok_or_else(|| AppError::ServiceUnavailable("PDF renderer not initialised — Chrome unavailable".into()))?;

    let pdf_bytes = renderer.render_invoice(&ctx).await
        .map_err(AppError::Internal)?;

    let safe_name: String = invoice.invoice_number
        .to_string()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    let filename = format!("{safe_name}.pdf");
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))
        .body(Body::from(pdf_bytes))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{e}")))?;

    Ok(response)
}
