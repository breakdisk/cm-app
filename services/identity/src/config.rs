use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    pub auth: AuthConfig,
    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: String,
    /// Comma-separated allowed CORS origins. Defaults to localhost dev ports.
    /// Production example: `https://app.cargomarket.net,https://admin.cargomarket.net`
    #[serde(default)]
    pub cors_origins: Option<String>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_seconds: u64,
    pub refresh_token_expiry_seconds: u64,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct EmailConfig {
    /// AWS SES sender address — e.g. "LogisticOS <no-reply@logisticos.app>"
    pub from_address: Option<String>,
    /// AWS region for SES (defaults to us-east-1; overridden by AWS_REGION).
    pub aws_region: Option<String>,
    /// Base URL for reset/verify links — e.g. "https://app.logisticos.app"
    pub app_base_url: Option<String>,
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
