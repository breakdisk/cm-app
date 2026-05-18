//! Driver Cash Liability & Reconciliation API
//!
//! Provides endpoints for:
//!   - Drivers to view their current cash liability balance
//!   - Hub managers to execute end-of-shift cash reconciliation
//!   - Ops/Finance to query disputed ledgers and generate shift reports
//!
//! ## Endpoint Matrix
//!
//! | Method | Path                                       | Role Required      |
//! |--------|--------------------------------------------|--------------------|
//! | GET    | /v1/drivers/{driver_id}/cash-liability     | DRIVER_SELF / OPS  |
//! | GET    | /v1/drivers/{driver_id}/cash-liability/entries | OPS / FINANCE  |
//! | POST   | /v1/drivers/{driver_id}/cash-liability/remit   | HUB_MANAGER    |
//! | POST   | /v1/drivers/{driver_id}/cash-liability/reconcile | HUB_MANAGER  |
//! | GET    | /v1/shipments/{shipment_id}/billing-clearance  | INTERNAL / OPS |

use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use logisticos_errors::AppError;
use logisticos_events::{envelope::Event, producer::KafkaProducer, topics};
use logisticos_types::TenantId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{
    entities::{DriverLedger, LedgerReconciliationStatus},
    events::{DriverCashReconciled, DriverCashDisputed},
    repositories::{DriverLedgerRepository, InvoiceRepository},
};

// ── Sub-state ─────────────────────────────────────────────────────────────────

pub struct DriverCashState {
    pub ledger_repo:  Arc<dyn DriverLedgerRepository>,
    pub invoice_repo: Arc<dyn InvoiceRepository>,
    pub kafka:        Arc<KafkaProducer>,
}

/// Mount all driver cash liability routes onto a `Router<()>`.
///
/// Ownership of the state is taken here via `with_state`, so the caller
/// can `.nest()` or `.merge()` the returned router into the main application
/// router without any additional state passing.
///
/// # Typical wiring in `mod.rs`:
/// ```rust,ignore
/// let dc_state = Arc::new(DriverCashState { ledger_repo, invoice_repo, kafka });
/// let dc_router = driver_cash::router(dc_state);
/// Router::new().merge(dc_router)
/// ```
pub fn router(state: Arc<DriverCashState>) -> Router {
    Router::new()
        .route(
            "/v1/drivers/:driver_id/cash-liability",
            get(get_cash_liability),
        )
        .route(
            "/v1/drivers/:driver_id/cash-liability/entries",
            get(get_ledger_entries),
        )
        .route(
            "/v1/drivers/:driver_id/cash-liability/remit",
            post(remit_cash),
        )
        .route(
            "/v1/drivers/:driver_id/cash-liability/reconcile",
            post(reconcile_cash),
        )
        .route(
            "/v1/shipments/:shipment_id/billing-clearance",
            get(get_billing_clearance),
        )
        .with_state(state)
}

// ── Request / Response shapes ─────────────────────────────────────────────────

