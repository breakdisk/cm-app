use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:       AppConfig,
    pub database:  DatabaseConfig,
    pub redis:     RedisConfig,
    pub kafka:     KafkaConfig,
    pub anthropic: AnthropicConfig,
    pub services:  DownstreamServices,
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

/// Anthropic API credentials.
#[derive(Debug, Deserialize, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
}

/// Internal service URLs for tool execution.
#[derive(Debug, Deserialize, Clone)]
pub struct DownstreamServices {
    pub dispatch_url:     String,
    pub order_intake_url: String,
    pub driver_ops_url:   String,
    pub payments_url:     String,
    pub engagement_url:   String,
    pub analytics_url:    String,
    pub cdp_url:          String,
    pub hub_ops_url:      String,
    pub fleet_url:        String,
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
