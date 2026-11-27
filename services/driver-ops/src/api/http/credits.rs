//! Internal: credit a driver with pay earned outside a delivered task.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use logisticos_errors::AppError;

use crate::api::http::AppState;
use crate::infrastructure::db::credit_repo::{Credited, EarningCredit};

/// `POST /v1/internal/earnings/credits` — mesh-internal (the gateway refuses
/// `/internal/`). 201 when credited, 200 when it already was: a caller may
/// retry freely.
pub async fn internal_credit(
    State(state): State<Arc<AppState>>,
    Json(c): Json<EarningCredit>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    if let Some(p) = c.problem() {
        return Err(AppError::Validation(p));
    }
    match state.credits.credit(&c).await.map_err(AppError::Internal)? {
        Credited::New => {
            tracing::info!(driver_id = %c.driver_id, kind = %c.kind, reference_id = %c.reference_id,
                amount_cents = c.amount_cents, "earning credited");
            Ok((StatusCode::CREATED, Json(serde_json::json!({ "data": { "credited": true } }))))
        }
        Credited::Already => Ok((StatusCode::OK, Json(serde_json::json!({ "data": { "credited": false } })))),
        Credited::UnknownDriver => Err(AppError::NotFound { resource: "Driver", id: c.driver_id.to_string() }),
    }
}
