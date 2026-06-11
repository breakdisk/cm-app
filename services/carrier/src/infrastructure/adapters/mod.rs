/// Unified carrier adapter layer — one trait, six 3PL implementations.
///
/// The MCP tools (`mcp::tools`) call into this layer exclusively; they never
/// touch carrier-specific HTTP logic directly.  Adding a new carrier means
/// adding a module here and registering it in `AdapterRegistry::from_config`.
pub mod dhl;
pub mod fedex;
pub mod ups;
pub mod tnt;
pub mod dpd;
pub mod aramex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("HTTP transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Carrier API returned HTTP {status}: {body}")]
    CarrierError { status: u16, body: String },

    #[error("Carrier '{0}' is not configured on this tenant")]
    NotConfigured(String),

    #[error("Authentication failed: {0}")]
    Auth(String),

    #[error("Response parse error: {0}")]
    Parse(String),
}

// ── Shared request / response types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Address {
    pub name:        String,
    pub company:     Option<String>,
    pub street1:     String,
    pub street2:     Option<String>,
    pub city:        String,
    pub state:       Option<String>,
    pub postal_code: String,
    /// ISO 3166-1 alpha-2 (e.g. "PH", "US")
    pub country:     String,
    pub phone:       Option<String>,
    pub email:       Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateRequest {
    pub origin:       Address,
    pub destination:  Address,
    pub weight_kg:    f64,
    pub length_cm:    Option<f64>,
    pub width_cm:     Option<f64>,
    pub height_cm:    Option<f64>,
    /// Optional service type filter (carrier-specific code, e.g. "EXPRESS_9_00")
    pub service_type: Option<String>,
    /// ISO 4217 currency for returned prices (e.g. "USD", "PHP")
    pub currency:     String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateQuote {
    pub carrier_code:  String,
    pub service_code:  String,
    pub service_name:  String,
    /// Price in minor currency units (cents / centavos)
    pub price_cents:   i64,
    pub currency:      String,
    pub transit_days:  Option<u32>,
    /// ISO-8601 datetime until which this quote is valid
    pub valid_until:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingRequest {
    pub service_code:          String,
    pub shipper:               Address,
    pub consignee:             Address,
    pub weight_kg:             f64,
    pub length_cm:             Option<f64>,
    pub width_cm:              Option<f64>,
    pub height_cm:             Option<f64>,
    pub description:           String,
    pub declared_value_cents:  i64,
    pub currency:              String,
    /// Platform AWB — sent as customer reference to the carrier
    pub reference:             Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookingConfirmation {
    pub booking_ref:        String,
    pub tracking_number:    String,
    /// Direct URL to download the label (if carrier provides one).
    /// Populated by the MCP handler / allocation worker after uploading to R2.
    pub label_url:          Option<String>,
    /// Raw label bytes returned inline by the carrier API (e.g. DHL documentImages).
    /// Not serialized — consumed by the booking handler before the response is sent.
    #[serde(skip)]
    pub label_bytes:        Option<Vec<u8>>,
    /// ISO-8601 date
    pub estimated_delivery: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingEvent {
    /// ISO-8601 datetime
    pub timestamp:   String,
    /// Carrier status code
    pub status:      String,
    pub description: String,
    pub location:    Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingInfo {
    pub tracking_number:    String,
    pub current_status:     String,
    pub current_location:   Option<String>,
    /// ISO-8601 date
    pub estimated_delivery: Option<String>,
    pub events:             Vec<TrackingEvent>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LabelFormat {
    Pdf,
    Zpl,
    Png,
}

impl std::fmt::Display for LabelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LabelFormat::Pdf => write!(f, "PDF"),
            LabelFormat::Zpl => write!(f, "ZPL"),
            LabelFormat::Png => write!(f, "PNG"),
        }
    }
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait CarrierAdapter: Send + Sync {
    fn carrier_code(&self) -> &'static str;
    fn carrier_name(&self) -> &'static str;

    async fn get_rates(&self, req: &RateRequest) -> Result<Vec<RateQuote>, AdapterError>;
    async fn book_shipment(&self, req: &BookingRequest) -> Result<BookingConfirmation, AdapterError>;
    async fn track_shipment(&self, tracking_number: &str) -> Result<TrackingInfo, AdapterError>;
    async fn cancel_shipment(&self, booking_ref: &str) -> Result<(), AdapterError>;
    /// Returns raw label bytes (PDF/ZPL/PNG) ready to stream or store.
    async fn get_label(&self, booking_ref: &str, format: LabelFormat) -> Result<Vec<u8>, AdapterError>;
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// Thread-safe map from carrier code → active adapter.
/// Built once at startup from `Config`; shared via `Arc` across all MCP handlers.
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn CarrierAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self { adapters: HashMap::new() }
    }

    pub fn register(&mut self, adapter: Arc<dyn CarrierAdapter>) {
        self.adapters.insert(adapter.carrier_code().to_string(), adapter);
    }

    pub fn get(&self, code: &str) -> Option<Arc<dyn CarrierAdapter>> {
        self.adapters.get(code).cloned()
    }

    /// All active carrier codes — used by `get_carrier_rates` to fan out.
    pub fn codes(&self) -> Vec<&str> {
        self.adapters.keys().map(String::as_str).collect()
    }

    pub fn all(&self) -> Vec<Arc<dyn CarrierAdapter>> {
        self.adapters.values().cloned().collect()
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self { Self::new() }
}
