use chrono::{DateTime, Utc};
use logisticos_types::{awb::ChildAwb, HubId, PieceStatus, ShipmentId};
use serde::{Deserialize, Serialize};

use crate::domain::value_objects::ShipmentDimensions;
use crate::domain::value_objects::ShipmentWeight;

/// An individual physical unit within a multi-piece shipment.
///
/// Created at booking time alongside the master AWB.  Each piece gets its own
/// `ChildAwb` barcode that hub operators scan independently.  Piece statuses
/// aggregate up to determine the parent shipment's commercial status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Piece {
    /// Internal DB identifier.
    pub id: uuid::Uuid,
    /// The shipment this piece belongs to.
    pub shipment_id: ShipmentId,
    /// 1-based piece number within the shipment (1..=999).
    pub piece_number: u16,
    /// Scannable piece-level barcode (e.g. `LS-PH1-B0009012Z-002`).
    pub piece_awb: ChildAwb,
    /// Declared weight at booking.
    pub declared_weight: ShipmentWeight,
    /// Actual weight recorded at hub (set when hub re-weighs; None until scanned).
    pub actual_weight: Option<ShipmentWeight>,
    pub dimensions: Option<ShipmentDimensions>,
    /// Short description of contents (e.g. "Clothes", "Electronics").
    pub description: Option<String>,
    pub status: PieceStatus,
    /// Hub where this piece was last scanned inbound.
    pub last_hub_id: Option<HubId>,
    pub last_scanned_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Piece {
    /// True when hub re-weighing found the actual weight exceeds declared.
    pub fn has_weight_discrepancy(&self) -> bool {
        match self.actual_weight {
            Some(actual) => actual.grams > self.declared_weight.grams,
            None => false,
        }
    }

    /// Delta in grams: positive means underweight was declared (surcharge applies).
    pub fn weight_delta_grams(&self) -> i32 {
        match self.actual_weight {
            Some(actual) => actual.grams as i32 - self.declared_weight.grams as i32,
            None => 0,
        }
    }

    /// Volumetric weight in grams using DIM factor 5000 cm³/kg.
    pub fn volumetric_weight_grams(&self) -> Option<u32> {
        self.dimensions.map(|d| d.volumetric_weight_grams())
    }

    /// Billable weight: max(actual or declared, volumetric).
    pub fn billable_weight_grams(&self) -> u32 {
        let base = self.actual_weight.unwrap_or(self.declared_weight).grams;
        match self.volumetric_weight_grams() {
            Some(vol) => base.max(vol),
            None => base,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::{Awb, ServiceCode, TenantCode};

    fn make_piece(declared_grams: u32) -> Piece {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        let piece_awb = ChildAwb::new(&master, 1).unwrap();
        Piece {
            id: uuid::Uuid::new_v4(),
            shipment_id: ShipmentId::new(),
            piece_number: 1,
            piece_awb,
            declared_weight: ShipmentWeight::from_grams(declared_grams),
            actual_weight: None,
            dimensions: None,
            description: Some("Clothes".to_string()),
            status: PieceStatus::Pending,
            last_hub_id: None,
            last_scanned_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn no_discrepancy_when_actual_not_set() {
        let piece = make_piece(25_000);
        assert!(!piece.has_weight_discrepancy());
        assert_eq!(piece.weight_delta_grams(), 0);
    }

    #[test]
    fn discrepancy_detected_when_actual_heavier() {
        let mut piece = make_piece(25_000);
        piece.actual_weight = Some(ShipmentWeight::from_grams(32_000));
        assert!(piece.has_weight_discrepancy());
        assert_eq!(piece.weight_delta_grams(), 7_000);
    }

    #[test]
    fn no_discrepancy_when_actual_equal() {
        let mut piece = make_piece(25_000);
        piece.actual_weight = Some(ShipmentWeight::from_grams(25_000));
        assert!(!piece.has_weight_discrepancy());
    }

    #[test]
    fn billable_weight_uses_volumetric_when_larger() {
        let mut piece = make_piece(1_000);
        piece.dimensions = Some(ShipmentDimensions { length_cm: 40, width_cm: 40, height_cm: 40 });
        // vol = 40*40*40 / 5 = 12_800 grams
        assert_eq!(piece.billable_weight_grams(), 12_800);
    }

    #[test]
    fn billable_weight_uses_actual_when_larger_than_volumetric() {
        let mut piece = make_piece(1_000);
        piece.actual_weight = Some(ShipmentWeight::from_grams(20_000));
        piece.dimensions = Some(ShipmentDimensions { length_cm: 10, width_cm: 10, height_cm: 10 });
        // vol = 1000/5 = 200g — actual 20kg wins
        assert_eq!(piece.billable_weight_grams(), 20_000);
    }
}
