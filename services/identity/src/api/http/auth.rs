use axum::{extract::State, Json};
use std::sync::Arc;
use crate::{
    api::http::AppState,
    application::commands::{LoginCommand, RefreshTokenCommand},
};
use logisticos_errors::AppError;

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<LoginCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.auth_service.login(cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<RefreshTokenCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = state.auth_service.refresh(cmd).await?;
    Ok(Json(serde_json::json!({ "data": result })))
}
