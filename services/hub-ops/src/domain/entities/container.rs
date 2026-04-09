use chrono::{DateTime, Utc};
use logisticos_types::{
    awb::{Awb, ChildAwb},
    ContainerId, HubId, PalletId, TenantId,
    ContainerStatus, TransportMode,
};
use serde::{Deserialize, Serialize};

/// A transport unit that carries pallets and/or loose pieces between hubs.
///
/// Maps to: road truck, sea container (FCL/LCL), or air ULD.
///
/// A container is invisible to merchants and end-customers — billing is always
/// at the AWB/piece level.  Container costs are absorbed into base freight rates
/// and fuel surcharges.
///
/// Lifecycle: Planning → Manifested → Loading → Sealed → InTransit
///            → ArrivedAtPort (sea/air) → Customs (international)
///            → Released → Delivered
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Container {
    pub id:               ContainerId,
    pub tenant_id:        TenantId,
    pub transport_mode:   TransportMode,
    /// External carrier's reference (e.g. bill of lading number, MAWB).
    pub carrier_ref:      Option<String>,
    pub origin_hub_id:    HubId,
    pub destination_hub:  HubId,
    /// Sealed pallets loaded into this container.
    pub pallets:          Vec<PalletId>,
    /// Pieces loaded directly (not on a pallet) — oversized or express single-pieces.
    pub loose_pieces:     Vec<ChildAwb>,
    /// All master AWBs in this container — denormalized for bulk status updates.
    pub master_awbs:      Vec<Awb>,
    pub status:           ContainerStatus,
    pub departed_at:      Option<DateTime<Utc>>,
    pub estimated_arrival: Option<DateTime<Utc>>,
    pub arrived_at:       Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}

impl Container {
    pub fn new(
        tenant_id:       TenantId,
        transport_mode:  TransportMode,
        origin_hub_id:   HubId,
        destination_hub: HubId,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ContainerId::new(),
            tenant_id,
            transport_mode,
            carrier_ref: None,
            origin_hub_id,
            destination_hub,
            pallets: Vec::new(),
            loose_pieces: Vec::new(),
            master_awbs: Vec::new(),
            status: ContainerStatus::Planning,
            departed_at: None,
            estimated_arrival: None,
            arrived_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Load a sealed pallet into this container.
    pub fn load_pallet(&mut self, pallet_id: PalletId, pallet_awbs: Vec<Awb>) -> Result<(), ContainerError> {
        self.require_mutable()?;
        if self.pallets.contains(&pallet_id) {
            return Err(ContainerError::AlreadyLoaded);
        }
        self.pallets.push(pallet_id);
        for awb in pallet_awbs {
            if !self.master_awbs.contains(&awb) {
                self.master_awbs.push(awb);
            }
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Load a piece directly (loose, not on a pallet).
    pub fn load_loose_piece(&mut self, piece_awb: ChildAwb, master_awb: Awb) -> Result<(), ContainerError> {
        self.require_mutable()?;
        self.loose_pieces.push(piece_awb);
        if !self.master_awbs.contains(&master_awb) {
            self.master_awbs.push(master_awb);
        }
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Finalise the manifest — no more loading after this.
    pub fn finalise_manifest(&mut self) -> Result<(), ContainerError> {
        if self.status != ContainerStatus::Planning {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Manifested".to_string(),
            });
        }
        if self.pallets.is_empty() && self.loose_pieces.is_empty() {
            return Err(ContainerError::Empty);
        }
        self.status     = ContainerStatus::Manifested;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Record departure — triggers `ContainerDeparted` Kafka event in the handler.
    pub fn depart(&mut self, eta: Option<DateTime<Utc>>) -> Result<(), ContainerError> {
        if !matches!(self.status, ContainerStatus::Manifested | ContainerStatus::Sealed) {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "InTransit".to_string(),
            });
        }
        let now = Utc::now();
        self.status           = ContainerStatus::InTransit;
        self.departed_at      = Some(now);
        self.estimated_arrival = eta;
        self.updated_at       = now;
        Ok(())
    }

    /// Record arrival at destination hub.
    pub fn arrive(&mut self) -> Result<(), ContainerError> {
        if self.status != ContainerStatus::InTransit {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Delivered".to_string(),
            });
        }
        let now = Utc::now();
        self.status     = ContainerStatus::Delivered;
        self.arrived_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    pub fn pallet_count(&self) -> usize    { self.pallets.len() }
    pub fn loose_piece_count(&self) -> usize { self.loose_pieces.len() }

