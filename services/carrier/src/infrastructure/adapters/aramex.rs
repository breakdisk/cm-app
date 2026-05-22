/// Aramex REST API v1
/// Docs: https://www.aramex.com/developer/apis
/// Auth: API key passed as `ClientInfo` in every request body (Aramex-specific pattern).
/// Base URL: https://ws.aramex.net/ShippingAPI.V2
/// Sandbox URL: https://ws.sandbox.aramex.net/ShippingAPI.V2
///
/// Aramex is the dominant express carrier across the Middle East, SE Asia (Philippines),
/// and Africa — a key partner for LogisticOS's target markets.

use async_trait::async_trait;
use base64::Engine as _;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;

use super::{
    AdapterError, Address, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingEvent, TrackingInfo,
};

pub struct AramexAdapter {
    http:                Client,
    base_url:            String,
    username:            String,
    password:            String,
    version:             String,
    entity:              String,
    account_number:      String,
    account_pin:         String,
    account_country_code: String,
}

impl AramexAdapter {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        base_url: String,
        username: String,
        password: String,
        version: String,
        entity: String,
        account_number: String,
        account_pin: String,
        account_country_code: String,
    ) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("LogisticOS-CarrierGateway/1.0")
            .build()
            .expect("Aramex HTTP client");
        Self {
            http,
            base_url,
            username,
            password,
            version,
            entity,
            account_number,
            account_pin,
            account_country_code,
        }
    }

    /// Aramex includes full auth in every request body as `ClientInfo`.
    fn client_info(&self) -> Value {
        json!({
            "UserName":           self.username,
            "Password":           self.password,
            "Version":            self.version,
            "AccountNumber":      self.account_number,
            "AccountPin":         self.account_pin,
            "AccountEntity":      self.entity,
            "AccountCountryCode": self.account_country_code,
            "Source":             24,
        })
    }

    fn map_party(addr: &Address) -> Value {
        json!({
            "Reference1":    null,
            "Reference2":    null,
            "AccountNumber": null,
            "PartyAddress": {
                "Line1":       addr.street1,
                "Line2":       addr.street2,
                "Line3":       null,
                "City":        addr.city,
                "StateOrProvinceCode": addr.state,
                "PostCode":    addr.postal_code,
                "CountryCode": addr.country,
            },
            "Contact": {
                "Department":  null,
                "PersonName":  addr.name,
                "Title":       null,
                "CompanyName": addr.company,
                "PhoneNumber1": addr.phone,
                "PhoneNumber1Ext": null,
                "PhoneNumber2": null,
                "PhoneNumber2Ext": null,
                "FaxNumber":   null,
                "CellPhone":   null,
                "EmailAddress": addr.email,
                "Type":        null,
            }
        })
    }

    async fn check_error(resp: reqwest::Response) -> Result<Value, AdapterError> {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AdapterError::CarrierError { status: status.as_u16(), body });
        }
        let data: Value = serde_json::from_str(&body)
            .map_err(|e| AdapterError::Parse(e.to_string()))?;

        // Aramex embeds business-level errors in HTTP 200 responses
        if data["HasErrors"].as_bool().unwrap_or(false) {
            let msg = data["Notifications"]
                .as_array()
                .and_then(|n| n.first())
                .and_then(|n| n["Message"].as_str())
                .unwrap_or("Aramex API error")
                .to_string();
            return Err(AdapterError::CarrierError { status: 200, body: msg });
        }

        Ok(data)
    }
}

