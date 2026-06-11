use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub redis:    RedisConfig,
    pub kafka:    KafkaConfig,
    /// Per-carrier API credentials. Each sub-section is optional;
    /// if absent, that carrier's adapter is not registered at startup.
    #[serde(default)]
    pub carriers: CarriersConfig,
    /// S3-compatible object storage for carrier labels (Cloudflare R2 / MinIO / AWS S3).
    #[serde(default)]
    pub s3: StorageConfig,
    /// URLs of sibling services consumed by background workers.
    #[serde(default)]
    pub services: ServicesConfig,
}

/// S3 / Cloudflare R2 / MinIO storage for carrier label files.
/// Env vars: S3__ACCESS_KEY_ID, S3__SECRET_ACCESS_KEY
#[derive(Debug, Deserialize, Clone, Default)]
pub struct StorageConfig {
    pub access_key_id:     Option<String>,
    pub secret_access_key: Option<String>,
}

/// HTTP URLs for sibling services — used by the allocation booking worker.
/// Env vars: SERVICES__ORDER_INTAKE_URL
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ServicesConfig {
    #[serde(default = "default_order_intake_url")]
    pub order_intake_url: String,
}

fn default_order_intake_url() -> String { "http://localhost:8004".into() }

/// Top-level carrier credentials container.
/// Each carrier section maps to env vars:
///   CARRIERS__DHL__API_KEY, CARRIERS__FEDEX__CLIENT_ID, etc.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct CarriersConfig {
    pub dhl:    Option<DhlConfig>,
    pub fedex:  Option<FedexConfig>,
    pub ups:    Option<UpsConfig>,
    pub tnt:    Option<TntConfig>,
    pub dpd:    Option<DpdConfig>,
    pub aramex: Option<AramexConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DhlConfig {
    pub base_url:       String,
    pub account_number: String,
    pub api_key:        String,
    pub api_secret:     String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FedexConfig {
    pub base_url:       String,
    pub client_id:      String,
    pub client_secret:  String,
    pub account_number: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct UpsConfig {
    pub base_url:       String,
    pub client_id:      String,
    pub client_secret:  String,
    pub account_number: String,
}

/// TNT uses the FedEx OAuth2 infrastructure — configure with a FedEx account
/// that has TNT international lanes activated.
#[derive(Debug, Deserialize, Clone)]
pub struct TntConfig {
    pub base_url:       String,
    pub client_id:      String,
    pub client_secret:  String,
    pub account_number: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DpdConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub account:  String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AramexConfig {
    pub base_url:             String,
    pub username:             String,
    pub password:             String,
    pub version:              String,
    pub entity:               String,
    pub account_number:       String,
    pub account_pin:          String,
    pub account_country_code: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    /// Optional secondary port for the gRPC server (APP__GRPC_PORT).
    /// If unset, the gRPC server is not started.
    pub grpc_port: Option<u16>,
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
