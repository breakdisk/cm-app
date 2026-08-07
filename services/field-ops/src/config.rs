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

    /// A courier's claim is released if no heartbeat arrives within this window,
    /// so a crashed client cannot hold a courier hostage forever.
    #[serde(default = "default_claim_ttl_secs")]
    pub claim_ttl_secs: i64,
}

fn default_claim_ttl_secs() -> i64 { 120 }

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8090)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
