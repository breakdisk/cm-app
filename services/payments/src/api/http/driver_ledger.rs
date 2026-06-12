//! Driver-scoped COD cash-position view.
//!
//! `GET /v1/cod/driver-ledger/me` — the driver app's "Cash to Remit" card and
//! Cash history tab. Lives under `/v1/cod/` so the existing API-gateway prefix
//! routes it to payments without a gateway change (`/v1/drivers/*` routes to
//! driver-ops).
//!
//! Read-only: no money movement. The driver identity comes from the JWT
//! (`claims.user_id`), which is the same identity UUID the ledger consumers
//! key on — drivers can only ever see their own ledgers.

use axum::{extract::State, Json};
use std::sync::Arc;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use logisticos_types::TenantId;

use crate::api::http::AppState;
use crate::domain::entities::DriverLedger;

/// How many historical ledgers (shifts) to return. One ledger per shift —
/// 30 covers a month of daily shifts, plenty for the in-app history view.
const RECENT_LEDGER_LIMIT: i64 = 30;

pub async fn get_my_ledger(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    let ledgers = state.driver_ledger_repo
        .list_recent_for_driver(&tenant_id, claims.user_id, RECENT_LEDGER_LIMIT)
        .await
        .map_err(AppError::Internal)?;

    // The open ledger (at most one by invariant) carries the live balance the
    // driver owes the hub; reconciled ones are pure history.
    let open: Option<&DriverLedger> = ledgers.iter().find(|l| l.is_open());

    let open_balance_cents = open.map(|l| l.balance_cents).unwrap_or(0);
    let open_entries = open.map(|l| l.entries.clone()).unwrap_or_default();

    let recent: Vec<serde_json::Value> = ledgers.iter().map(|l| {
        serde_json::json!({
            "id":            l.id,
            "status":        l.status,
            "balance_cents": l.balance_cents,
            "created_at":    l.created_at,
            "entry_count":   l.entries.len(),
            "entries":       l.entries,
        })
    }).collect();

    Ok(Json(serde_json::json!({
        "data": {
            "open_balance_cents": open_balance_cents,
            "open_entries":       open_entries,
            "recent_ledgers":     recent,
        }
    })))
}
