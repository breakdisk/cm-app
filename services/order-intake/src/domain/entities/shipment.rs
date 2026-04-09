use crate::domain::value_objects::{ServiceType, ShipmentWeight, ShipmentDimensions};
use logisticos_types::{awb::Awb, ShipmentId, MerchantId, CustomerId, Money, Address, ShipmentStatus, TenantId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shipment {
    pub id: ShipmentId,
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub customer_id: CustomerId,
    pub customer_name: String,
    pub customer_phone: String,
    /// Master AWB — the structured, checksummed customer-facing tracking number.
    pub awb: Awb,
    /// Number of physical pieces (1..=999). Drives child AWB generation.
    pub piece_count: u16,
    pub status: ShipmentStatus,
    pub service_type: ServiceType,
    pub origin: Address,
    pub destination: Address,
    pub weight: ShipmentWeight,
    pub dimensions: Option<ShipmentDimensions>,
    pub declared_value: Option<Money>,
    pub cod_amount: Option<Money>,
    pub special_instructions: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Shipment {
    /// Business rule: COD amount must not exceed declared value
    pub fn validate_cod(&self) -> Result<(), &'static str> {
        if let (Some(cod), Some(declared)) = (&self.cod_amount, &self.declared_value) {
            if cod.amount > declared.amount {
                return Err("COD amount cannot exceed declared value");
            }
        }
        Ok(())
    }

    /// Business rule: Shipment can only be cancelled before pickup
    pub fn can_cancel(&self) -> bool {
        matches!(
            self.status,
            ShipmentStatus::Pending | ShipmentStatus::Confirmed
        )
    }

    /// Business rule: Reschedule only allowed on failed delivery
    pub fn can_reschedule(&self) -> bool {
        matches!(
            self.status,
            ShipmentStatus::DeliveryAttempted | ShipmentStatus::Failed
        )
    }

    /// Compute base delivery fee (simplified pricing logic)
    pub fn compute_base_fee(&self) -> Money {
        use logisticos_types::Currency;
        let base = match self.service_type {
            ServiceType::Standard  => 8500,  // PHP 85.00
            ServiceType::Express   => 15000, // PHP 150.00
            ServiceType::SameDay   => 20000, // PHP 200.00
            ServiceType::Balikbayan => 50000, // PHP 500.00
        };
        // Weight surcharge: +PHP 10 per 0.5kg over 1kg
        let weight_kg = self.weight.grams as f64 / 1000.0;
        let surcharge = if weight_kg > 1.0 {
            ((weight_kg - 1.0) / 0.5).ceil() as i64 * 1000
        } else {
            0
        };
        Money::new(base + surcharge, Currency::PHP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logisticos_types::{Address, Currency};

    fn make_address() -> Address {
        Address {
            line1:        "123 Test St".to_string(),
            line2:        None,
            barangay:     None,
            city:         "Manila".to_string(),
            province:     "Metro Manila".to_string(),
            postal_code:  "1000".to_string(),
            country_code: "PH".to_string(),
            coordinates:  None,
        }
    }

    fn make_awb() -> Awb {
        let tenant = logisticos_types::awb::TenantCode::new("PH1").unwrap();
        Awb::generate(&tenant, logisticos_types::awb::ServiceCode::Standard, 1)
    }

    #[test]
    fn shipment_has_customer_fields() {
        let s = Shipment {
            id:                   logisticos_types::ShipmentId::new(),
            tenant_id:            logisticos_types::TenantId::from_uuid(uuid::Uuid::new_v4()),
            merchant_id:          logisticos_types::MerchantId::from_uuid(uuid::Uuid::new_v4()),
            customer_id:          logisticos_types::CustomerId::new(),
            customer_name:        "Test Customer".to_string(),
            customer_phone:       "+63912345678".to_string(),
            awb:                  make_awb(),
            piece_count:          1,
            status:               logisticos_types::ShipmentStatus::Pending,
            service_type:         crate::domain::value_objects::ServiceType::Standard,
            origin:               make_address(),
            destination:          make_address(),
            weight:               crate::domain::value_objects::ShipmentWeight::from_grams(1000),
            dimensions:           None,
            declared_value:       None,
            cod_amount:           None,
            special_instructions: None,
            created_at:           chrono::Utc::now(),
            updated_at:           chrono::Utc::now(),
        };
        assert_eq!(s.customer_name,  "Test Customer");
        assert_eq!(s.customer_phone, "+63912345678");
        assert!(s.awb.is_valid());
    }

    #[test]
    fn compute_base_fee_standard_no_surcharge() {
        let s = Shipment {
            id:                   logisticos_types::ShipmentId::new(),
            tenant_id:            logisticos_types::TenantId::from_uuid(uuid::Uuid::new_v4()),
            merchant_id:          logisticos_types::MerchantId::from_uuid(uuid::Uuid::new_v4()),
            customer_id:          logisticos_types::CustomerId::new(),
            customer_name:        "Alice".to_string(),
            customer_phone:       "+63900000001".to_string(),
            awb:                  make_awb(),
            piece_count:          1,
            status:               logisticos_types::ShipmentStatus::Pending,
            service_type:         crate::domain::value_objects::ServiceType::Standard,
            origin:               make_address(),
            destination:          make_address(),
            weight:               crate::domain::value_objects::ShipmentWeight::from_grams(500),
            dimensions:           None,
            declared_value:       None,
            cod_amount:           None,
            special_instructions: None,
            created_at:           chrono::Utc::now(),
            updated_at:           chrono::Utc::now(),
        };
        let fee = s.compute_base_fee();
        assert_eq!(fee, logisticos_types::Money::new(8500, Currency::PHP));
    }
}
