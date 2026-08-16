use chrono::{DateTime, Utc};
use logisticos_types::{ContainerId, HubId, TenantId, TransportMode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Customs clearance state of a transfer manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomsStatus {
    /// Domestic road moves — no customs.
    NotRequired,
    /// Filed, awaiting inspection.
    Pending,
    /// Under customs hold.
    Hold,
    /// Approved and released.
    Cleared,
}

/// One per container — the customs/freight document. Holds the carrier booking
/// reference (MAWB / Bill of Lading), customs broker reference, duty amounts, and
/// the clearance audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubTransferManifest {
    pub id:                  Uuid,
    pub tenant_id:           TenantId,
    pub container_id:        ContainerId,
    pub origin_hub_id:       HubId,
    pub destination_hub_id:  HubId,
    pub transport_mode:      TransportMode,
    /// MAWB / Bill of Lading.
    pub carrier_booking_ref: Option<String>,
    /// Customs broker reference.
    pub customs_filing_ref:  Option<String>,
    pub customs_status:      CustomsStatus,
    pub duties_total_cents:  Option<i64>,
    /// Optional broker API pre-fill payload.
    pub broker_payload:      Option<serde_json::Value>,
    /// Hub agent who approved clearance (mandatory human gate).
    pub cleared_by:          Option<Uuid>,
    pub cleared_at:          Option<DateTime<Utc>>,
    pub created_at:          DateTime<Utc>,
    pub updated_at:          DateTime<Utc>,
}

impl HubTransferManifest {
    /// Create a manifest for a container. Road moves start `NotRequired`; sea/air
    /// moves start `Pending` (customs filing expected).
    pub fn new(
        tenant_id:          TenantId,
        container_id:       ContainerId,
        origin_hub_id:      HubId,
        destination_hub_id: HubId,
        transport_mode:     TransportMode,
    ) -> Self {
        let now = Utc::now();
        let customs_status = match transport_mode {
            TransportMode::Road => CustomsStatus::NotRequired,
            _ => CustomsStatus::Pending,
        };
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            container_id,
            origin_hub_id,
            destination_hub_id,
            transport_mode,
            carrier_booking_ref: None,
            customs_filing_ref: None,
            customs_status,
            duties_total_cents: None,
            broker_payload: None,
            cleared_by: None,
            cleared_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// Record the customs broker filing + duty estimate. Valid from `Pending`.
    pub fn file_customs(
        &mut self,
        filing_ref:    impl Into<String>,
        duties_cents:  Option<i64>,
        broker_payload: Option<serde_json::Value>,
    ) -> Result<(), ManifestError> {
        if self.customs_status != CustomsStatus::Pending {
            return Err(ManifestError::InvalidCustomsTransition {
                from: self.customs_status,
                to:   CustomsStatus::Pending,
            });
        }
        self.customs_filing_ref = Some(filing_ref.into());
        self.duties_total_cents = duties_cents;
        self.broker_payload     = broker_payload;
        self.updated_at         = Utc::now();
        Ok(())
    }

    /// Place under customs hold. Valid from `Pending`.
    pub fn hold(&mut self) -> Result<(), ManifestError> {
        if self.customs_status != CustomsStatus::Pending {
            return Err(ManifestError::InvalidCustomsTransition {
                from: self.customs_status,
                to:   CustomsStatus::Hold,
            });
        }
        self.customs_status = CustomsStatus::Hold;
        self.updated_at     = Utc::now();
        Ok(())
    }

    /// Approve clearance — the mandatory human gate. Valid from `Hold`; records
    /// `cleared_by` + `cleared_at`.
    pub fn clear(&mut self, cleared_by: Uuid) -> Result<(), ManifestError> {
        if self.customs_status != CustomsStatus::Hold {
            return Err(ManifestError::InvalidCustomsTransition {
                from: self.customs_status,
                to:   CustomsStatus::Cleared,
            });
        }
        let now = Utc::now();
        self.customs_status = CustomsStatus::Cleared;
        self.cleared_by     = Some(cleared_by);
        self.cleared_at     = Some(now);
        self.updated_at     = now;
        Ok(())
    }

    pub fn has_duties(&self) -> bool {
        self.duties_total_cents.map(|c| c > 0).unwrap_or(false)
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ManifestError {
    #[error("Invalid customs transition from {from:?} to {to:?}")]
    InvalidCustomsTransition { from: CustomsStatus, to: CustomsStatus },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sea_manifest() -> HubTransferManifest {
        HubTransferManifest::new(
            TenantId::new(),
            ContainerId::new(),
            HubId::new(),
            HubId::new(),
            TransportMode::SeaFcl,
        )
    }

    #[test]
    fn road_manifest_starts_not_required() {
        let m = HubTransferManifest::new(
            TenantId::new(), ContainerId::new(), HubId::new(), HubId::new(), TransportMode::Road,
        );
        assert_eq!(m.customs_status, CustomsStatus::NotRequired);
    }

    #[test]
    fn sea_manifest_starts_pending() {
        assert_eq!(sea_manifest().customs_status, CustomsStatus::Pending);
    }

    #[test]
    fn full_customs_happy_path_records_clearer() {
        let mut m = sea_manifest();
        m.file_customs("BRK-123", Some(15_000), None).unwrap();
        assert!(m.has_duties());
        m.hold().unwrap();
        assert_eq!(m.customs_status, CustomsStatus::Hold);
        let agent = Uuid::new_v4();
        m.clear(agent).unwrap();
        assert_eq!(m.customs_status, CustomsStatus::Cleared);
        assert_eq!(m.cleared_by, Some(agent));
        assert!(m.cleared_at.is_some());
    }

    #[test]
    fn cannot_clear_without_hold() {
        let mut m = sea_manifest(); // Pending, not Hold
        let err = m.clear(Uuid::new_v4()).unwrap_err();
        assert_eq!(
            err,
            ManifestError::InvalidCustomsTransition { from: CustomsStatus::Pending, to: CustomsStatus::Cleared }
        );
    }

    #[test]
    fn cannot_hold_a_road_manifest() {
        let mut m = HubTransferManifest::new(
            TenantId::new(), ContainerId::new(), HubId::new(), HubId::new(), TransportMode::Road,
        );
        assert!(m.hold().is_err());
    }
}
