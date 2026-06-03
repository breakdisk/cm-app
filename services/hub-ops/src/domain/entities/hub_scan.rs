use chrono::{DateTime, Utc};
use logisticos_types::{
    awb::{Awb, ChildAwb},
    ContainerId, HubId, PalletId, ShipmentId, TenantId,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What kind of barcode read produced this scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanType {
    /// Piece arrives at hub from a first-mile driver.
    InboundReceive,
    /// Piece scanned onto a pallet.
    PalletAssign,
    /// Pallet or piece loaded into a container/vehicle.
    OutboundLoad,
    /// Piece broken out of a container at the destination hub.
    ContainerDeconsolidate,
    /// Piece scanned into a last-mile delivery cage/bin.
    LocalSortAssign,
    /// Damaged, missing, weight mismatch.
    ExceptionFlag,
}

/// Exception annotation captured at scan time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanException {
    Missing,
    Damaged,
    WeightMismatch,
}

/// Append-only chain-of-custody scan log.
///
/// Every barcode read at a hub creates exactly one immutable row — never updated
/// or deleted (enforced at the DB layer via `REVOKE UPDATE, DELETE`). Carries the
/// dual-timestamp contract: `device_timestamp` (hardware clock at scan moment,
/// the SLA basis) and `server_timestamp` (backend receipt time).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubScan {
    pub id:               Uuid,
    pub tenant_id:        TenantId,
    pub hub_id:           HubId,
    pub piece_awb:        ChildAwb,
    pub master_awb:       Awb,
    pub shipment_id:      ShipmentId,
    pub scan_type:        ScanType,
    /// Hub agent or driver user_id who performed the scan.
    pub scanned_by:       Uuid,
    /// Hardware clock at the physical scan moment (SLA basis).
    pub device_timestamp: DateTime<Utc>,
    /// Backend receipt time.
    pub server_timestamp: DateTime<Utc>,
    pub pallet_id:        Option<PalletId>,
    pub container_id:     Option<ContainerId>,
    pub exception:        Option<ScanException>,
}

impl HubScan {
    /// Record a new scan. `server_timestamp` is stamped now; `device_timestamp`
    /// is supplied by the caller from the hardware clock at scan moment.
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        tenant_id:        TenantId,
        hub_id:           HubId,
        piece_awb:        ChildAwb,
        master_awb:       Awb,
        shipment_id:      ShipmentId,
        scan_type:        ScanType,
        scanned_by:       Uuid,
        device_timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            hub_id,
            piece_awb,
            master_awb,
            shipment_id,
            scan_type,
            scanned_by,
            device_timestamp,
            server_timestamp: Utc::now(),
            pallet_id: None,
            container_id: None,
            exception: None,
        }
    }

    pub fn with_pallet(mut self, pallet_id: PalletId) -> Self {
        self.pallet_id = Some(pallet_id);
        self
    }

    pub fn with_container(mut self, container_id: ContainerId) -> Self {
        self.container_id = Some(container_id);
        self
    }

    /// Flag an exception. Forces `scan_type` to `ExceptionFlag` so the scan is
    /// visible in exception reporting regardless of the originating action.
    pub fn with_exception(mut self, exception: ScanException) -> Self {
        self.exception  = Some(exception);
        self.scan_type  = ScanType::ExceptionFlag;
        self
    }

    /// Time basis for SLA / transit-velocity — the device clock at scan moment.
    pub fn effective_time(&self) -> DateTime<Utc> {
        self.device_timestamp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::awb::{ServiceCode, TenantCode};

    fn fixture(scan_type: ScanType) -> HubScan {
        let tenant = TenantCode::new("PH1").unwrap();
        let master = Awb::generate(&tenant, ServiceCode::Balikbayan, 9012);
        let piece  = ChildAwb::new(&master, 1).unwrap();
        HubScan::record(
            TenantId::new(),
            HubId::new(),
            piece,
            master,
            ShipmentId::new(),
            scan_type,
            Uuid::new_v4(),
            Utc::now(),
        )
    }

    #[test]
    fn record_stamps_server_timestamp_and_no_exception() {
        let scan = fixture(ScanType::InboundReceive);
        assert!(scan.exception.is_none());
        assert!(scan.pallet_id.is_none());
        assert_eq!(scan.scan_type, ScanType::InboundReceive);
        // server_timestamp is stamped at/after the device timestamp captured in the fixture
        assert!(scan.server_timestamp >= scan.device_timestamp);
    }

    #[test]
    fn effective_time_uses_device_clock() {
        let scan = fixture(ScanType::OutboundLoad);
        assert_eq!(scan.effective_time(), scan.device_timestamp);
    }

    #[test]
    fn with_exception_forces_exception_flag_scan_type() {
        let scan = fixture(ScanType::InboundReceive).with_exception(ScanException::Damaged);
        assert_eq!(scan.scan_type, ScanType::ExceptionFlag);
        assert_eq!(scan.exception, Some(ScanException::Damaged));
    }

    #[test]
    fn builder_attaches_pallet_and_container() {
        let scan = fixture(ScanType::OutboundLoad)
            .with_pallet(PalletId::new())
            .with_container(ContainerId::new());
        assert!(scan.pallet_id.is_some());
        assert!(scan.container_id.is_some());
    }

    #[test]
    fn scan_type_serialises_snake_case() {
        let json = serde_json::to_string(&ScanType::ContainerDeconsolidate).unwrap();
        assert_eq!(json, "\"container_deconsolidate\"");
    }
}
