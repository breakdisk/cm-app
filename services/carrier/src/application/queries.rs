use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use logisticos_types::TenantId;

// ── Carrier queries ───────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct GetCarrierByIdQuery {
    pub carrier_id: Uuid,
}

/// Resolves a carrier by the authenticated user's email — used by the partner
/// portal `/v1/carriers/me` endpoint so the partner doesn't need to know its
/// own UUID.
#[derive(Debug)]
pub struct GetCarrierByEmailQuery {
    pub tenant_id: TenantId,
    pub email:     String,
}

#[derive(Debug, Deserialize)]
pub struct ListCarriersQuery {
    pub tenant_id: TenantId,
    #[serde(default = "default_limit")]
    pub limit:     i64,
    #[serde(default)]
    pub offset:    i64,
}

fn default_limit() -> i64 { 50 }

/// Return quotes from all active carriers for a given service + weight.
/// Sorted by total cost ascending on the read side.
#[derive(Debug, Deserialize)]
pub struct RateShopQuery {
    pub tenant_id:    TenantId,
    pub service_type: String,
    pub weight_kg:    f32,
}

// ── SLA queries ───────────────────────────────────────────────────────────────

/// Zone-level SLA aggregate for a carrier over a rolling window.
/// Backed by `GET /v1/carriers/:id/sla-summary?from=&to=`.
#[derive(Debug)]
pub struct SlaZoneSummaryQuery {
    pub carrier_id: Uuid,
    pub from:       DateTime<Utc>,
    pub to:         DateTime<Utc>,
}

/// Paginated per-shipment SLA history for the partner portal detail view.
/// Backed by `GET /v1/carriers/:id/sla-history?limit=&offset=`.
#[derive(Debug, Deserialize)]
pub struct SlaHistoryQuery {
    pub carrier_id: Uuid,
    #[serde(default = "default_limit")]
    pub limit:      i64,
    #[serde(default)]
    pub offset:     i64,
}

// ── Query results ─────────────────────────────────────────────────────────────

/// Thin wrapper so callers don't need to build paginated shapes ad-hoc.
#[derive(Debug)]
pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    /// Total row count (without the LIMIT — useful for page controls).
    /// `None` when the backend returns only the current page length.
    pub total: Option<i64>,
}

impl<T> PaginatedResult<T> {
    pub fn new(items: Vec<T>) -> Self {
        let total = items.len() as i64;
        Self { items, total: Some(total) }
    }

    pub fn with_total(items: Vec<T>, total: i64) -> Self {
        Self { items, total: Some(total) }
    }
}
