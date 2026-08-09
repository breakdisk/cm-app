use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:          AppConfig,
    pub database:     DatabaseConfig,
    pub order_intake: OrderIntakeConfig,
    /// Optional: a deployment with no OmniDeliv tier configured simply cannot
    /// sync catalogs, and the route says so. Making it required would stop the
    /// whole service booting over a feature most tenants do not use.
    #[serde(default)]
    pub omnideliv:    Option<OmniDelivConfig>,
    pub auth:         AuthConfig,
}

/// Base URL of the omnideliv service, reached over the mesh.
/// Example: http://omnideliv:8091
#[derive(Debug, Deserialize, Clone)]
pub struct OmniDelivConfig {
    pub internal_url: String,
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
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 { 10 }

/// Base URL of the order-intake service's internal endpoint.
/// Example: http://order-intake:8005
#[derive(Debug, Deserialize, Clone)]
pub struct OrderIntakeConfig {
    pub internal_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
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
