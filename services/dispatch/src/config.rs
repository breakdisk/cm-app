use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub dispatch: DispatchConfig,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct DispatchConfig {
    /// `DISPATCH__CLOSE_ASSIGNMENTS`. Writes `completed` assignments when work
    /// ends. Keep false until every replica runs a build that reads
    /// `completed` (an older image would read it as an active assignment).
    #[serde(default)]
    pub close_assignments: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env: String,
    /// When true the shipment consumer defers auto-dispatch to the AI layer agent
    /// instead of running the heuristic directly. Fallback to heuristic if the
    /// AI layer is unreachable.
    #[serde(default)]
    pub ai_dispatch_enabled: bool,
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
