use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub env:  String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url:             String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    pub brokers: String,
}

/// Object storage for product photos.
///
/// Every field defaults to empty so an environment that has not configured
/// storage still boots — `PhotoStorage::new` refuses, the upload route reports
/// itself unconfigured, and catalogs keep serving. Photos are an addition to
/// OmniDeliv, not a precondition for it.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct StorageConfig {
    /// e.g. `http://minio:9000`. Empty disables photo storage.
    #[serde(default)]
    pub endpoint:   String,
    #[serde(default)]
    pub bucket:     String,
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default)]
    pub region:     Option<String>,
    /// MinIO needs path-style; virtual-hosted addressing resolves
    /// `bucket.minio:9000`, which has no DNS entry.
    #[serde(default = "default_true")]
    pub force_path_style: bool,
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,
    /// Absent env vars must still deserialize — see StorageConfig.
    #[serde(default)]
    pub storage:  StorageConfig,

    /// How old a vendor-declared availability flag may be before the catalog
    /// stops treating it as trustworthy. Drives defensive substitution — see
    /// `Availability::confidence`.
    #[serde(default = "default_stock_freshness_mins")]
    pub stock_freshness_mins: i64,

    /// How long a scanned table session stays valid.
    ///
    /// Minutes, not hours. The credential that mints it is printed on vinyl in
    /// a public room, so the blast radius of a photographed code is bounded
    /// mostly by how quickly what it mints stops working. Long enough for a
    /// meal, short enough that a code photographed at lunch is useless by
    /// dinner.
    #[serde(default = "default_table_session_mins")]
    pub table_session_mins: i64,

    /// How many parties may hold a live session at one table at once.
    ///
    /// A four-top does not need fifty. Without a cap, one photographed code is
    /// an unbounded session factory. Above one so a table of friends can each
    /// order from their own phone, which is the normal case this exists for.
    #[serde(default = "default_table_session_cap")]
    pub table_session_cap: i64,

    /// Base URL the printed QR code points at, e.g. `https://eat.example.com`.
    /// The code encodes `{base}/t/{token}`.
    #[serde(default = "default_table_scan_base_url")]
    pub table_scan_base_url: String,

    /// See the KNOWN LIMITATION in bootstrap: the mesh tool box is built once
    /// at startup, so these stand in for per-run request context until it is
    /// built per run.
    #[serde(default)]
    pub default_tenant_id: uuid::Uuid,
    #[serde(default = "default_lat")]
    pub default_lat: f64,
    #[serde(default = "default_lng")]
    pub default_lng: f64,

    /// field-ops, the platform tier that owns couriers. Checkout offers every
    /// placed order to it.
    #[serde(default = "default_field_ops_url")]
    pub field_ops_url: String,

    /// `services/payments`, for `PaymentMethod::Online`'s authorize-then-
    /// capture-or-void checkout. Unlike `field_ops_url`, a deployment with
    /// this misconfigured still boots and still takes COD orders — only
    /// `Online` checkout fails, and it fails loudly per-request rather than
    /// at startup.
    #[serde(default = "default_payments_url")]
    pub payments_url: String,

    /// The currency every `Online` authorization is opened in. One value for
    /// the whole service: OmniDeliv has no multi-currency concept anywhere
    /// else in this crate (baskets, prices and fees are all bare cents with
    /// no currency tag), so this is the one place that needs one.
    #[serde(default = "default_payment_currency")]
    pub payment_currency: String,

    /// Base URL the customer's browser/WebView is redirected to after
    /// completing (or abandoning) the hosted checkout page. The default is a
    /// placeholder — every real deployment must override this with its own
    /// public-facing URL.
    #[serde(default = "default_payment_return_url_base")]
    pub payment_return_url_base: String,

    /// How long an `Online` order may sit `Authorized` with no courier before
    /// its hold is voided and the order cancelled. NI's own docs describe an
    /// authorization void as same-day only, so this must stay well inside a
    /// day. 30 minutes mirrors `services/payments`' own hosted-checkout
    /// session TTL (`payment_intent_service::INTENT_TTL`) — long enough that
    /// a courier search genuinely has time to work, short enough to leave
    /// many hours of margin before NI's own same-day cutoff.
    #[serde(default = "default_online_no_courier_timeout_mins")]
    pub online_no_courier_timeout_mins: i64,

    /// The customer's whole wait on Screen B before the mesh gives up, shared
    /// across specialists. Tunable without a rebuild because the right value
    /// depends on the model and the load, and the first live run showed the
    /// compiled-in default was an order of magnitude too small.
    #[serde(default = "default_mesh_deadline_secs")]
    pub mesh_deadline_secs: u64,

    pub claude_api_key: String,
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    /// 8192 rather than ai-layer's 4096: mesh specialists emit structured
    /// proposals with several lines plus reasoning, and a tighter cap risks
    /// truncating a proposal mid-array — which the runner reads as unparseable
    /// and degrades, turning a token limit into a missing vertical.
    #[serde(default = "default_claude_max_tokens")]
    pub claude_max_tokens: u32,
}

fn default_field_ops_url() -> String { "http://field-ops:8090".to_string() }
fn default_payments_url() -> String { "http://payments:8012".to_string() }
fn default_payment_currency() -> String { "AED".to_string() }
fn default_payment_return_url_base() -> String {
    "https://app.omnideliv.example/payment/return".to_string()
}
fn default_online_no_courier_timeout_mins() -> i64 { 30 }

fn default_lat() -> f64 { 14.5995 }
fn default_lng() -> f64 { 120.9842 }

fn default_mesh_deadline_secs() -> u64 { 45 }
fn default_claude_model() -> String { "claude-opus-4-6".to_string() }
fn default_claude_max_tokens() -> u32 { 8192 }

fn default_stock_freshness_mins() -> i64 { 30 }

fn default_table_session_mins() -> i64 { 120 }
fn default_table_session_cap() -> i64 { 8 }
fn default_table_scan_base_url() -> String { "https://order.cargomarket.net".to_string() }

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8091)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
