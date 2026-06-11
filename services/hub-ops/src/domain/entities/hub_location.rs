use chrono::{DateTime, Utc};
use logisticos_types::{HubId, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A physical slot inside a hub warehouse — the structural topology tree
/// (zone → aisle → rack → shelf → bin). Created by ops via the admin portal and
/// tenant-scoped. Inventory location is referenced by `id`, never by free text on
/// shipment/container rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubLocation {
    pub id:           Uuid,
    pub tenant_id:    TenantId,
    pub hub_id:       HubId,
    /// e.g. "ZONE_A_INBOUND", "ZONE_C_CROSSDOCK".
    pub zone_id:      String,
    pub aisle:        Option<String>,
    pub rack:         Option<String>,
    pub shelf:        Option<String>,
    pub bin:          Option<String>,
    /// Globally unique scannable tag, e.g. "HUB1-Z-A12-B04-L3-BB".
    pub location_tag: String,
    /// Physical capacity in cubic metres; prevents overloading a slot.
    pub capacity_cbm: Option<f64>,
    pub is_active:    bool,
    pub created_at:   DateTime<Utc>,
}

impl HubLocation {
    pub fn new(
        tenant_id:    TenantId,
        hub_id:       HubId,
        zone_id:      impl Into<String>,
        location_tag: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            hub_id,
            zone_id: zone_id.into(),
            aisle: None,
            rack: None,
            shelf: None,
            bin: None,
            location_tag: location_tag.into(),
            capacity_cbm: None,
            is_active: true,
            created_at: Utc::now(),
        }
    }

    pub fn with_path(
        mut self,
        aisle: Option<String>,
        rack:  Option<String>,
        shelf: Option<String>,
        bin:   Option<String>,
    ) -> Self {
        self.aisle = aisle;
        self.rack  = rack;
        self.shelf = shelf;
        self.bin   = bin;
        self
    }

    /// Human-readable path from the structural components, e.g.
    /// "ZONE_A_INBOUND / Aisle 12 / Bay 04 / Level 3 / Bin B".
    pub fn path(&self) -> String {
        let mut parts = vec![self.zone_id.clone()];
        for p in [&self.aisle, &self.rack, &self.shelf, &self.bin].into_iter().flatten() {
            parts.push(p.clone());
        }
        parts.join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults_active_with_empty_path_components() {
        let loc = HubLocation::new(TenantId::new(), HubId::new(), "ZONE_A_INBOUND", "HUB1-Z-A");
        assert!(loc.is_active);
        assert!(loc.aisle.is_none());
        assert_eq!(loc.path(), "ZONE_A_INBOUND");
    }

    #[test]
    fn path_joins_present_components_in_order() {
        let loc = HubLocation::new(TenantId::new(), HubId::new(), "ZONE_A", "TAG")
            .with_path(Some("Aisle 12".into()), Some("Bay 04".into()), None, Some("Bin B".into()));
        // shelf is None and must be skipped, preserving order
        assert_eq!(loc.path(), "ZONE_A / Aisle 12 / Bay 04 / Bin B");
    }
}
