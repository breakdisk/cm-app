use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub auth: AuthConfig,
    pub redis: RedisConfig,
    pub services: ServicesConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: String,
    /// Comma-separated list of allowed CORS origins.
    /// Use `*` to allow all origins (dev/staging only).
    /// Production example: `https://os.cargomarket.net,https://admin.cargomarket.net`
    #[serde(default = "default_cors_origins")]
    pub cors_allowed_origins: String,
}

fn default_cors_origins() -> String {
    "*".to_owned()
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_expiry_seconds: i64,
    pub refresh_token_expiry_seconds: i64,
}

/// Downstream service URLs — injected via environment.
/// In K8s these are Kubernetes service DNS names.
#[derive(Debug, Deserialize, Clone)]
pub struct ServicesConfig {
    pub identity_url:             String,  // http://identity:8001
    pub cdp_url:                  String,
    pub engagement_url:           String,
    pub order_intake_url:         String,
    pub dispatch_url:             String,
    pub driver_ops_url:           String,
    pub delivery_experience_url:  String,
    pub fleet_url:                String,
    pub hub_ops_url:              String,
    pub carrier_url:              String,
    pub pod_url:                  String,
    pub payments_url:             String,
    pub analytics_url:            String,
    pub marketing_url:            String,
    pub business_logic_url:       String,
    pub ai_layer_url:             String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
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
