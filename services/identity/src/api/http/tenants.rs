use axum::{extract::State, Json};
use std::sync::Arc;
use crate::{api::http::AppState, application::commands::CreateTenantCommand};
use logisticos_errors::AppError;

pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<CreateTenantCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant = state.tenant_service.create_tenant(cmd).await?;
    Ok(Json(serde_json::json!({ "data": { "tenant_id": tenant.id, "slug": tenant.slug } })))
}
