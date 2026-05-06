use axum::{extract::{Path, State}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use chrono::Utc;
use crate::{
    api::http::AppState,
    domain::entities::MerchantBillingAccount,
};

pub async fn get_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    let acct = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "billing_account", id: merchant_id.to_string() })?;
    Ok(Json(account_to_json(&acct)))
}

pub async fn upsert_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertBillingAccountBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);

    let existing = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?;

    let is_new = existing.is_none();
    let mut acct = if let Some(existing_acct) = existing {
        existing_acct
    } else {
        let email = body.billing_email.clone()
            .filter(|e| !e.trim().is_empty())
            .ok_or_else(|| AppError::Validation("billing_email is required".into()))?;
        MerchantBillingAccount::new(claims.tenant_id, merchant_id, email)
    };

    apply_body(&mut acct, body);
    acct.updated_at = Utc::now();
    state.merchant_billing_account_repo.upsert(&acct).await.map_err(AppError::Internal)?;

    let status = if is_new { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(account_to_json(&acct))))
}

pub async fn patch_billing_account(
    AuthClaims(claims): AuthClaims,
    Path(merchant_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<UpsertBillingAccountBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);

    let mut acct = state.merchant_billing_account_repo
        .find_by_merchant(merchant_id).await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "billing_account", id: merchant_id.to_string() })?;

    apply_body(&mut acct, body);
    acct.updated_at = Utc::now();
    state.merchant_billing_account_repo.upsert(&acct).await.map_err(AppError::Internal)?;

    Ok(Json(account_to_json(&acct)))
}

#[derive(Deserialize)]
pub struct UpsertBillingAccountBody {
    pub base_rate_override_centavos: Option<i64>,
    pub payment_terms_days:          Option<i16>,
    pub credit_limit_centavos:       Option<i64>,
    pub tin:                         Option<String>,
    pub vat_registered:              Option<bool>,
    pub billing_email:               Option<String>,
    pub invoice_channel:             Option<String>,
    pub bank_name:                   Option<String>,
    pub bank_account_number:         Option<String>,
    pub bank_account_name:           Option<String>,
}

fn apply_body(acct: &mut MerchantBillingAccount, body: UpsertBillingAccountBody) {
    if let Some(v) = body.base_rate_override_centavos { acct.base_rate_override_centavos = Some(v); }
    if let Some(v) = body.payment_terms_days          { acct.payment_terms_days = v; }
    if let Some(v) = body.credit_limit_centavos       { acct.credit_limit_centavos = v; }
    if let Some(v) = body.tin                         { acct.tin = Some(v); }
    if let Some(v) = body.vat_registered              { acct.vat_registered = v; }
    if let Some(v) = body.billing_email               { acct.billing_email = v; }
    if let Some(v) = body.invoice_channel             { acct.invoice_channel = v; }
    if let Some(v) = body.bank_name                   { acct.bank_name = Some(v); }
    if let Some(v) = body.bank_account_number         { acct.bank_account_number = Some(v); }
    if let Some(v) = body.bank_account_name           { acct.bank_account_name = Some(v); }
}

fn account_to_json(a: &MerchantBillingAccount) -> serde_json::Value {
    serde_json::json!({
        "id":                          a.id,
        "merchant_id":                 a.merchant_id,
        "base_rate_override_centavos": a.base_rate_override_centavos,
        "payment_terms_days":          a.payment_terms_days,
        "credit_limit_centavos":       a.credit_limit_centavos,
        "tin":                         a.tin,
        "vat_registered":              a.vat_registered,
        "billing_email":               a.billing_email,
        "invoice_channel":             a.invoice_channel,
        "bank_name":                   a.bank_name,
        "bank_account_number":         a.masked_bank_account(),
        "bank_account_name":           a.bank_account_name,
        "created_at":                  a.created_at.to_rfc3339(),
        "updated_at":                  a.updated_at.to_rfc3339(),
    })
}
