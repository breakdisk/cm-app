/// DHL Express — MyDHL+ REST API
/// Docs: https://developer.dhl.com/api-reference/mydhlapiexpress
/// Auth: HTTP Basic (API key : API secret), per DHL developer portal credentials.
/// Sandbox base URL: https://api-sandbox.dhl.com
/// Production base URL: https://api.dhl.com

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::{
    AdapterError, Address, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingEvent, TrackingInfo,
};

pub struct DhlAdapter {
    http:           Client,
    base_url:       String,
    account_number: String,
    api_key:        String,
    api_secret:     String,
}

impl DhlAdapter {
    pub fn new(
        base_url: String,
        account_number: String,
        api_key: String,
        api_secret: String,
    ) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("LogisticOS-CarrierGateway/1.0")
            .build()
            .expect("DHL HTTP client");
        Self { http, base_url, account_number, api_key, api_secret }
    }

    fn auth_header(&self) -> String {
        let creds = STANDARD.encode(format!("{}:{}", self.api_key, self.api_secret));
        format!("Basic {creds}")
    }

    fn map_address(addr: &Address) -> Value {
        json!({
            "postalAddress": {
                "cityName":    addr.city,
                "countryCode": addr.country,
                "postalCode":  addr.postal_code,
                "addressLine1": addr.street1,
                "addressLine2": addr.street2,
            },
            "contactInformation": {
                "fullName":    addr.name,
                "companyName": addr.company,
                "phone":       addr.phone,
                "email":       addr.email,
            }
        })
    }

    async fn check_error(resp: reqwest::Response) -> Result<Value, AdapterError> {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AdapterError::CarrierError { status: status.as_u16(), body });
        }
        serde_json::from_str(&body).map_err(|e| AdapterError::Parse(e.to_string()))
    }
}

