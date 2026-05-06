use axum::{extract::{Query, State}, Json, http::StatusCode};
use std::sync::Arc;
use serde::Deserialize;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_auth::require_permission;
use logisticos_errors::AppError;
use crate::api::http::AppState;
use crate::infrastructure::db::partner_bonus_repo::PartnerBonus;

#[derive(Deserialize)]
pub struct BreakdownParams {
    merchant_id: Uuid,
    year:        i32,
    month:       u32,
}

pub async fn get_commission_breakdown(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<BreakdownParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_VIEW);
    let breakdown = state.commission_query
        .run(params.merchant_id, params.year, params.month).await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": breakdown })))
}

#[derive(Deserialize)]
pub struct CreateBonusBody {
    pub merchant_id:     Uuid,
    pub amount_centavos: i64,
    pub reason:          String,
    pub effective_month: chrono::NaiveDate,
}

pub async fn create_partner_bonus(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBonusBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::BILLING_MANAGE);
    if body.amount_centavos <= 0 {
        return Err(AppError::Validation("amount_centavos must be positive".into()));
    }
    let bonus = PartnerBonus {
        id:              uuid::Uuid::new_v4(),
        tenant_id:       claims.tenant_id,
        merchant_id:     body.merchant_id,
        amount_centavos: body.amount_centavos,
        currency:        "PHP".into(),
        reason:          body.reason,
        effective_month: body.effective_month,
        created_by:      claims.user_id,
        created_at:      chrono::Utc::now(),
    };
    state.partner_bonus_repo.insert(&bonus).await.map_err(AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": bonus.id }))))
}