/// Summary of a driver's current cash liability — shown on the driver app home screen.
#[derive(Debug, Serialize)]
pub struct CashLiabilitySummary {
    pub ledger_id:              Uuid,
    pub driver_id:              Uuid,
    pub shift_id:               Option<Uuid>,
    /// Total cash the driver has collected this shift (positive).
    pub total_collected_cents:  i64,
    /// Total cash already remitted to hub (positive).
    pub total_remitted_cents:   i64,
    /// Outstanding amount still with driver (positive = owes platform).
    pub outstanding_cents:      i64,
    pub currency:               String,
    pub reconciliation_status:  String,
    pub last_reconciliation_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LedgerEntrySummary {
    pub id:           Uuid,
    pub entry_type:   String,
    pub amount_cents: i64,
    pub memo:         String,
    pub shipment_id:  Option<Uuid>,
    pub invoice_id:   Option<Uuid>,
    pub pod_id:       Option<Uuid>,
    pub created_at:   String,
}

/// POST body for recording cash handed to hub manager.
#[derive(Debug, Deserialize)]
pub struct RemitCashRequest {
    pub shift_id:    Option<Uuid>,
    pub amount_cents: i64,
    pub memo:         Option<String>,
}

/// POST body for end-of-shift reconciliation.
#[derive(Debug, Deserialize)]
pub struct ReconcileCashRequest {
    pub shift_id:     Option<Uuid>,
    /// When true, forces reconciliation even with non-zero balance
    /// (flags ledger as Disputed instead of Reconciled).
    pub force_close:  Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ReconcileResponse {
    pub ledger_id:     Uuid,
    pub final_balance: i64,
    pub status:        String,
    pub reconciled_at: String,
}

/// Response for the billing clearance gate used by Carrier / Driver-Ops services.
#[derive(Debug, Serialize)]
pub struct BillingClearanceResponse {
    pub shipment_id:          Uuid,
    pub billing_domain:       Option<String>,
    pub is_cleared:           bool,
    pub unpaid_invoice_count: i64,
    pub unpaid_invoice_ids:   Vec<Uuid>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// GET /v1/drivers/{driver_id}/cash-liability
///
/// Returns the driver's current cash liability summary for their active shift.
/// Used by the Kotlin driver app home screen to display "Cash held: ₱X".
pub async fn get_cash_liability(
    State(state): State<Arc<DriverCashState>>,
    // In production, tenant_id comes from JWT claims middleware.
    // Stub: read from request extension or header for now.
    Path(driver_id): Path<Uuid>,
) -> impl IntoResponse {
    // TODO: Extract tenant_id from JWT claims in middleware.
    // For now, this is a placeholder that will be wired in router setup.
    let tenant_id = TenantId::new(); // replaced by auth middleware

    match state.ledger_repo
        .find_active_for_driver(&tenant_id, driver_id)
        .await
    {
        Ok(Some(ledger)) => Json(CashLiabilitySummary {
            ledger_id:              ledger.id,
            driver_id:              ledger.driver_id,
            shift_id:               ledger.shift_id,
            total_collected_cents:  ledger.total_collected_cents(),
            total_remitted_cents:   ledger.total_remitted_cents(),
            outstanding_cents:      ledger.outstanding_cents(),
            currency:               format!("{:?}", ledger.currency),
            reconciliation_status:  format!("{:?}", ledger.reconciliation_status)
                                    .to_lowercase(),
            last_reconciliation_at: ledger.last_reconciliation_at
                                    .map(|t| t.to_rfc3339()),
        }).into_response(),

        Ok(None) => {
            // No active ledger — driver has no cash liability this shift.
            Json(CashLiabilitySummary {
                ledger_id:              Uuid::nil(),
                driver_id,
                shift_id:               None,
                total_collected_cents:  0,
                total_remitted_cents:   0,
                outstanding_cents:      0,
                currency:               "PHP".into(),
                reconciliation_status:  "none".into(),
                last_reconciliation_at: None,
            }).into_response()
        }

        Err(e) => {
            tracing::error!(driver_id = %driver_id, error = %e, "Failed to fetch cash liability");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch cash liability").into_response()
        }
    }
}

/// GET /v1/drivers/{driver_id}/cash-liability/entries
///
/// Full audit trail of all cash movements for the driver's active shift.
/// Ops/Finance use this during reconciliation disputes.
pub async fn get_ledger_entries(
    State(state): State<Arc<DriverCashState>>,
    Path(driver_id): Path<Uuid>,
) -> impl IntoResponse {
    let tenant_id = TenantId::new(); // replaced by auth middleware

    match state.ledger_repo
        .find_active_for_driver(&tenant_id, driver_id)
        .await
    {
        Ok(Some(ledger)) => {
            let entries: Vec<LedgerEntrySummary> = ledger.entries.iter().map(|e| {
                LedgerEntrySummary {
                    id:           e.id,
                    entry_type:   format!("{:?}", e.entry_type).to_lowercase(),
                    amount_cents: e.amount_cents,
                    memo:         e.memo.clone(),
                    shipment_id:  e.shipment_id,
                    invoice_id:   e.invoice_id,
                    pod_id:       e.pod_id,
                    created_at:   e.created_at.to_rfc3339(),
                }
            }).collect();
            Json(entries).into_response()
        }
        Ok(None) => Json(Vec::<LedgerEntrySummary>::new()).into_response(),
        Err(e) => {
            tracing::error!(driver_id = %driver_id, error = %e, "Failed to fetch ledger entries");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch ledger entries").into_response()
        }
    }
}

/// POST /v1/drivers/{driver_id}/cash-liability/remit
///
/// Hub manager records cash physically received from driver (partial or full shift close).
/// Can be called multiple times before `reconcile`.
pub async fn remit_cash(
    State(state): State<Arc<DriverCashState>>,
    Path(driver_id): Path<Uuid>,
    Json(req): Json<RemitCashRequest>,
) -> impl IntoResponse {
    if req.amount_cents <= 0 {
        return (StatusCode::BAD_REQUEST, "amount_cents must be positive").into_response();
    }

    let tenant_id = TenantId::new(); // replaced by auth middleware

    let ledger_result = state.ledger_repo
        .find_active_for_driver(&tenant_id, driver_id)
        .await;

    match ledger_result {
        Ok(Some(mut ledger)) => {
            let memo = req.memo.unwrap_or_else(|| {
                format!("Cash remitted to hub — {} cents", req.amount_cents)
            });

            if let Err(e) = ledger.record_cash_remitted(req.amount_cents, memo) {
                return (StatusCode::CONFLICT, e.to_string()).into_response();
            }

            match state.ledger_repo.save(&ledger).await {
                Ok(_) => {
                    tracing::info!(
                        driver_id    = %driver_id,
                        amount_cents = req.amount_cents,
                        balance      = ledger.balance_cents,
                        "Cash remitted to hub"
                    );
                    Json(CashLiabilitySummary {
                        ledger_id:              ledger.id,
                        driver_id:              ledger.driver_id,
                        shift_id:               ledger.shift_id,
                        total_collected_cents:  ledger.total_collected_cents(),
                        total_remitted_cents:   ledger.total_remitted_cents(),
                        outstanding_cents:      ledger.outstanding_cents(),
                        currency:               format!("{:?}", ledger.currency),
                        reconciliation_status:  "pending".into(),
                        last_reconciliation_at: None,
                    }).into_response()
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to save ledger after remit");
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save ledger").into_response()
                }
            }
        }
        Ok(None) => (StatusCode::NOT_FOUND, "No active ledger for driver").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to fetch ledger for remit");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch ledger").into_response()
        }
    }
}

/// POST /v1/drivers/{driver_id}/cash-liability/reconcile
///
/// Hub manager executes end-of-shift reconciliation.
///
/// Business rules:
///   - If balance == 0 → status = Reconciled. Clean close.
///   - If balance != 0 and force_close = false → HTTP 409 (balance outstanding).
///   - If balance != 0 and force_close = true  → status = Disputed. Emits alert.
pub async fn reconcile_cash(
    State(state): State<Arc<DriverCashState>>,
    Path(driver_id): Path<Uuid>,
    Json(req): Json<ReconcileCashRequest>,
) -> impl IntoResponse {
    let tenant_id     = TenantId::new(); // replaced by auth middleware
    let reconciled_by = Uuid::new_v4();  // replaced by JWT sub claim

    let ledger_result = state.ledger_repo
        .find_active_for_driver(&tenant_id, driver_id)
        .await;

    let mut ledger = match ledger_result {
        Ok(Some(l)) => l,
        Ok(None) => return (StatusCode::NOT_FOUND, "No active ledger for driver").into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to fetch ledger for reconcile");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch ledger").into_response();
        }
    };

    // Guard: cannot reconcile if balance outstanding and force_close not set
    let force = req.force_close.unwrap_or(false);
    if !force && ledger.balance_cents != 0 {
        return (
            StatusCode::CONFLICT,
            format!(
                "Ledger has outstanding balance of {} cents. \
                 Collect remaining cash or set force_close=true to flag as disputed.",
                ledger.balance_cents.abs()
            ),
        ).into_response();
    }

    let final_balance = ledger.balance_cents;
    let total_collected = ledger.total_collected_cents();
    let total_remitted  = ledger.total_remitted_cents();

    match ledger.reconcile(reconciled_by, force) {
        Err(e) => return (StatusCode::CONFLICT, e.to_string()).into_response(),
        Ok(status) => {
            if let Err(e) = state.ledger_repo.save(&ledger).await {
                tracing::error!(error = %e, "Failed to save reconciled ledger");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save ledger").into_response();
            }

            // Publish reconciliation event
            let now = Utc::now();
            let (topic, event_payload) = match status {
                LedgerReconciliationStatus::Reconciled => {
                    let payload = DriverCashReconciled {
                        ledger_id:             ledger.id,
                        tenant_id:             tenant_id.inner(),
                        driver_id,
                        shift_id:              ledger.shift_id,
                        total_collected_cents: total_collected,
                        total_remitted_cents:  total_remitted,
                        final_balance_cents:   final_balance,
                        currency:              format!("{:?}", ledger.currency),
                        reconciled_by,
                        reconciled_at:         now.to_rfc3339(),
                    };
                    tracing::info!(
                        driver_id    = %driver_id,
                        ledger_id    = %ledger.id,
                        "Cash reconciliation complete — clean close"
                    );
                    (topics::DRIVER_CASH_RECONCILED, serde_json::to_value(payload).unwrap())
                }
                LedgerReconciliationStatus::Disputed => {
                    let payload = DriverCashDisputed {
                        ledger_id:         ledger.id,
                        tenant_id:         tenant_id.inner(),
                        driver_id,
                        shift_id:          ledger.shift_id,
                        outstanding_cents: final_balance.abs(),
                        currency:          format!("{:?}", ledger.currency),
                        flagged_by:        reconciled_by,
                        flagged_at:        now.to_rfc3339(),
                    };
                    tracing::warn!(
                        driver_id         = %driver_id,
                        outstanding_cents = final_balance.abs(),
                        "Cash reconciliation disputed — non-zero balance"
                    );
                    (topics::DRIVER_CASH_DISPUTED, serde_json::to_value(payload).unwrap())
                }
                LedgerReconciliationStatus::Pending => unreachable!(),
            };

            // Fire-and-forget Kafka — reconciliation audit is not blocking
            let kafka = Arc::clone(&state.kafka);
            tokio::spawn(async move {
                let evt = Event::new_raw("payments", topic, tenant_id.inner(), event_payload);
                if let Err(e) = kafka.publish_event(topic, &evt).await {
                    tracing::error!(error = %e, "Failed to publish reconciliation event");
                }
            });

            Json(ReconcileResponse {
                ledger_id:     ledger.id,
                final_balance: ledger.balance_cents,
                status:        format!("{:?}", ledger.reconciliation_status).to_lowercase(),
                reconciled_at: Utc::now().to_rfc3339(),
            }).into_response()
        }
    }
}

