use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:       AppConfig,
    pub database:  DatabaseConfig,
    pub redis:     RedisConfig,
    pub kafka:     KafkaConfig,
    pub anthropic: AnthropicConfig,
    pub services:  DownstreamServices,
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

/// Anthropic API credentials and model selection.
///
/// Model and token budget used to be compile-time constants inside the Claude
/// client. Moving that client into `logisticos-agent-runtime` — which is shared
/// across products and must not carry one product's model choice — pushed the
/// decision out here, where it also becomes deployment-tunable.
#[derive(Debug, Deserialize, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    #[serde(default = "default_claude_model")]
    pub model: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
}

fn default_claude_model() -> String { "claude-opus-4-6".to_string() }
fn default_max_tokens() -> u32 { 4096 }

/// Internal service URLs for tool execution.
#[derive(Debug, Deserialize, Clone)]
pub struct DownstreamServices {
    pub dispatch_url:     String,
    pub order_intake_url: String,
    pub driver_ops_url:   String,
    pub payments_url:     String,
    pub engagement_url:   String,
    pub analytics_url:    String,
    pub cdp_url:          String,
    pub hub_ops_url:      String,
    pub fleet_url:        String,
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
