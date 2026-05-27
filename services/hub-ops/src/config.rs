use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub payments: PaymentsConfig,
    #[serde(default)]
    pub pod: PodConfig,
}

/// Downstream payments service config for billing clearance checks.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PaymentsConfig {
    /// Base URL of the payments service (e.g. "http://payments:8080").
    /// When empty, billing clearance checks are skipped (container departure always passes).
    #[serde(default)]
    pub url: String,
}

/// Downstream pod service config for POP status checks.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct PodConfig {
    /// Base URL of the pod service (e.g. "http://pod:8080").
    /// When empty, POP status checks are skipped (container loading always passes).
    #[serde(default)]
    pub url: String,
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
