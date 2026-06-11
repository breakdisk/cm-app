use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    /// Internal URL of the pod service — used to fetch POP/POD evidence when
    /// building enriched tracking responses for merchant/customer portals.
    /// Defaults to the Dokploy Docker-network hostname.
    /// Override with `POD_INTERNAL_URL` env var.
    #[serde(default = "default_pod_internal_url")]
    pub pod_internal_url: String,
}

fn default_pod_internal_url() -> String {
    // "pod" is the Docker Compose service name — reachable on the logisticos
    // bridge network. Dokploy uses the same name. Override with POD_INTERNAL_URL
    // in local dev where the service name isn't resolvable.
    "http://pod:8011".to_string()
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
