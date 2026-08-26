use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    pub order_intake: OrderIntakeConfig,
    pub network_international: NetworkInternationalConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OrderIntakeConfig {
    /// Base URL of the order-intake service, e.g. http://order-intake:8004
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkInternationalConfig {
    /// Base URL of NI's API — sandbox or production, set per environment.
    pub base_url: String,
    pub api_key: String,
    /// Shared secret used to verify inbound webhook signatures.
    pub webhook_secret: String,
    /// NI outlet reference this tenant's charges post against.
    pub outlet_ref: String,
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
