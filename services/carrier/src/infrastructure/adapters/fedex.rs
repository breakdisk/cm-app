/// FedEx REST API v1
/// Docs: https://developer.fedex.com/api/en-us/catalog/
/// Auth: OAuth2 client_credentials — token cached until expiry - 60 s.
/// Sandbox base URL: https://apis-sandbox.fedex.com
/// Production base URL: https://apis.fedex.com

use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use super::{
    AdapterError, Address, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingEvent, TrackingInfo,
};

struct TokenCache {
    token:   String,
    expires: Instant,
}

pub struct FedexAdapter {
    http:           Client,
    base_url:       String,
    client_id:      String,
    client_secret:  String,
    account_number: String,
    token_cache:    Mutex<Option<TokenCache>>,
}

impl FedexAdapter {
    pub fn new(
        base_url: String,
        client_id: String,
        client_secret: String,
        account_number: String,
    ) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("LogisticOS-CarrierGateway/1.0")
            .build()
            .expect("FedEx HTTP client");
        Self {
            http,
            base_url,
            client_id,
            client_secret,
            account_number,
            token_cache: Mutex::new(None),
        }
    }

    async fn bearer_token(&self) -> Result<String, AdapterError> {
        // Return cached token if still valid (with 60-second buffer)
        {
            let cache = self.token_cache.lock().unwrap();
            if let Some(ref c) = *cache {
                if c.expires > Instant::now() {
                    return Ok(c.token.clone());
                }
            }
        }

        #[derive(Deserialize)]
        struct TokenResp { access_token: String, expires_in: u64 }

        let resp = self.http
            .post(format!("{}/oauth/token", self.base_url))
            .form(&[
                ("grant_type",    "client_credentials"),
                ("client_id",     &self.client_id),
                ("client_secret", &self.client_secret),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body   = resp.text().await.unwrap_or_default();
            return Err(AdapterError::Auth(format!("FedEx OAuth {status}: {body}")));
        }

        let tr: TokenResp = resp.json().await?;
        let token  = tr.access_token.clone();
        let expiry = Instant::now() + Duration::from_secs(tr.expires_in.saturating_sub(60));
        *self.token_cache.lock().unwrap() = Some(TokenCache { token: tr.access_token, expires: expiry });
        Ok(token)
    }

    fn map_address(addr: &Address) -> Value {
        let mut lines = vec![addr.street1.clone()];
        if let Some(ref s2) = addr.street2 { lines.push(s2.clone()); }
        json!({
            "contact": {
                "personName":   addr.name,
                "companyName":  addr.company,
                "phoneNumber":  addr.phone.clone().unwrap_or_default(),
                "emailAddress": addr.email,
            },
            "address": {
                "streetLines":          lines,
                "city":                 addr.city,
                "stateOrProvinceCode":  addr.state.clone().unwrap_or_default(),
                "postalCode":           addr.postal_code,
                "countryCode":          addr.country,
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
impl CarrierAdapter for FedexAdapter {
    fn carrier_code(&self) -> &'static str { "FEDEX" }
    fn carrier_name(&self) -> &'static str { "FedEx" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "accountNumber": { "value": self.account_number },
            "requestedShipment": {
                "shipper":   Self::map_address(&req.origin),
                "recipient": Self::map_address(&req.destination),
                "serviceType": req.service_type,
                "requestedPackageLineItems": [{
                    "weight": { "value": req.weight_kg, "units": "KG" },
                    "dimensions": {
                        "length": req.length_cm.unwrap_or(30.0) as u32,
                        "width":  req.width_cm.unwrap_or(20.0) as u32,
                        "height": req.height_cm.unwrap_or(10.0) as u32,
                        "units":  "CM",
                    },
                }],
                "rateRequestType": ["ACCOUNT", "LIST"],
            }
        });

        let resp = self.http
            .post(format!("{}/rate/v1/rates/quotes", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        let data    = Self::check_error(resp).await?;
        let mut quotes = vec![];

        for d in data["output"]["rateReplyDetails"].as_array().unwrap_or(&vec![]) {
            let price_cents = d["ratedShipmentDetails"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|s| s["totalNetFedExCharge"].as_f64())
                .map(|f| (f * 100.0) as i64)
                .unwrap_or(0);

            quotes.push(RateQuote {
                carrier_code: "FEDEX".into(),
                service_code: d["serviceType"].as_str().unwrap_or("").into(),
                service_name: d["serviceName"].as_str().unwrap_or("").into(),
                price_cents,
                currency:     req.currency.clone(),
                transit_days: d["commit"]["dateDetail"]["dayOfWeek"]
                    .as_u64()
                    .map(|d| d as u32),
                valid_until:  None,
            });
        }

        Ok(quotes)
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "labelResponseOptions": "URL_ONLY",
            "requestedShipment": {
                "shipper":     Self::map_address(&req.shipper),
                "recipients":  [Self::map_address(&req.consignee)],
                "serviceType": req.service_code,
                "packagingType": "YOUR_PACKAGING",
                "requestedPackageLineItems": [{
                    "weight": { "value": req.weight_kg, "units": "KG" },
                    "dimensions": {
                        "length": req.length_cm.unwrap_or(30.0) as u32,
                        "width":  req.width_cm.unwrap_or(20.0) as u32,
                        "height": req.height_cm.unwrap_or(10.0) as u32,
                        "units":  "CM",
                    },
                    "customerReferences": [{
                        "customerReferenceType": "CUSTOMER_REFERENCE",
                        "value": req.reference.clone().unwrap_or_default(),
                    }],
                }],
                "customsClearanceDetail": {
                    "dutiesPayment": { "paymentType": "SENDER" },
                    "commodities": [{
                        "description": req.description,
                        "quantity": 1,
                        "weight": { "value": req.weight_kg, "units": "KG" },
                        "unitPrice":    { "amount": req.declared_value_cents as f64 / 100.0, "currency": req.currency },
                        "customsValue": { "amount": req.declared_value_cents as f64 / 100.0, "currency": req.currency },
                    }],
                },
            },
            "accountNumber": { "value": self.account_number },
        });

        let resp = self.http
            .post(format!("{}/ship/v1/shipments", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        let data  = Self::check_error(resp).await?;
        let pkg   = &data["output"]["transactionShipments"][0];
        let tnum  = pkg["masterTrackingNumber"].as_str().unwrap_or("").to_string();
        let label = pkg["pieceResponses"][0]["packageDocuments"][0]["url"]
            .as_str()
            .map(String::from);

        Ok(BookingConfirmation {
            booking_ref:        tnum.clone(),
            tracking_number:    tnum,
            label_url:          label,
            label_bytes:        None,
            estimated_delivery: pkg["operationalDetail"]["deliveryDate"]
                .as_str()
                .map(String::from),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "includeDetailedScans": true,
            "trackingInfo": [{ "trackingNumberInfo": { "trackingNumber": tracking_number } }],
        });

        let resp = self.http
            .post(format!("{}/track/v1/trackingnumbers", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        let data   = Self::check_error(resp).await?;
        let result = &data["output"]["completeTrackResults"][0]["trackResults"][0];

        let events: Vec<TrackingEvent> = result["scanEvents"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|e| TrackingEvent {
                timestamp:   e["date"].as_str().unwrap_or("").into(),
                status:      e["eventType"].as_str().unwrap_or("").into(),
                description: e["eventDescription"].as_str().unwrap_or("").into(),
                location:    e["scanLocation"]["city"].as_str().map(String::from),
            })
            .collect();

        Ok(TrackingInfo {
            tracking_number:    tracking_number.into(),
            current_status:     result["latestStatusDetail"]["statusByLocale"]
                .as_str()
                .unwrap_or("UNKNOWN")
                .into(),
            current_location:   result["latestStatusDetail"]["scanLocation"]["city"]
                .as_str()
                .map(String::from),
            estimated_delivery: result["estimatedDeliveryTimeWindow"]["window"]["begins"]
                .as_str()
                .map(String::from),
            events,
        })
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "accountNumber":  { "value": self.account_number },
            "trackingNumber": booking_ref,
        });

        let resp = self.http
            .put(format!("{}/ship/v1/shipments/cancel", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        if resp.status().is_success() {
            Ok(())
        } else {
            let status = resp.status().as_u16();
            let body   = resp.text().await.unwrap_or_default();
            Err(AdapterError::CarrierError { status, body })
        }
    }

    async fn get_label(&self, booking_ref: &str, _format: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({ "trackingNumber": booking_ref, "imageType": "PDF" });

        let resp = self.http
            .post(format!("{}/ship/v1/shipments/packages/labels", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body   = resp.text().await.unwrap_or_default();
            return Err(AdapterError::CarrierError { status, body });
        }

        Ok(resp.bytes().await?.to_vec())
    }
}
