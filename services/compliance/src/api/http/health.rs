use axum::{extract::State, Json};
use std::sync::Arc;
use crate::api::http::AppState;

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "compliance" }))
}

pub async fn ready(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let ok = sqlx::query("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    if ok {
        Ok(Json(serde_json::json!({ "status": "ready" })))
    } else {
        Err(axum::http::StatusCode::SERVICE_UNAVAILABLE)
    }
}