/// GET /v1/shipments/{shipment_id}/billing-clearance
///
/// Manifest gate query — used by Dispatch, Carrier, and Driver-Ops services
/// before allowing critical operations:
///   - Balikbayan: before assigning to sea container / sealing customs manifest
///   - Standard Parcel: before allowing delivery release (overage hold check)
///
/// Returns HTTP 200 with `is_cleared: true` when all invoices are paid.
/// Returns HTTP 200 with `is_cleared: false` + `unpaid_invoice_ids` when not cleared.
/// The CALLER is responsible for interpreting `is_cleared: false` as a gate rejection.
pub async fn get_billing_clearance(
    State(state): State<Arc<DriverCashState>>,
    Path(shipment_id): Path<Uuid>,
) -> impl IntoResponse {
    let tenant_id = TenantId::new(); // replaced by auth middleware

    match state.invoice_repo
        .get_billing_clearance(&tenant_id, shipment_id)
        .await
    {
        Ok(Some(clearance)) => Json(BillingClearanceResponse {
            shipment_id,
            billing_domain:       Some(clearance.billing_domain),
            is_cleared:           clearance.is_cleared,
            unpaid_invoice_count: clearance.unpaid_invoice_count,
            unpaid_invoice_ids:   clearance.unpaid_invoice_ids,
        }).into_response(),

        Ok(None) => {
            // No billing domain entries for this shipment — cleared by default
            // (merchant_periodic shipments don't gate on invoice status).
            Json(BillingClearanceResponse {
                shipment_id,
                billing_domain:       None,
                is_cleared:           true,
                unpaid_invoice_count: 0,
                unpaid_invoice_ids:   vec![],
            }).into_response()
        }

        Err(e) => {
            tracing::error!(
                shipment_id = %shipment_id,
                error = %e,
                "Failed to query billing clearance"
            );
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to query billing clearance").into_response()
        }
    }
}
