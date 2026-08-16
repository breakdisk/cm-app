//! Where the courier is, as omnideliv needs it.
//!
//! The trait belongs to omnideliv and the implementation calls field-ops, so
//! the dependency points inward exactly as `CourierDispatch` does. field-ops
//! knows nothing about this caller.

use async_trait::async_trait;
use uuid::Uuid;

/// A courier position, already judged for freshness.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CourierFix {
    pub lat:                f64,
    pub lng:                f64,
    pub heading_deg:        Option<f32>,
    pub smoothed_speed_kph: Option<f64>,
    pub age_seconds:        i64,
}

#[async_trait]
pub trait CourierTelemetry: Send + Sync {
    /// `None` when there is no assignment, no fix, or field-ops cannot answer.
    async fn position(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<CourierFix>>;
}

/// Used when field-ops is unreachable at startup, mirroring `NoopOrderEvents`.
///
/// A tracking screen without a dot is a worse screen. A tracking screen that
/// 500s is no screen at all, and the order state, the timeline and the amount
/// owed are all still worth serving.
pub struct NoopCourierTelemetry;

#[async_trait]
impl CourierTelemetry for NoopCourierTelemetry {
    async fn position(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierFix>> {
        Ok(None)
    }
}
