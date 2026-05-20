/// DPD REST API
/// Docs: https://api.dpd.co.uk/reference  (UK reference; adjust base URL per market)
/// Auth: HTTP Basic (username : password) per DPD account.
/// DPD operates country-specific API hubs — set base_url per deployment region:
///   UK:  https://api.dpd.co.uk
///   DE:  https://api.dpd.de/services
///   FR:  https://api.dpd.fr
///   NL:  https://api.dpd.nl
/// The request/response schema is consistent across markets.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::{
    AdapterError, Address, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingEvent, TrackingInfo,
};

pub struct DpdAdapter {
    http:     Client,
    base_url: String,
    username: String,
    password: String,
    /// DPD account number assigned at registration
    account:  String,
}

impl DpdAdapter {
    pub fn new(base_url: String, username: String, password: String, account: String) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("LogisticOS-CarrierGateway/1.0")
            .build()
            .expect("DPD HTTP client");
        Self { http, base_url, username, password, account }
    }

    fn auth_header(&self) -> String {
        let creds = STANDARD.encode(format!("{}:{}", self.username, self.password));
        format!("Basic {creds}")
    }

    fn map_address(addr: &Address) -> Value {
        json!({
            "name":           addr.company.as_deref().unwrap_or(&addr.name),
            "contactName":    addr.name,
            "street":         addr.street1,
            "locality":       addr.street2,
            "town":           addr.city,
            "county":         addr.state,
            "postcode":       addr.postal_code,
            "countryCode":    addr.country,
            "countryName":    addr.country,
            "telephoneNumber": addr.phone,
            "email":          addr.email,
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
impl CarrierAdapter for DpdAdapter {
    fn carrier_code(&self) -> &'static str { "DPD" }
    fn carrier_name(&self) -> &'static str { "DPD" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let body = json!({
            "collectionOnDelivery": false,
            "invoice": false,
            "parcels": [{
                "weight":  req.weight_kg,
                "height":  req.height_cm.unwrap_or(10.0),
                "width":   req.width_cm.unwrap_or(20.0),
                "length":  req.length_cm.unwrap_or(30.0),
            }],
            "collectionDetails": {
                "contactDetails": Self::map_address(&req.origin),
                "address":        Self::map_address(&req.origin),
            },
            "deliveryDetails": {
                "contactDetails": Self::map_address(&req.destination),
                "address":        Self::map_address(&req.destination),
            },
        });

        let resp = self.http
            .post(format!("{}/shipping/rate", self.base_url))
            .header("Authorization", self.auth_header())
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?;

        let data    = Self::check_error(resp).await?;
        let mut quotes = vec![];

        for product in data["data"]["products"].as_array().unwrap_or(&vec![]) {
            let price_cents = product["price"]
                .as_f64()
                .map(|f| (f * 100.0) as i64)
                .unwrap_or(0);

            quotes.push(RateQuote {
                carrier_code: "DPD".into(),
                service_code: product["networkCode"].as_str().unwrap_or("").into(),
                service_name: product["productDescription"].as_str().unwrap_or("DPD Service").into(),
                price_cents,
                currency:     req.currency.clone(),
                transit_days: product["transit"].as_u64().map(|d| d as u32),
                valid_until:  None,
            });
        }

        Ok(quotes)
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        let body = json!({
            "jobId": null,
            "collectionOnDelivery": false,
            "invoice": false,
            "collectionDate": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            "consolidate": false,
            "consignment": [{
                "consignmentNumber": null,
                "consignmentRef":    req.reference,
                "parcels": [{
                    "packageNumber":  null,
                    "packageTyping":  null,
                    "weight":         req.weight_kg,
                    "height":         req.height_cm.unwrap_or(10.0),
                    "width":          req.width_cm.unwrap_or(20.0),
                    "length":         req.length_cm.unwrap_or(30.0),
                    "contents":       req.description,
                    "value":          req.declared_value_cents as f64 / 100.0,
                }],
                "collectionDetails": {
                    "contactDetails":    Self::map_address(&req.shipper),
                    "address":           Self::map_address(&req.shipper),
                },
                "deliveryDetails": {
                    "contactDetails":    Self::map_address(&req.consignee),
                    "address":           Self::map_address(&req.consignee),
                },
                "networkCode":           req.service_code,
                "numberOfParcels":       1,
                "totalWeight":           req.weight_kg,
                "shippingRef1":          req.reference,
                "shippingRef2":          null,
                "shippingRef3":          null,
                "customsValue":          null,
                "deliveryInstructions":  null,
                "parcelDescription":     req.description,
                "liabilityValue":        null,
                "liability":             false,
            }],
        });

        let resp = self.http
            .post(format!("{}/shipping/shipment", self.base_url))
            .header("Authorization", self.auth_header())
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?;

        let data     = Self::check_error(resp).await?;
        let consignment = &data["data"]["consignmentDetail"][0];
        let parcel      = &consignment["parcel"][0];
        let tnum = parcel["parcelNumber"].as_str().unwrap_or("").to_string();

        Ok(BookingConfirmation {
            booking_ref:        consignment["consignmentRef"].as_str().unwrap_or(&tnum).to_string(),
            tracking_number:    tnum,
            label_url:          None,
            estimated_delivery: consignment["deliveryDate"].as_str().map(String::from),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let resp = self.http
            .get(format!("{}/shipping/track/{}", self.base_url, tracking_number))
            .header("Authorization", self.auth_header())
            .header("Accept", "application/json")
            .send()
            .await?;

        let data   = Self::check_error(resp).await?;
        let parcel = &data["data"][0];

        let events: Vec<TrackingEvent> = parcel["events"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|e| TrackingEvent {
                timestamp:   e["completionDate"].as_str().unwrap_or("").into(),
                status:      e["eventCode"].as_str().unwrap_or("").into(),
                description: e["eventDescription"].as_str().unwrap_or("").into(),
                location:    e["depotCode"].as_str().map(String::from),
            })
            .collect();

        Ok(TrackingInfo {
            tracking_number:    tracking_number.into(),
            current_status:     parcel["status"].as_str().unwrap_or("UNKNOWN").into(),
            current_location:   events.first().and_then(|e| e.location.clone()),
            estimated_delivery: parcel["estimatedDeliveryDate"].as_str().map(String::from),
            events,
        })
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        let resp = self.http
            .delete(format!("{}/shipping/shipment/{}", self.base_url, booking_ref))
            .header("Authorization", self.auth_header())
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

    async fn get_label(&self, booking_ref: &str, format: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        let fmt_str = match format {
            LabelFormat::Pdf => "pdf",
            LabelFormat::Zpl => "zpl",
            LabelFormat::Png => "png",
        };

        let resp = self.http
            .get(format!("{}/shipping/label/{}", self.base_url, booking_ref))
            .header("Authorization", self.auth_header())
            .query(&[("format", fmt_str)])
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
