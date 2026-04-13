//! HTTP client for the order-intake internal billing endpoint.
//!
//! Implements the `ShipmentBillingSource` driven port.
//! Calls `GET {ORDER_INTAKE_URL}/v1/internal/shipments/:id/billing`.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::repositories::{ShipmentBillingDto, ShipmentBillingSource};

pub struct OrderIntakeClient {
    base_url: String,
    http:     reqwest::Client,
}

impl OrderIntakeClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            http:     reqwest::Client::new(),
        }
    }
}

/// Wire shape returned by GET /v1/internal/shipments/:id/billing
#[derive(Debug, serde::Deserialize)]
struct BillingResponse {
    shipment_id:          Uuid,
    awb:                  String,
    merchant_id:          Uuid,
    currency:             String,
    base_freight:         i64,
    fuel_surcharge:       i64,
    insurance:            i64,
    total:                i64,
}

#[async_trait]
impl ShipmentBillingSource for OrderIntakeClient {
    async fn fetch(&self, shipment_id: Uuid) -> anyhow::Result<ShipmentBillingDto> {
        let url = format!(
            "{}/v1/internal/shipments/{}/billing",
            self.base_url.trim_end_matches('/'),
            shipment_id,
        );

        let resp = self.http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json::<BillingResponse>()
            .await?;

        Ok(ShipmentBillingDto {
            shipment_id:          resp.shipment_id,
            awb:                  resp.awb,
            merchant_id:          resp.merchant_id,
            currency:             resp.currency,
            base_freight_cents:   resp.base_freight,
            fuel_surcharge_cents: resp.fuel_surcharge,
            insurance_cents:      resp.insurance,
            total_cents:          resp.total,
        })
    }
}
