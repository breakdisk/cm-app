use chrono::{DateTime, Utc};
use logisticos_types::{awb::ChildAwb, HubId, PalletId, TenantId};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a single inventory record tracks.
///
/// When a piece is assigned to a pallet, its `Piece` record is superseded and a
/// single `Consolidated` record becomes authoritative for the whole pallet — so
/// moving a pallet is exactly **one** inventory update regardless of piece count.
/// At break-bulk the `Piece` records are re-created and the `Consolidated` record
/// is archived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum InventoryUnit {
    Piece(ChildAwb),
    Consolidated(PalletId),
}

/// The current warehouse location of a piece or consolidated pallet.
///
/// This is the **only mutable hub entity** — `location_id` is updated in place
/// when inventory physically moves (one row per unit, not per move).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubInventory {
    pub id:             Uuid,
    pub tenant_id:      TenantId,
    pub hub_id:         HubId,
    /// → HubLocation.id
    pub location_id:    Uuid,
    pub inventory_unit: InventoryUnit,
    pub batch_number:   Option<String>,
    pub checked_in_by:  Uuid,
    pub checked_in_at:  DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

impl HubInventory {
    pub fn check_in(
        tenant_id:      TenantId,
        hub_id:         HubId,
        location_id:    Uuid,
        inventory_unit: InventoryUnit,
        checked_in_by:  Uuid,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            hub_id,
            location_id,
            inventory_unit,
            batch_number: None,
            checked_in_by,
            checked_in_at: now,
            updated_at: now,
        }
    }

    /// Move this unit to a new location in place. No-op timestamp bump is avoided
    /// when the location is unchanged.
    pub fn move_to(&mut self, location_id: Uuid) {
        if self.location_id != location_id {
            self.location_id = location_id;
            self.updated_at  = Utc::now();
        }
    }

    pub fn is_consolidated(&self) -> bool {
        matches!(self.inventory_unit, InventoryUnit::Consolidated(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::{Awb, ServiceCode, TenantCode};

    fn piece_unit() -> InventoryUnit {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        InventoryUnit::Piece(ChildAwb::new(&master, 1).unwrap())
    }

    fn inv(unit: InventoryUnit) -> HubInventory {
        HubInventory::check_in(TenantId::new(), HubId::new(), Uuid::new_v4(), unit, Uuid::new_v4())
    }

    #[test]
    fn move_to_changes_location_and_bumps_updated_at() {
        let mut i = inv(piece_unit());
        let original = i.updated_at;
        let dest = Uuid::new_v4();
        // ensure a measurable clock tick
        std::thread::sleep(std::time::Duration::from_millis(2));
        i.move_to(dest);
        assert_eq!(i.location_id, dest);
        assert!(i.updated_at > original);
    }

    #[test]
    fn move_to_same_location_is_noop() {
        let mut i = inv(piece_unit());
        let same = i.location_id;
        let original = i.updated_at;
        i.move_to(same);
        assert_eq!(i.updated_at, original);
    }

    #[test]
    fn consolidated_unit_is_detected() {
        let i = inv(InventoryUnit::Consolidated(PalletId::new()));
        assert!(i.is_consolidated());
    }

    #[test]
    fn piece_unit_is_not_consolidated() {
        assert!(!inv(piece_unit()).is_consolidated());
    }
}