    fn require_mutable(&self) -> Result<(), ContainerError> {
        if matches!(self.status, ContainerStatus::InTransit | ContainerStatus::Delivered | ContainerStatus::Customs | ContainerStatus::Released) {
            return Err(ContainerError::AlreadyDeparted);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ContainerError {
    #[error("Container has already departed and cannot be modified")]
    AlreadyDeparted,

    #[error("This pallet or piece is already loaded in this container")]
    AlreadyLoaded,

    #[error("Container is empty — must load at least one pallet or piece before manifesting")]
    Empty,

    #[error("Invalid status transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::{Awb, ChildAwb, ServiceCode, TenantCode};

    fn make_awb(seq: u32) -> Awb {
        let t = TenantCode::new("PH1").unwrap();
        Awb::generate(&t, ServiceCode::Balikbayan, seq)
    }

    fn make_child(master: &Awb, n: u16) -> ChildAwb {
        ChildAwb::new(master, n).unwrap()
    }

    fn make_container() -> Container {
        Container::new(TenantId::new(), TransportMode::Road, HubId::new(), HubId::new())
    }

    #[test]
    fn load_pallet_adds_master_awbs() {
        let mut c = make_container();
        let awbs  = vec![make_awb(1), make_awb(2)];
        c.load_pallet(PalletId::new(), awbs).unwrap();
        assert_eq!(c.pallet_count(), 1);
        assert_eq!(c.master_awbs.len(), 2);
    }

    #[test]
    fn load_loose_piece_deduplicates_master_awbs() {
        let mut c  = make_container();
        let master = make_awb(9);
        c.load_loose_piece(make_child(&master, 1), master.clone()).unwrap();
        c.load_loose_piece(make_child(&master, 2), master.clone()).unwrap();
        assert_eq!(c.loose_piece_count(), 2);
        assert_eq!(c.master_awbs.len(), 1, "same master should not be duplicated");
    }

    #[test]
    fn depart_transitions_to_in_transit() {
        let mut c = make_container();
        c.load_pallet(PalletId::new(), vec![make_awb(1)]).unwrap();
        c.finalise_manifest().unwrap();
        c.depart(None).unwrap();
        assert_eq!(c.status, ContainerStatus::InTransit);
        assert!(c.departed_at.is_some());
    }

    #[test]
    fn arrive_transitions_to_delivered() {
        let mut c = make_container();
        c.load_pallet(PalletId::new(), vec![make_awb(1)]).unwrap();
        c.finalise_manifest().unwrap();
        c.depart(None).unwrap();
        c.arrive().unwrap();
        assert_eq!(c.status, ContainerStatus::Delivered);
        assert!(c.arrived_at.is_some());
    }

    #[test]
    fn cannot_load_after_departure() {
        let mut c = make_container();
        c.load_pallet(PalletId::new(), vec![make_awb(1)]).unwrap();
        c.finalise_manifest().unwrap();
        c.depart(None).unwrap();
        let err = c.load_pallet(PalletId::new(), vec![make_awb(2)]).unwrap_err();
        assert_eq!(err, ContainerError::AlreadyDeparted);
    }

    #[test]
    fn cannot_manifest_empty_container() {
        let mut c = make_container();
        assert_eq!(c.finalise_manifest().unwrap_err(), ContainerError::Empty);
    }
}
