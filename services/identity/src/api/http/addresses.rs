use axum::{extract::{Path, State}, Json};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::{api::http::AppState, domain::entities::PickupAddress};

#[derive(Debug, Deserialize)]
pub struct CreatePickupAddressBody {
    pub label:       String,
    pub address:     String,
    pub city:        Option<String>,
    pub province:    Option<String>,
    pub postal_code: Option<String>,
    pub country:     Option<String>,
    pub lat:         Option<f64>,
    pub lng:         Option<f64>,
}

pub async fn list(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = logisticos_types::UserId::from_uuid(claims.user_id);
    let addrs = state.address_repo.list_by_user(&user_id).await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": addrs })))
}

pub async fn create(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreatePickupAddressBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id   = logisticos_types::UserId::from_uuid(claims.user_id);
    let tenant_id = logisticos_types::TenantId::from_uuid(claims.tenant_id);

    let addr = PickupAddress::new(
        user_id,
        tenant_id,
        body.label,
        body.address,
        body.city.unwrap_or_default(),
        body.province.unwrap_or_default(),
        body.postal_code.unwrap_or_default(),
        body.country.unwrap_or_else(|| "PH".to_string()),
        body.lat,
        body.lng,
    );
    state.address_repo.insert(&addr).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": addr })))
}

pub async fn delete(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = logisticos_types::UserId::from_uuid(claims.user_id);
    state.address_repo.delete(id, &user_id).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn set_default(
    AuthClaims(claims): AuthClaims,
    Path(id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = logisticos_types::UserId::from_uuid(claims.user_id);
    state.address_repo.set_default(id, &user_id).await.map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
