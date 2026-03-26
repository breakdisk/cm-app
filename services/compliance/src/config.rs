use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,
    pub storage:  StorageConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env:  String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url:             String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub brokers:          String,
    pub consumer_group:   String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    pub endpoint:   String,
    pub bucket:     String,
    pub access_key: String,
    pub secret_key: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let cfg = config::Config::builder()
            .add_source(config::Environment::default().separator("__"))
            .build()?
            .try_deserialize()?;
        Ok(cfg)
    }
}
