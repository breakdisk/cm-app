//! HTTP client for order-intake internal billing endpoints.
//!
//! Implements two driven ports:
//! - `ShipmentBillingSource` — `GET /v1/internal/shipments/:id/billing` (single AWB, receipts).
//! - `MerchantBillingSource` — `GET /v1/internal/billing/shipments`     (period query, aggregation).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::repositories::{
    BillingShipmentDto, MerchantBillingSource,
    ShipmentBillingDto, ShipmentBillingSource,
};

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

/// Wire shape from GET /v1/internal/billing/shipments (inner `shipments` array).
#[derive(Debug, serde::Deserialize)]
struct BillingListResponse {
    shipments: Vec<BillingShipmentWire>,
}

#[derive(Debug, serde::Deserialize)]
struct BillingShipmentWire {
    shipment_id:          Uuid,
    awb:                  String,
    merchant_id:          Uuid,
    currency:             String,
    base_freight_cents:   i64,
    fuel_surcharge_cents: i64,
    insurance_cents:      i64,
    total_cents:          i64,
    delivered_at:         DateTime<Utc>,
}

#[async_trait]
impl MerchantBillingSource for OrderIntakeClient {
    async fn list_delivered(
        &self,
        tenant_id:   Uuid,
        merchant_id: Uuid,
        from:        DateTime<Utc>,
        to:          DateTime<Utc>,
    ) -> anyhow::Result<Vec<BillingShipmentDto>> {
        let url = format!(
            "{}/v1/internal/billing/shipments",
            self.base_url.trim_end_matches('/'),
        );

        let resp = self.http
            .get(&url)
            .query(&[
                ("tenant_id",   tenant_id.to_string()),
                ("merchant_id", merchant_id.to_string()),
                ("from",        from.to_rfc3339()),
                ("to",          to.to_rfc3339()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<BillingListResponse>()
            .await?;

        Ok(resp.shipments.into_iter().map(|w| BillingShipmentDto {
            shipment_id:          w.shipment_id,
            awb:                  w.awb,
            merchant_id:          w.merchant_id,
            currency:             w.currency,
            base_freight_cents:   w.base_freight_cents,
            fuel_surcharge_cents: w.fuel_surcharge_cents,
            insurance_cents:      w.insurance_cents,
            total_cents:          w.total_cents,
            delivered_at:         w.delivered_at,
        }).collect())
    }
}
