//! `GET /v1/internal/drivers/idle` — drivers who just crossed an idle threshold.
//! Mesh-internal: not behind the JWT layer, refused by the gateway. The tenant
//! comes from the calling service, per the internal-route contract.

use std::sync::Arc;

use axum::{extract::{Query, State}, Json};
use chrono::{DateTime, Utc};
use logisticos_errors::AppError;
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::infrastructure::db::idle_repo::resolve_since;

#[derive(Debug, Deserialize)]
pub struct IdleQuery {
    pub tenant_id:    Uuid,
    pub idle_minutes: u32,
    /// Required unless `since` is given; when `since` is given it is ignored.
    pub window_secs:  Option<u32>,
    /// Watermark (RFC3339): return episodes whose crossing time is in `(since, as_of]`.
    pub since:        Option<DateTime<Utc>>,
}

pub fn validate(q: &IdleQuery) -> Result<(), AppError> {
    if !(1..=1440).contains(&q.idle_minutes) {
        return Err(AppError::Validation("idle_minutes must be in 1..=1440".into()));
    }
    match q.window_secs {
        Some(w) if !(30..=3600).contains(&w) => {
            Err(AppError::Validation("window_secs must be in 30..=3600".into()))
        }
        None if q.since.is_none() => {
            Err(AppError::Validation("window_secs or since is required".into()))
        }
        _ => Ok(()),
    }
}

pub async fn internal_idle_drivers(
    State(state): State<Arc<AppState>>,
    Query(q): Query<IdleQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    validate(&q)?;
    // One clock reading per request: it bounds the query and is handed back as
    // the caller's next watermark.
    let as_of = Utc::now();
    let since = resolve_since(as_of, q.window_secs.unwrap_or(0), q.since).map_err(AppError::Validation)?;
    let drivers = state
        .idle
        .idle_crossing(q.tenant_id, q.idle_minutes, since, as_of)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "data": drivers, "as_of": as_of })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(idle_minutes: u32, window_secs: u32) -> IdleQuery {
        IdleQuery { tenant_id: Uuid::new_v4(), idle_minutes, window_secs: Some(window_secs), since: None }
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

    #[test]
    fn window_secs_is_optional_only_with_since() {
        let mut x = q(30, 60);
        x.window_secs = None;
        assert!(matches!(validate(&x), Err(AppError::Validation(_))));
        x.since = Some(Utc::now());
        assert!(validate(&x).is_ok());
        // given and out of range: still rejected even with since
        x.window_secs = Some(5);
        assert!(matches!(validate(&x), Err(AppError::Validation(_))));
    }
}
