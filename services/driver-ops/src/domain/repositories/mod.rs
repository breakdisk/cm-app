use async_trait::async_trait;
use chrono::NaiveDate;
use logisticos_types::{DriverId, TenantId};
use serde::Serialize;
use uuid::Uuid;
use crate::domain::entities::{Driver, DriverTask, DriverLocation};

/// Tenant-wide task counts for the admin summary KPI strip.
#[derive(Debug, Clone, Serialize)]
pub struct TenantTaskSummary {
    pub total_assigned:  i64,
    pub total_completed: i64,
    pub total_failed:    i64,
    pub cod_collected_cents: i64,
}

/// One manifest row per (driver, date, task_type) tuple. Used by the partner
/// portal to surface a daily operational summary without having to page
/// through individual tasks. Counts are computed server-side via SQL
/// aggregation for O(tasks) cost rather than N+1.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    pub driver_id:   Uuid,
    pub driver_name: String,
    pub task_type:   String,       // "pickup" | "delivery"
    pub total:       i64,
    pub completed:   i64,
    pub failed:      i64,
    pub in_progress: i64,
    pub pending:     i64,
}

/// Driver performance counters (decline accountability + customer rating).
/// Kept off the `Driver` entity so the dozens of existing struct literals
/// (registration, tests) don't need updating — these are read on demand for
/// the profile API and the driver-app performance strip.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DriverPerformance {
    pub decline_count: i32,
    pub rating_avg:    Option<f32>,
    pub rating_count:  i32,
}

#[async_trait]
pub trait DriverRepository: Send + Sync {
    async fn find_by_id(&self, id: &DriverId) -> anyhow::Result<Option<Driver>>;
    async fn find_by_user_id(&self, user_id: Uuid) -> anyhow::Result<Option<Driver>>;
    async fn list_by_tenant(&self, tenant_id: &TenantId) -> anyhow::Result<Vec<Driver>>;
    async fn save(&self, driver: &Driver) -> anyhow::Result<()>;
    /// Hard-delete a driver row. The caller (DriverService) must verify there
    /// are no active/pending tasks before calling this.
    async fn delete(&self, id: &DriverId) -> anyhow::Result<bool>;
    /// Performance counters for the driver-app performance strip. Default
    /// implementation returns None so in-memory test repositories keep
    /// compiling without stubbing it.
    async fn get_performance(&self, _user_id: Uuid) -> anyhow::Result<Option<DriverPerformance>> {
        Ok(None)
    }
    /// Gig offer stats: (impression-verified offers seen, offers claimed).
    /// Drives the acceptance-rate strip — the broadcast-model replacement for
    /// the decline counter (passing an offer nine others saw is not a
    /// refusal). Default None keeps in-memory test repos compiling.
    async fn get_offer_stats(&self, _user_id: Uuid) -> anyhow::Result<Option<(i64, i64)>> {
        Ok(None)
    }
}

#[async_trait]
pub trait TaskRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverTask>>;
    async fn list_by_driver(&self, driver_id: &DriverId) -> anyhow::Result<Vec<DriverTask>>;
    async fn list_by_route(&self, route_id: Uuid) -> anyhow::Result<Vec<DriverTask>>;
    async fn save(&self, task: &DriverTask) -> anyhow::Result<()>;
    async fn bulk_save(&self, tasks: &[DriverTask]) -> anyhow::Result<()>;

    /// Aggregate tasks for a given date, optionally filtered to a single
    /// carrier's drivers. Returns one row per (driver, task_type) with
    /// status counts.
    async fn list_manifest(
        &self,
        tenant_id: &TenantId,
        carrier_id: Option<Uuid>,
        date: NaiveDate,
    ) -> anyhow::Result<Vec<ManifestEntry>>;

    /// Tenant-wide task summary for today — drives the admin KPI strip.
    async fn tenant_summary(&self, tenant_id: &TenantId, date: NaiveDate) -> anyhow::Result<TenantTaskSummary>;

    /// Admin operation: cancel all `pending` and `in_progress` tasks for a
    /// driver (identified by their `drivers.id`). Used to clear a stale task
    /// queue so the driver can receive new auto-dispatch assignments.
    /// Returns the number of rows updated.
    async fn cancel_all_for_driver(&self, driver_id: &DriverId) -> anyhow::Result<u64>;

    /// Per-task earnings entries for the driver (identified by identity
    /// user_id), newest first. Only completed DELIVERY tasks count — a
    /// shipment's pickup+delivery pair pays once, on the delivery leg.
    /// Default impl returns empty so in-memory test repositories keep
    /// compiling without stubbing it (same pattern as get_performance).
    async fn list_earnings(
        &self,
        _user_id: Uuid,
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
        _limit: i64,
        _offset: i64,
    ) -> anyhow::Result<Vec<EarningEntry>> {
        Ok(Vec::new())
    }

    /// Daily payout totals over the window — drives the Today / This Week
    /// summary and the 7-day sparkline. Indexed range aggregate; an async
    /// rollup table was reviewed and rejected as YAGNI at current volumes.
    async fn daily_earnings(
        &self,
        _user_id: Uuid,
        _from: chrono::DateTime<chrono::Utc>,
        _to: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<DailyEarning>> {
        Ok(Vec::new())
    }
}

/// One earnings line: a completed delivery task with its contractual payout.
#[derive(Debug, Clone, Serialize)]
pub struct EarningEntry {
    pub task_id:           Uuid,
    pub tracking_number:   Option<String>,
    pub merchant_name:     String,
    pub delivery_category: String,
    pub completed_at:      Option<chrono::DateTime<chrono::Utc>>,
    pub payout_cents:      Option<i64>,
}

/// Payout total for one calendar day (UTC).
#[derive(Debug, Clone, Serialize)]
pub struct DailyEarning {
    pub day:         NaiveDate,
    pub total_cents: i64,
    pub deliveries:  i64,
}

#[async_trait]
pub trait LocationRepository: Send + Sync {
    /// Append a location record to the time-series table.
    async fn record(&self, location: &DriverLocation) -> anyhow::Result<()>;
    /// Get the most recent location for a driver.
    async fn latest(&self, driver_id: &DriverId) -> anyhow::Result<Option<DriverLocation>>;
    /// Get location trail over a time range (for replay/audit).
    async fn history(
        &self,
        driver_id: &DriverId,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<Vec<DriverLocation>>;
}
