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

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,

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

fn default_lat() -> f64 { 14.5995 }
fn default_lng() -> f64 { 120.9842 }

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
