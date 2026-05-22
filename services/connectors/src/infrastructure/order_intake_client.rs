//! HTTP client for calling the order-intake service's internal (mTLS-only) endpoint.

use logisticos_errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Payload sent to `POST /v1/internal/shipments` on the order-intake service.
/// Mirrors `InternalCreateShipmentCommand` in order-intake.
#[derive(Debug, Serialize)]
pub struct InternalCreateShipmentRequest {
    pub tenant_id:         Uuid,
    pub merchant_id:       Uuid,
    pub tenant_slug:       String,
    pub source_platform:   String,
    pub external_order_id: Option<String>,

    pub customer_name:    String,
    pub customer_phone:   String,
    pub customer_email:   Option<String>,

    pub origin:      AddressPayload,
    pub destination: AddressPayload,

    pub service_type:     String,
    pub weight_grams:     u32,
    pub length_cm:        Option<u32>,
    pub width_cm:         Option<u32>,
    pub height_cm:        Option<u32>,

    pub declared_value_cents: Option<i64>,
    pub cod_amount_cents:     Option<i64>,
    pub merchant_reference:   Option<String>,
    pub description:          Option<String>,
    pub special_instructions: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddressPayload {
    pub line1:        String,
    pub line2:        Option<String>,
    pub barangay:     Option<String>,
    pub city:         String,
    pub province:     String,
    pub postal_code:  String,
    pub country_code: String,
}

/// Successful response from `POST /v1/internal/shipments`.
#[derive(Debug, Deserialize)]
pub struct CreatedShipmentResponse {
    pub id:  Uuid,
    pub awb: String,
}

pub struct OrderIntakeClient {
    client:   reqwest::Client,
    base_url: String,
}

impl OrderIntakeClient {
    pub fn new(base_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build OrderIntakeClient");
        Self { client, base_url }
    }

    /// Create a shipment via the internal order-intake endpoint.
    pub async fn create_shipment(
        &self,
        req: &InternalCreateShipmentRequest,
    ) -> AppResult<CreatedShipmentResponse> {
        let url = format!("{}/v1/internal/shipments", self.base_url.trim_end_matches('/'));

        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| AppError::ExternalService {
                service: "order-intake".into(),
                message: e.to_string(),
            })?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::ExternalService {
                service: "order-intake".into(),
                message: format!("HTTP {status}: {body}"),
            });
        }

        resp.json::<CreatedShipmentResponse>().await.map_err(|e| AppError::ExternalService {
            service: "order-intake".into(),
            message: format!("failed to parse response: {e}"),
        })
    }
}
