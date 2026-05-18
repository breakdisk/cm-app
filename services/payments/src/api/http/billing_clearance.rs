//! Internal service-to-service endpoint for billing clearance checks.
//!
//! Called by hub-ops before allowing a container to depart. Returns the
//! billing clearance status for a batch of shipment IDs.
//!
//! Route: `GET /v1/internal/billing-clearance?shipment_ids=<uuid>&shipment_ids=<uuid>...`
//!
//! No JWT required — the route sits behind Istio mTLS which gates caller identity.
//!
//! Response shape:
//! ```json
//! {
//!   "clearances": [
//!     { "shipment_id": "...", "is_cleared": true,  "unpaid_invoice_ids": [] },
//!     { "shipment_id": "...", "is_cleared": false, "unpaid_invoice_ids": ["..."] }
//!   ],
//!   "all_cleared": false
//! }
//! ```

use axum::{extract::{Query, State}, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct ClearanceQuery {
    #[serde(rename = "shipment_ids")]
    pub shipment_ids: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct ShipmentClearance {
    pub shipment_id:       Uuid,
    pub is_cleared:        bool,
    pub unpaid_invoice_ids: Vec<Uuid>,
}

#[derive(Serialize)]
pub struct BillingClearanceResponse {
    pub clearances:  Vec<ShipmentClearance>,
    pub all_cleared: bool,
}

/// `GET /v1/internal/billing-clearance?shipment_ids=<uuid>&...`
///
/// Returns clearing status for each requested shipment.
/// A shipment is "cleared" when it has no invoices in `draft` or `issued` status
/// that are outstanding (i.e., all are `paid`, `void`, or no invoice exists yet).
///
/// For Track A (Balikbayan) container departure, the guard is:
///   - If a non-draft invoice exists → cleared (invoice was raised, billing chain is open).
///   - If no invoice exists at all → cleared (parcel may not have a billing obligation).
///   - If any invoice is in `draft` status → NOT cleared (billing setup incomplete).
pub async fn get_billing_clearance(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ClearanceQuery>,
) -> Result<Json<BillingClearanceResponse>, AppError> {
    if q.shipment_ids.is_empty() {
        return Ok(Json(BillingClearanceResponse {
            clearances:  vec![],
            all_cleared: true,
        }));
    }

    let clearances = state.invoice_service
        .check_billing_clearance_batch(&q.shipment_ids)
        .await?;

    let all_cleared = clearances.iter().all(|c| c.is_cleared);

    Ok(Json(BillingClearanceResponse {
        clearances,
        all_cleared,
    }))
}
