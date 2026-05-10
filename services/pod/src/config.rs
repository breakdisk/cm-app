use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub storage: StorageConfig,
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

/// S3 / Cloudflare R2 / MinIO storage configuration.
///
/// All fields are optional so existing deployments without these env vars
/// keep working. For Cloudflare R2, set S3_ACCESS_KEY_ID and
/// S3_SECRET_ACCESS_KEY explicitly — R2 keys are 32 chars and the AWS SDK's
/// auto-discovery chain will pick up any 20-char AWS_ACCESS_KEY_ID instead,
/// causing presigned URLs to be rejected with "Credential access key has
/// length 20, should be 32".
#[derive(Debug, Deserialize, Clone, Default)]
pub struct StorageConfig {
    /// R2/S3/MinIO access key ID — 32 chars for R2, 20 chars for AWS S3.
    /// Set via S3__ACCESS_KEY_ID env var.
    pub access_key_id: Option<String>,
    /// Corresponding secret access key. Set via S3__SECRET_ACCESS_KEY env var.
    pub secret_access_key: Option<String>,
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
