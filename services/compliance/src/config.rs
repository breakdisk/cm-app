use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:      AppConfig,
    pub auth:     AuthConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,
    pub storage:  StorageConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    /// Access token TTL in seconds. Default: 3600 (1 hour).
    #[serde(default = "AuthConfig::default_access_ttl")]
    pub access_token_ttl:  i64,
    /// Refresh token TTL in seconds. Default: 86400 (24 hours).
    #[serde(default = "AuthConfig::default_refresh_ttl")]
    pub refresh_token_ttl: i64,
}

impl AuthConfig {
    fn default_access_ttl()  -> i64 { 3600  }
    fn default_refresh_ttl() -> i64 { 86400 }
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
    /// S3 region. MinIO/R2 accept any value; defaults to "auto" when unset so
    /// the AWS SDK never falls back to the IMDS probe (which times out on a
    /// non-AWS VPS and then errors "A region must be set").
    #[serde(default)]
    pub region: Option<String>,
    /// Path-style addressing (`endpoint/bucket/key`) vs virtual-hosted
    /// (`bucket.endpoint/key`). MinIO REQUIRES path-style. Cloudflare R2 works
    /// with either for direct API calls. Defaults to true (MinIO is the current
    /// backend); set STORAGE__FORCE_PATH_STYLE=false when moving to R2.
    #[serde(default)]
    pub force_path_style: Option<bool>,
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
