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

fn default_lat() -> f64 { 14.5995 }
fn default_lng() -> f64 { 120.9842 }

fn default_mesh_deadline_secs() -> u64 { 45 }
fn default_claude_model() -> String { "claude-opus-4-6".to_string() }
fn default_claude_max_tokens() -> u32 { 8192 }

fn default_stock_freshness_mins() -> i64 { 30 }

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
