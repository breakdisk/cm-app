use chrono::{DateTime, Utc};
use logisticos_types::{
    awb::{Awb, ChildAwb},
    ContainerId, HubId, PalletId, TenantId,
    ContainerStatus, TransportMode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    /// Linked consolidation plan for this container (set when plan is confirmed).
    pub truck_spec_id:    Option<Uuid>,
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
            truck_spec_id: None,
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

    /// Sea/air only: InTransit → ArrivedAtPort. Emits `hub.container.arrived_at_port`.
    pub fn arrive_at_port(&mut self) -> Result<(), ContainerError> {
        if self.transport_mode == TransportMode::Road {
            return Err(ContainerError::InvalidTransition {
                from: format!("Road mode ({:?})", self.status),
                to:   "ArrivedAtPort".to_string(),
            });
        }
        if self.status != ContainerStatus::InTransit {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "ArrivedAtPort".to_string(),
            });
        }
        let now = Utc::now();
        self.status     = ContainerStatus::ArrivedAtPort;
        self.arrived_at = Some(now);
        self.updated_at = now;
        Ok(())
    }

    /// ArrivedAtPort → Customs. Emits `hub.container.customs_hold`.
    pub fn enter_customs(&mut self) -> Result<(), ContainerError> {
        if self.status != ContainerStatus::ArrivedAtPort {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Customs".to_string(),
            });
        }
        self.status     = ContainerStatus::Customs;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Customs → Released. The `cleared_by` audit is recorded on the
    /// `HubTransferManifest` by the service layer (mandatory human gate).
    /// Emits `hub.container.customs_cleared`.
    pub fn clear_customs(&mut self) -> Result<(), ContainerError> {
        if self.status != ContainerStatus::Customs {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Released".to_string(),
            });
        }
        self.status     = ContainerStatus::Released;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Road mode only: InTransit → Released (skips ArrivedAtPort/Customs).
    /// Emits `hub.container.released_domestic`.
    pub fn release_domestic(&mut self) -> Result<(), ContainerError> {
        if self.transport_mode != TransportMode::Road {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?} mode", self.transport_mode),
                to:   "Released (domestic)".to_string(),
            });
        }
        if self.status != ContainerStatus::InTransit {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Released".to_string(),
            });
        }
        self.status     = ContainerStatus::Released;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Released → Deconsolidated (terminal). Returns all loose `ChildAwb`s for
    /// routing fan-out; pallet pieces are resolved by the service layer from
    /// pallet records. Emits `hub.container.deconsolidated`.
    pub fn deconsolidate(&mut self) -> Result<Vec<ChildAwb>, ContainerError> {
        if self.status != ContainerStatus::Released {
            return Err(ContainerError::InvalidTransition {
                from: format!("{:?}", self.status),
                to:   "Deconsolidated".to_string(),
            });
        }
        self.status     = ContainerStatus::Deconsolidated;
        self.updated_at = Utc::now();
        Ok(self.loose_pieces.clone())
    }

    pub fn pallet_count(&self) -> usize    { self.pallets.len() }
    pub fn loose_piece_count(&self) -> usize { self.loose_pieces.len() }

    fn require_mutable(&self) -> Result<(), ContainerError> {
        if matches!(
            self.status,
            ContainerStatus::InTransit
                | ContainerStatus::ArrivedAtPort
                | ContainerStatus::Customs
                | ContainerStatus::Released
                | ContainerStatus::Deconsolidated
                | ContainerStatus::Delivered
        ) {
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

    fn departed(mode: TransportMode) -> Container {
        let mut c = Container::new(TenantId::new(), mode, HubId::new(), HubId::new());
        c.load_pallet(PalletId::new(), vec![make_awb(1)]).unwrap();
        c.finalise_manifest().unwrap();
        c.depart(None).unwrap();
        c
    }

    #[test]
    fn cross_border_flow_in_transit_to_deconsolidated() {
        let mut c = Container::new(TenantId::new(), TransportMode::SeaFcl, HubId::new(), HubId::new());
        let master = make_awb(1);
        c.load_loose_piece(make_child(&master, 1), master.clone()).unwrap();
        c.finalise_manifest().unwrap();
        c.depart(None).unwrap();

        c.arrive_at_port().unwrap();
        assert_eq!(c.status, ContainerStatus::ArrivedAtPort);
        c.enter_customs().unwrap();
        assert_eq!(c.status, ContainerStatus::Customs);
        c.clear_customs().unwrap();
        assert_eq!(c.status, ContainerStatus::Released);

        let pieces = c.deconsolidate().unwrap();
        assert_eq!(c.status, ContainerStatus::Deconsolidated);
        assert_eq!(pieces.len(), 1);
    }

    #[test]
    fn domestic_road_flow_in_transit_to_deconsolidated() {
        let mut c = departed(TransportMode::Road);
        c.release_domestic().unwrap();
        assert_eq!(c.status, ContainerStatus::Released);
        c.deconsolidate().unwrap();
        assert_eq!(c.status, ContainerStatus::Deconsolidated);
    }

    #[test]
    fn arrive_at_port_rejects_road_mode() {
        let mut c = departed(TransportMode::Road);
        assert!(c.arrive_at_port().is_err());
    }

    #[test]
    fn release_domestic_rejects_sea_air_mode() {
        let mut c = departed(TransportMode::SeaFcl);
        assert!(c.release_domestic().is_err());
    }

    #[test]
    fn enter_customs_requires_arrived_at_port() {
        let mut c = departed(TransportMode::AirUld); // still InTransit
        assert!(c.enter_customs().is_err());
    }

    #[test]
    fn clear_customs_requires_customs_state() {
        let mut c = departed(TransportMode::SeaFcl);
        c.arrive_at_port().unwrap(); // ArrivedAtPort, not Customs
        assert!(c.clear_customs().is_err());
    }

    #[test]
    fn deconsolidate_requires_released() {
        let mut c = departed(TransportMode::Road); // InTransit
        assert!(c.deconsolidate().is_err());
    }
}
