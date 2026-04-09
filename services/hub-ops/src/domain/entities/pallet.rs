use chrono::{DateTime, Utc};
use logisticos_types::{awb::ChildAwb, HubId, PalletId, PalletStatus, TenantId};
use serde::{Deserialize, Serialize};

/// A consolidated physical unit at a hub — multiple pieces stacked and wrapped together.
///
/// A pallet can contain pieces from **different shipments/merchants**; it is a
/// purely physical grouping for linehaul efficiency.  Billing is always at the
/// piece/AWB level — never at the pallet level.
///
/// Lifecycle: Open → Sealed → Loaded → InTransit → Arrived → Broken
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pallet {
    pub id:               PalletId,
    pub tenant_id:        TenantId,
    /// Hub where this pallet was created and sealed.
    pub origin_hub_id:    HubId,
    /// Hub this pallet is destined for (None = local last-mile from same hub).
    pub destination_hub:  Option<HubId>,
    /// All piece AWBs loaded onto this pallet (cross-shipment mix allowed).
    pub pieces:           Vec<ChildAwb>,
    /// Total declared weight of all pieces (grams). Updated as pieces are added.
    pub total_weight_grams: u32,
    pub status:           PalletStatus,
    pub created_at:       DateTime<Utc>,
    pub sealed_at:        Option<DateTime<Utc>>,
    pub sealed_by:        Option<uuid::Uuid>,
    pub updated_at:       DateTime<Utc>,
}

impl Pallet {
    pub fn new(tenant_id: TenantId, origin_hub_id: HubId, destination_hub: Option<HubId>) -> Self {
        let now = Utc::now();
        Self {
            id: PalletId::new(),
            tenant_id,
            origin_hub_id,
            destination_hub,
            pieces: Vec::new(),
            total_weight_grams: 0,
            status: PalletStatus::Open,
            created_at: now,
            sealed_at: None,
            sealed_by: None,
            updated_at: now,
        }
    }

    /// Add a piece to this pallet.
    /// Business rule: cannot add pieces to a sealed pallet.
    pub fn add_piece(&mut self, piece_awb: ChildAwb, weight_grams: u32) -> Result<(), PalletError> {
        if self.status != PalletStatus::Open {
            return Err(PalletError::NotOpen(format!("{:?}", self.status)));
        }
        if self.pieces.len() >= 999 {
            return Err(PalletError::PieceLimitReached);
        }
        self.total_weight_grams += weight_grams;
        self.pieces.push(piece_awb);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Seal the pallet — no more pieces can be added after this.
    /// Business rule: must have at least 1 piece.
    pub fn seal(&mut self, sealed_by: uuid::Uuid) -> Result<(), PalletError> {
        if self.status != PalletStatus::Open {
            return Err(PalletError::NotOpen(format!("{:?}", self.status)));
        }
        if self.pieces.is_empty() {
            return Err(PalletError::Empty);
        }
        let now = Utc::now();
        self.status    = PalletStatus::Sealed;
        self.sealed_at = Some(now);
        self.sealed_by = Some(sealed_by);
        self.updated_at = now;
        Ok(())
    }

    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    pub fn total_weight_kg(&self) -> f32 {
        self.total_weight_grams as f32 / 1000.0
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PalletError {
    #[error("Pallet is not open (current status: {0})")]
    NotOpen(String),

    #[error("Pallet is empty — must have at least one piece before sealing")]
    Empty,

    #[error("Pallet has reached maximum piece capacity (999)")]
    PieceLimitReached,
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::{Awb, ServiceCode, TenantCode};

    fn make_child(n: u16) -> ChildAwb {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        ChildAwb::new(&master, n).unwrap()
    }

    fn make_pallet() -> Pallet {
        Pallet::new(TenantId::new(), HubId::new(), None)
    }

    #[test]
    fn add_piece_increases_weight_and_count() {
        let mut p = make_pallet();
        p.add_piece(make_child(1), 25_000).unwrap();
        p.add_piece(make_child(2), 18_000).unwrap();
        assert_eq!(p.piece_count(), 2);
        assert_eq!(p.total_weight_grams, 43_000);
    }

    #[test]
    fn seal_transitions_to_sealed() {
        let mut p = make_pallet();
        p.add_piece(make_child(1), 10_000).unwrap();
        p.seal(uuid::Uuid::new_v4()).unwrap();
        assert_eq!(p.status, PalletStatus::Sealed);
        assert!(p.sealed_at.is_some());
    }

    #[test]
    fn cannot_add_piece_to_sealed_pallet() {
        let mut p = make_pallet();
        p.add_piece(make_child(1), 5_000).unwrap();
        p.seal(uuid::Uuid::new_v4()).unwrap();
        let err = p.add_piece(make_child(2), 5_000).unwrap_err();
        assert!(matches!(err, PalletError::NotOpen(_)));
    }

    #[test]
    fn cannot_seal_empty_pallet() {
        let mut p = make_pallet();
        assert_eq!(p.seal(uuid::Uuid::new_v4()).unwrap_err(), PalletError::Empty);
    }

    #[test]
    fn weight_in_kg() {
        let mut p = make_pallet();
        p.add_piece(make_child(1), 25_000).unwrap();
        assert!((p.total_weight_kg() - 25.0).abs() < 0.01);
    }
}
