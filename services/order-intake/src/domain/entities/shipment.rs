use crate::domain::value_objects::{ServiceType, ShipmentWeight, ShipmentDimensions};
use logisticos_types::{ShipmentId, MerchantId, CustomerId, Money, Address, ShipmentStatus, TenantId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shipment {
    pub id: ShipmentId,
    pub tenant_id: TenantId,
    pub merchant_id: MerchantId,
    pub customer_id: CustomerId,
    pub tracking_number: String,
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