#[async_trait]
impl CarrierAdapter for DhlAdapter {
    fn carrier_code(&self) -> &'static str { "DHL" }
    fn carrier_name(&self) -> &'static str { "DHL Express" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let body = json!({
            "customerDetails": {
                "shipperDetails":  Self::map_address(&req.origin),
                "receiverDetails": Self::map_address(&req.destination),
            },
            "accounts": [{ "typeCode": "shipper", "number": self.account_number }],
            "packages": [{
                "weight": req.weight_kg,
                "dimensions": {
                    "length": req.length_cm.unwrap_or(30.0),
                    "width":  req.width_cm.unwrap_or(20.0),
                    "height": req.height_cm.unwrap_or(10.0),
                }
            }],
            "localProductCode": req.service_type,
            "requestAllValueAddedServices": false,
            "unitOfMeasurement": "metric",
        });

        let resp = self.http
            .post(format!("{}/mydhlapi/rates", self.base_url))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;

        let data = Self::check_error(resp).await?;
        let mut quotes = vec![];

        for p in data["products"].as_array().unwrap_or(&vec![]) {
            let price_cents = p["totalPrice"]
                .as_array()
                .and_then(|arr| arr.iter().find(|v| v["currencyType"] == "BILLC"))
                .and_then(|v| v["price"].as_f64())
                .map(|f| (f * 100.0) as i64)
                .unwrap_or(0);

            quotes.push(RateQuote {
                carrier_code: "DHL".into(),
                service_code: p["productCode"].as_str().unwrap_or("").into(),
                service_name: p["productName"].as_str().unwrap_or("").into(),
                price_cents,
                currency:     req.currency.clone(),
                transit_days: p["deliveryCapabilities"]["totalTransitDays"].as_u64().map(|d| d as u32),
                valid_until:  None,
            });
        }

        Ok(quotes)
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        let body = json!({
            "plannedShippingDateAndTime": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S+00:00").to_string(),
            "pickup": { "isRequested": false },
            "productCode": req.service_code,
            "accounts": [{ "typeCode": "shipper", "number": self.account_number }],
            "customerDetails": {
                "shipperDetails":  Self::map_address(&req.shipper),
                "receiverDetails": Self::map_address(&req.consignee),
            },
            "content": {
                "packages": [{
                    "weight": req.weight_kg,
                    "dimensions": {
                        "length": req.length_cm.unwrap_or(30.0),
                        "width":  req.width_cm.unwrap_or(20.0),
                        "height": req.height_cm.unwrap_or(10.0),
                    }
                }],
                "isCustomsDeclarable": false,
                "declaredValue":         req.declared_value_cents as f64 / 100.0,
                "declaredValueCurrency": req.currency,
                "description":           req.description,
                "incoterm":              "DAP",
                "unitOfMeasurement":     "metric",
            },
            "outputImageProperties": {
                "printerDPI":     300,
                "encodingFormat": "pdf",
                "imageOptions":   [{ "typeCode": "label", "templateName": "ECOM26_84_001" }],
            },
            "customerReferences": [{
                "value":    req.reference.clone().unwrap_or_default(),
                "typeCode": "CU",
            }],
        });

        let resp = self.http
            .post(format!("{}/mydhlapi/shipments", self.base_url))
            .header("Authorization", self.auth_header())
            .json(&body)
            .send()
            .await?;

        let data = Self::check_error(resp).await?;
        let tnum = data["shipmentTrackingNumber"].as_str().unwrap_or("").to_string();

        Ok(BookingConfirmation {
            booking_ref:        tnum.clone(),
            tracking_number:    tnum,
            label_url:          None,
            estimated_delivery: data["estimatedDeliveryDate"].as_str().map(String::from),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let resp = self.http
            .get(format!("{}/mydhlapi/shipments/{}/tracking", self.base_url, tracking_number))
            .header("Authorization", self.auth_header())
            .query(&[("trackingView", "all-checkpoints")])
            .send()
            .await?;

        let data     = Self::check_error(resp).await?;
        let shipment = &data["shipments"][0];

        let events: Vec<TrackingEvent> = shipment["events"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|e| TrackingEvent {
                timestamp:   e["timestamp"].as_str().unwrap_or("").into(),
                status:      e["typeCode"].as_str().unwrap_or("").into(),
                description: e["description"].as_str().unwrap_or("").into(),
                location:    e["location"]["address"]["addressLocality"].as_str().map(String::from),
            })
            .collect();

        Ok(TrackingInfo {
            tracking_number:    tracking_number.into(),
            current_status:     shipment["status"].as_str().unwrap_or("UNKNOWN").into(),
            current_location:   events.first().and_then(|e| e.location.clone()),
            estimated_delivery: shipment["estimatedDeliveryDate"].as_str().map(String::from),
            events,
        })
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        // DHL MyDHL+ does not expose a REST cancellation endpoint for most
        // account tiers.  Route to DHL support with the booking reference.
        tracing::warn!(
            booking_ref,
            carrier = "DHL",
            "DHL cancellation requires manual intervention — no REST endpoint available"
        );
        Err(AdapterError::CarrierError {
            status: 501,
            body:   format!(
                "DHL Express does not support API-level cancellation. \
                 Contact DHL support with booking reference: {booking_ref}"
            ),
        })
    }

    async fn get_label(&self, booking_ref: &str, _format: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        // DHL returns label bytes as base64 in `documentImages[].content` within
        // the shipment creation response.  Re-fetch via the documents endpoint
        // on accounts that have the Document Management service enabled.
        let resp = self.http
            .get(format!("{}/mydhlapi/shipments/{}/get-image", self.base_url, booking_ref))
            .header("Authorization", self.auth_header())
            .query(&[("imageFormat", "PDF"), ("typeCode", "label")])
            .send()
            .await?;

        let data = Self::check_error(resp).await?;
        let b64  = data["documents"][0]["content"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("DHL label content missing".into()))?;

        STANDARD.decode(b64).map_err(|e| AdapterError::Parse(e.to_string()))
    }
}
