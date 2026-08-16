//! TNT Express — FedEx acquired TNT in 2016.
//! TNT Connect APIs are being sunset; new integrations use FedEx APIs with
//! TNT-specific service codes (e.g. "FEDEX_TNT_EXPRESS").
//!
//! This adapter wraps `FedexAdapter` transparently, overriding carrier_code
//! and carrier_name so that TNT appears as a distinct carrier in rate quotes
//! and bookings while routing through the FedEx infrastructure.
//!
//! TNT-specific service codes on FedEx infrastructure:
//!   - 09DOC  — TNT Express Documents (same-day / overnight)
//!   - ECONOMY_SELECT — TNT Economy Express
//!   - FEDEX_INTERNATIONAL_PRIORITY — replaces TNT Express
//!
//! Docs: https://developer.fedex.com/api/en-us/catalog/ship/v1/docs.html
//! Auth: Same FedEx OAuth2 credentials; use the FedEx account that has TNT lanes activated.
//! Sandbox base URL: https://apis-sandbox.fedex.com
//! Production base URL: https://apis.fedex.com

use async_trait::async_trait;
use std::sync::Arc;

use super::{
    AdapterError, BookingConfirmation, BookingRequest, CarrierAdapter,
    LabelFormat, RateQuote, RateRequest, TrackingInfo,
    fedex::FedexAdapter,
};

pub struct TntAdapter {
    inner: Arc<FedexAdapter>,
}

impl TntAdapter {
    /// Create a TNT adapter backed by a FedEx account with TNT lanes.
    pub fn new(
        base_url: String,
        client_id: String,
        client_secret: String,
        account_number: String,
    ) -> Self {
        Self {
            inner: Arc::new(FedexAdapter::new(
                base_url,
                client_id,
                client_secret,
                account_number,
            )),
        }
    }
}

#[async_trait]
impl CarrierAdapter for TntAdapter {
    fn carrier_code(&self) -> &'static str { "TNT" }
    fn carrier_name(&self) -> &'static str { "TNT Express (FedEx)" }

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError> {
        let quotes = self.inner.get_rates(req).await?;
        // Re-label all returned quotes as TNT
        Ok(quotes
            .into_iter()
            .map(|q| RateQuote { carrier_code: "TNT".into(), ..q })
            .collect())
    }

    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError> {
        self.inner.book_shipment(req).await
    }

    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError> {
        let mut info = self.inner.track_shipment(tracking_number).await?;
        info.tracking_number = tracking_number.to_string();
        Ok(info)
    }

    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError> {
        self.inner.cancel_shipment(booking_ref).await
    }

    async fn get_label(&self, booking_ref: &str, format: LabelFormat) -> Result<Vec<u8>, AdapterError> {
        self.inner.get_label(booking_ref, format).await
    }
}
