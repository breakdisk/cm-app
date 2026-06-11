use chrono::{DateTime, Utc};
use logisticos_types::{HubId, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Last-mile handoff strategy at container release time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingType {
    /// Always use own dispatch.
    OwnDriver,
    /// Always use a 3PL carrier.
    Carrier,
    /// Try own driver, fall back to carrier after the fallback window.
    Auto,
}

/// Tenant + hub-level rule deciding own-driver vs. 3PL at release time.
/// Keyed by `(tenant_id, hub_id, destination_zone)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubRoutingConfig {
    pub id:                        Uuid,
    pub tenant_id:                 TenantId,
    pub hub_id:                    HubId,
    /// e.g. "Metro Manila", "Visayas".
    pub destination_zone:          String,
    pub routing_type:              RoutingType,
    /// Which 3PL for Carrier / Auto-fallback mode.
    pub carrier_id:                Option<Uuid>,
    /// Auto mode: minutes before falling back to 3PL. Ignored for non-Auto.
    pub auto_fallback_window_mins: i32,
    pub created_at:                DateTime<Utc>,
    pub updated_at:                DateTime<Utc>,
}

impl HubRoutingConfig {
    pub fn new(
        tenant_id:        TenantId,
        hub_id:           HubId,
        destination_zone: impl Into<String>,
        routing_type:     RoutingType,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            hub_id,
            destination_zone: destination_zone.into(),
            routing_type,
            carrier_id: None,
            auto_fallback_window_mins: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_carrier(mut self, carrier_id: Uuid) -> Self {
        self.carrier_id = Some(carrier_id);
        self
    }

    pub fn with_fallback_window(mut self, mins: i32) -> Self {
        self.auto_fallback_window_mins = mins;
        self
    }

    /// True when the first hop should be an own-driver dispatch request
    /// (`OwnDriver` always, `Auto` first-attempt).
    pub fn dispatches_own_driver_first(&self) -> bool {
        matches!(self.routing_type, RoutingType::OwnDriver | RoutingType::Auto)
    }

    /// True when an unfulfilled own-driver dispatch should escalate to a carrier.
    pub fn falls_back_to_carrier(&self) -> bool {
        matches!(self.routing_type, RoutingType::Auto)
    }

    /// True when the shipment goes straight to a carrier booking, skipping dispatch.
    pub fn carrier_only(&self) -> bool {
        matches!(self.routing_type, RoutingType::Carrier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_driver_dispatches_first_and_does_not_fall_back() {
        let c = HubRoutingConfig::new(TenantId::new(), HubId::new(), "Metro Manila", RoutingType::OwnDriver);
        assert!(c.dispatches_own_driver_first());
        assert!(!c.falls_back_to_carrier());
        assert!(!c.carrier_only());
    }

    #[test]
    fn carrier_only_skips_dispatch() {
        let c = HubRoutingConfig::new(TenantId::new(), HubId::new(), "Visayas", RoutingType::Carrier)
            .with_carrier(Uuid::new_v4());
        assert!(c.carrier_only());
        assert!(!c.dispatches_own_driver_first());
        assert!(c.carrier_id.is_some());
    }

    #[test]
    fn auto_dispatches_first_then_falls_back() {
        let c = HubRoutingConfig::new(TenantId::new(), HubId::new(), "Mindanao", RoutingType::Auto)
            .with_fallback_window(30);
        assert!(c.dispatches_own_driver_first());
        assert!(c.falls_back_to_carrier());
        assert_eq!(c.auto_fallback_window_mins, 30);
    }
}
