/// UPS REST API
/// Docs: https://developer.ups.com/catalog
/// Auth: OAuth2 client_credentials — token cached until expiry - 60 s.
/// Sandbox base URL: https://wwwcie.ups.com/api
/// Production base URL: https://onlinetools.ups.com/api

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

pub struct UpsAdapter {
    http:           Client,
    base_url:       String,
    client_id:      String,
    client_secret:  String,
    account_number: String,
    token_cache:    Mutex<Option<TokenCache>>,
}

impl UpsAdapter {
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
            .expect("UPS HTTP client");
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
            .post(format!("{}/security/v1/oauth/token", self.base_url))
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
            return Err(AdapterError::Auth(format!("UPS OAuth {status}: {body}")));
        }

        let tr: TokenResp = resp.json().await?;
        let token  = tr.access_token.clone();
        let expiry = Instant::now() + Duration::from_secs(tr.expires_in.saturating_sub(60));
        *self.token_cache.lock().unwrap() = Some(TokenCache { token: tr.access_token, expires: expiry });
        Ok(token)
    }

    fn map_address(addr: &Address) -> Value {
        json!({
            "Name": addr.company.as_deref().unwrap_or(&addr.name),
            "AttentionName": addr.name,
            "Phone": { "Number": addr.phone.clone().unwrap_or_default() },
            "Address": {
                "AddressLine":       [addr.street1.clone(), addr.street2.clone().unwrap_or_default()],
                "City":              addr.city,
                "StateProvinceCode": addr.state.clone().unwrap_or_default(),
                "PostalCode":        addr.postal_code,
                "CountryCode":       addr.country,
            },
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
impl CarrierAdapter for UpsAdapter {
    fn carrier_code(&self) -> &'static str { "UPS" }
    fn carrier_name(&self) -> &'static str { "UPS" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "RateRequest": {
                "Request": { "RequestOption": "Shop" },
                "Shipment": {
                    "Shipper": {
                        "ShipperNumber": self.account_number,
                        "Address": {
                            "AddressLine":  [req.origin.street1.clone()],
                            "City":         req.origin.city,
                            "PostalCode":   req.origin.postal_code,
                            "CountryCode":  req.origin.country,
                        }
                    },
                    "ShipTo": {
                        "Address": {
                            "AddressLine":  [req.destination.street1.clone()],
                            "City":         req.destination.city,
                            "PostalCode":   req.destination.postal_code,
                            "CountryCode":  req.destination.country,
                        }
                    },
                    "ShipFrom": {
                        "Address": {
                            "AddressLine":  [req.origin.street1.clone()],
                            "City":         req.origin.city,
                            "PostalCode":   req.origin.postal_code,
                            "CountryCode":  req.origin.country,
                        }
                    },
                    "Package": {
                        "PackagingType": { "Code": "02" },
                        "PackageWeight": {
                            "UnitOfMeasurement": { "Code": "KGS" },
                            "Weight": format!("{:.2}", req.weight_kg),
                        },
                        "Dimensions": {
                            "UnitOfMeasurement": { "Code": "CM" },
                            "Length": format!("{:.0}", req.length_cm.unwrap_or(30.0)),
                            "Width":  format!("{:.0}", req.width_cm.unwrap_or(20.0)),
                            "Height": format!("{:.0}", req.height_cm.unwrap_or(10.0)),
                        }
                    }
                }
            }
        });

        let resp = self.http
            .post(format!("{}/rating/v1/Shop", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        let data    = Self::check_error(resp).await?;
        let mut quotes = vec![];

        for service in data["RateResponse"]["RatedShipment"].as_array().unwrap_or(&vec![]) {
            let price_cents = service["TotalCharges"]["MonetaryValue"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .map(|f| (f * 100.0) as i64)
                .unwrap_or(0);

            quotes.push(RateQuote {
                carrier_code: "UPS".into(),
                service_code: service["Service"]["Code"].as_str().unwrap_or("").into(),
                service_name: service["Service"]["Description"].as_str().unwrap_or("UPS Service").into(),
                price_cents,
                currency:     req.currency.clone(),
                transit_days: service["GuaranteedDelivery"]["BusinessDaysInTransit"]
                    .as_str()
                    .and_then(|s| s.parse().ok()),
                valid_until:  None,
            });
        }

        Ok(quotes)
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        let token = self.bearer_token().await?;

        let body = json!({
            "ShipmentRequest": {
                "Request": { "RequestOption": "nonvalidate" },
                "Shipment": {
                    "Description": req.description,
                    "Shipper": {
                        "ShipperNumber": self.account_number,
                        "AttentionName": req.shipper.name,
                        "Address": {
                            "AddressLine":       [req.shipper.street1.clone()],
                            "City":              req.shipper.city,
                            "StateProvinceCode": req.shipper.state.clone().unwrap_or_default(),
                            "PostalCode":        req.shipper.postal_code,
                            "CountryCode":       req.shipper.country,
                        }
                    },
                    "ShipTo": Self::map_address(&req.consignee),
                    "ShipFrom": Self::map_address(&req.shipper),
                    "PaymentInformation": {
                        "ShipmentCharge": {
                            "Type": "01",
                            "BillShipper": { "AccountNumber": self.account_number }
                        }
                    },
                    "Service": { "Code": req.service_code },
                    "Package": {
                        "PackagingType": { "Code": "02" },
                        "PackageWeight": {
                            "UnitOfMeasurement": { "Code": "KGS" },
                            "Weight": format!("{:.2}", req.weight_kg),
                        },
                        "ReferenceNumber": { "Value": req.reference.clone().unwrap_or_default() },
                    }
                },
                "LabelSpecification": {
                    "LabelImageFormat": { "Code": "PDF" },
                    "LabelStockSize":   { "Height": "6", "Width": "4" }
                }
            }
        });

        let resp = self.http
            .post(format!("{}/shipments/v1/ship", self.base_url))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await?;

        let data   = Self::check_error(resp).await?;
        let result = &data["ShipmentResponse"]["ShipmentResults"];

        let tnum  = result["ShipmentIdentificationNumber"].as_str().unwrap_or("").to_string();
        let label = result["PackageResults"][0]["ShippingLabel"]["GraphicImage"]
            .as_str()
            .map(String::from);

        Ok(BookingConfirmation {
            booking_ref:        tnum.clone(),
            tracking_number:    tnum,
            label_url:          label,
            estimated_delivery: result["EstimatedDelivery"]["Date"].as_str().map(String::from),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let token = self.bearer_token().await?;

        let resp = self.http
            .get(format!("{}/track/v1/details/{}", self.base_url, tracking_number))
            .bearer_auth(&token)
            .query(&[("locale", "en_US"), ("returnSignature", "false")])
            .send()
            .await?;

        let data     = Self::check_error(resp).await?;
        let shipment = &data["trackResponse"]["shipment"][0];
        let pkg      = &shipment["package"][0];

        let events: Vec<TrackingEvent> = pkg["activity"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|e| TrackingEvent {
                timestamp:   format!(
                    "{} {}",
                    e["date"].as_str().unwrap_or(""),
                    e["time"].as_str().unwrap_or("")
                ),
                status:      e["status"]["type"].as_str().unwrap_or("").into(),
                description: e["status"]["description"].as_str().unwrap_or("").into(),
                location:    e["location"]["address"]["city"].as_str().map(String::from),
            })
            .collect();

        Ok(TrackingInfo {
            tracking_number:    tracking_number.into(),
            current_status:     pkg["currentStatus"]["description"]
                .as_str()
                .unwrap_or("UNKNOWN")
                .into(),
            current_location:   events.first().and_then(|e| e.location.clone()),
            estimated_delivery: pkg["deliveryDate"][0]["date"].as_str().map(String::from),
            events,
        })
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        let token = self.bearer_token().await?;

        let resp = self.http
            .delete(format!("{}/shipments/v1/void/cancel/{}", self.base_url, booking_ref))
            .bearer_auth(&token)
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
        // UPS returns label as base64 GraphicImage in the shipment response.
        // Re-fetch is not supported by default — label must be extracted at booking time
        // and stored in our carrier_bookings table.
        Err(AdapterError::CarrierError {
            status: 501,
            body:   format!(
                "UPS label re-fetch not supported. Extract GraphicImage from booking response \
                 stored for reference: {booking_ref}"
            ),
        })
    }
}
