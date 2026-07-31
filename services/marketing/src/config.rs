use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub redis:    RedisConfig,
    pub kafka:    KafkaConfig,
    /// Optional: downstream service URLs for audience resolution.
    /// When `services.cdp_url` is absent the marketing service operates without
    /// CDP-based audience resolution (only explicit recipient lists are supported).
    #[serde(default)]
    pub services: ServicesConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ServicesConfig {
    /// Base URL of the CDP service, e.g. "http://cdp-svc:8080".
    pub cdp_url:   Option<String>,
    /// Long-lived internal service token used for service-to-service CDP calls.
    /// Must carry the `customers:view` permission.
    pub cdp_token: Option<String>,
    /// Base URL of the engagement service, e.g. "http://engagement-svc:8080".
    /// Used by the journey executor to check campaign open/click state for
    /// "condition" steps via `/v1/internal/campaign-sends/:campaign_id/:customer_id`.
    /// When absent, condition steps that check campaign engagement take the "no" branch.
    pub engagement_url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub brokers: String,
    pub group_id: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        config::Config::builder()
            .add_source(config::Environment::default().separator("__"))
            .build()?
            .try_deserialize()
            .map_err(Into::into)
    }
}
