use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub geocoder: GeocoderConfig,
    /// HMAC-SHA256 signing secret for short-TTL quote tokens
    /// (`domain::value_objects::quote_token`). A top-level field, so it is
    /// read from the env var QUOTE_TOKEN_SECRET directly — no `__` prefix,
    /// since the `__` separator only applies between nested struct fields
    /// (e.g. `database.url` -> DATABASE__URL). Required, no default: an
    /// unset signing secret must fail service startup loudly, not silently
    /// sign/verify quotes with an empty key.
    pub quote_token_secret: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: String,
    /// Comma-separated list of allowed CORS origins.
    /// e.g. APP__CORS_ORIGINS=https://os.cargomarket.net,https://admin.cargomarket.net
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

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GeocoderConfig {
    /// Public Mapbox token (pk.*) with Geocoding scope. Set via
    /// GEOCODER__MAPBOX_ACCESS_TOKEN. When empty, the service falls back to
    /// PassthroughNormalizer and shipments are created with coordinates: None.
    #[serde(default)]
    pub mapbox_access_token: Option<String>,
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