#[async_trait]
impl CarrierAdapter for AramexAdapter {
    fn carrier_code(&self) -> &'static str { "ARAMEX" }
    fn carrier_name(&self) -> &'static str { "Aramex" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let body = json!({
            "ClientInfo": self.client_info(),
            "Transaction": {
                "Reference1": "",
                "Reference2": "",
                "Reference3": "",
                "Reference4": "",
                "Reference5": ""
            },
            "OriginAddress": {
                "Line1":       req.origin.street1,
                "City":        req.origin.city,
                "PostCode":    req.origin.postal_code,
                "CountryCode": req.origin.country,
            },
            "DestinationAddress": {
                "Line1":       req.destination.street1,
                "City":        req.destination.city,
                "PostCode":    req.destination.postal_code,
                "CountryCode": req.destination.country,
            },
            "ShipmentDetails": {
                "Dimensions": null,
                "ActualWeight": {
                    "Unit":  "KG",
                    "Value": req.weight_kg,
                },
                "ChargeableWeight": null,
                "DescriptionOfGoods": null,
                "GoodsOriginCountry": req.origin.country,
                "NumberOfPieces": 1,
                "ProductGroup":   req.service_type.as_deref().unwrap_or("EXP"),
                "ProductType":    req.service_type.as_deref().unwrap_or("PPX"),
                "PaymentType":    "P",
                "PaymentOptions": "",
                "CustomsValueAmount": null,
                "CashOnDeliveryAmount": null,
                "CollectAmount":   null,
                "InsuranceAmount": null,
                "CashAdditionalAmount": null,
                "CashAdditionalAmountDescription": null,
                "DeliveryDutyPaid": false,
            },
        });

        let resp = self.http
            .post(format!("{}/RateCalculator/Service_1_0.svc/json/CalculateRate", self.base_url))
            .json(&body)
            .send()
            .await?;

        let data    = Self::check_error(resp).await?;
        let mut quotes = vec![];

        // Aramex returns a single rate quote for the requested product type
        if let Some(rate) = data["TotalAmount"].as_object() {
            let price_cents = rate["Value"]
                .as_f64()
                .map(|f| (f * 100.0) as i64)
                .unwrap_or(0);
            let currency = rate["CurrencyCode"].as_str().unwrap_or(&req.currency).to_string();

            quotes.push(RateQuote {
                carrier_code: "ARAMEX".into(),
                service_code: req.service_type.clone().unwrap_or_else(|| "PPX".into()),
                service_name: "Aramex Priority Parcel Express".into(),
                price_cents,
                currency,
                transit_days: data["TransitDays"].as_u64().map(|d| d as u32),
                valid_until:  None,
            });
        }

        Ok(quotes)
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        let body = json!({
            "ClientInfo": self.client_info(),
            "Transaction": { "Reference1": req.reference },
            "Shipments": [{
                "Shipper":    Self::map_party(&req.shipper),
                "Consignee":  Self::map_party(&req.consignee),
                "ShippingDateTime": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
                "DueDate":         null,
                "PickupLocation":  "RECEPTION",
                "Comments":        null,
                "PickupGUID":      null,
                "Reference1":      req.reference,
                "Reference2":      null,
                "Reference3":      null,
                "ForeignHAWB":     null,
                "TransportType":   0,
                "Details": {
                    "Dimensions":    null,
                    "ActualWeight":  { "Unit": "KG", "Value": req.weight_kg },
                    "ChargeableWeight": null,
                    "DescriptionOfGoods": req.description,
                    "GoodsOriginCountry": req.shipper.country,
                    "NumberOfPieces": 1,
                    "ProductGroup":   "EXP",
                    "ProductType":    req.service_code,
                    "PaymentType":    "P",
                    "PaymentOptions": "",
                    "CustomsValueAmount": {
                        "CurrencyCode": req.currency,
                        "Value": req.declared_value_cents as f64 / 100.0,
                    },
                    "CashOnDeliveryAmount":  null,
                    "CollectAmount":         null,
                    "InsuranceAmount":       null,
                    "CashAdditionalAmount":  null,
                    "DeliveryDutyPaid":      false,
                    "Items": null,
                }
            }],
            "LabelInfo": {
                "ReportID":   9201,
                "ReportType": "URL",
            }
        });

        let resp = self.http
            .post(format!("{}/Shipping/Service_1_0.svc/json/CreateShipments", self.base_url))
            .json(&body)
            .send()
            .await?;

        let data     = Self::check_error(resp).await?;
        let shipment = &data["Shipments"][0];
        let tnum     = shipment["ID"].as_str().unwrap_or("").to_string();
        let label    = shipment["ShipmentLabel"]["LabelURL"].as_str().map(String::from);

        Ok(BookingConfirmation {
            booking_ref:        tnum.clone(),
            tracking_number:    tnum,
            label_url:          label,
            estimated_delivery: shipment["ScheduledDelivery"].as_str().map(String::from),
        })
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let body = json!({
            "ClientInfo": self.client_info(),
            "Transaction": null,
            "Shipments": [tracking_number],
            "GetLastTrackingUpdateOnly": false,
        });

        let resp = self.http
            .post(format!("{}/Tracking/Service_1_0.svc/json/TrackShipments", self.base_url))
            .json(&body)
            .send()
            .await?;

        let data     = Self::check_error(resp).await?;
        let shipment = &data["TrackingResults"][0]["Value"][0];

        let events: Vec<TrackingEvent> = shipment["TrackingUpdates"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .map(|e| TrackingEvent {
                timestamp:   e["UpdateDateTime"].as_str().unwrap_or("").into(),
                status:      e["UpdateCode"].as_str().unwrap_or("").into(),
                description: e["UpdateDescription"].as_str().unwrap_or("").into(),
                location:    e["UpdateLocation"].as_str().map(String::from),
            })
            .collect();

        Ok(TrackingInfo {
            tracking_number:    tracking_number.into(),
            current_status:     events
                .first()
                .map(|e| e.status.clone())
                .unwrap_or_else(|| "UNKNOWN".into()),
            current_location:   events.first().and_then(|e| e.location.clone()),
            estimated_delivery: shipment["ScheduledDelivery"].as_str().map(String::from),
            events,
        })
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        let body = json!({
            "ClientInfo": self.client_info(),
            "Transaction": null,
            "Shipments": [booking_ref],
        });

        let resp = self.http
            .post(format!("{}/Shipping/Service_1_0.svc/json/DeleteShipments", self.base_url))
            .json(&body)
            .send()
            .await?;

        Self::check_error(resp).await?;
        Ok(())
    }

    async fn get_label(&self, booking_ref: &str, _format: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        let body = json!({
            "ClientInfo": self.client_info(),
            "Transaction": null,
            "ShipmentNumber": booking_ref,
            "LabelInfo": { "ReportID": 9201, "ReportType": "B" },
        });

        let resp = self.http
            .post(format!("{}/Shipping/Service_1_0.svc/json/PrintLabel", self.base_url))
            .json(&body)
            .send()
            .await?;

        let data   = Self::check_error(resp).await?;
        let b64    = data["Label"]["LabelFileContents"]
            .as_str()
            .ok_or_else(|| AdapterError::Parse("Aramex label content missing".into()))?;

        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| AdapterError::Parse(e.to_string()))
    }
}
