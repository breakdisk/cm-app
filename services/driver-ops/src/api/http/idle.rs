//! `GET /v1/internal/drivers/idle` — drivers who just crossed an idle threshold.
//! Mesh-internal: not behind the JWT layer, refused by the gateway. The tenant
//! comes from the calling service, per the internal-route contract.

use std::sync::Arc;

use axum::{extract::{Query, State}, Json};
use chrono::Utc;
use logisticos_errors::AppError;
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;

#[derive(Debug, Deserialize)]
pub struct IdleQuery {
    pub tenant_id:    Uuid,
    pub idle_minutes: u32,
    pub window_secs:  u32,
}

pub fn validate(q: &IdleQuery) -> Result<(), AppError> {
    if !(1..=1440).contains(&q.idle_minutes) {
        return Err(AppError::Validation("idle_minutes must be in 1..=1440".into()));
    }
    if !(30..=3600).contains(&q.window_secs) {
        return Err(AppError::Validation("window_secs must be in 30..=3600".into()));
    }
    Ok(())
}

pub async fn internal_idle_drivers(
    State(state): State<Arc<AppState>>,
    Query(q): Query<IdleQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate(&q)?;
    let drivers = state
        .idle
        .idle_crossing(q.tenant_id, q.idle_minutes, q.window_secs, Utc::now())
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": drivers })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(idle_minutes: u32, window_secs: u32) -> IdleQuery {
        IdleQuery { tenant_id: Uuid::new_v4(), idle_minutes, window_secs }
    }

    #[test]
    fn accepts_the_range_edges() {
        for (m, w) in [(1, 30), (1440, 3600), (30, 60)] {
            assert!(validate(&q(m, w)).is_ok(), "{m}/{w}");
        }
    }

    #[test]
    fn rejects_out_of_range_with_validation_error() {
        for (m, w) in [(0, 60), (1441, 60), (30, 29), (30, 3601), (0, 0)] {
            assert!(matches!(validate(&q(m, w)), Err(AppError::Validation(_))), "{m}/{w}");
        }
    }
}
