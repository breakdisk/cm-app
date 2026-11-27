use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    /// Internal HTTP base URL for the identity service, e.g. http://identity:8080
    /// Used to fetch FCM push tokens before notifying drivers.
    #[serde(default)]
    pub identity: IdentityConfig,
    /// Firebase Cloud Messaging configuration for driver push notifications.
    /// Optional — if absent, FCM push is skipped (tasks still appear via polling).
    #[serde(default)]
    pub fcm: FcmConfig,
    /// Hours-of-service clock — display and record only.
    /// `HOS__MAX_ON_DUTY_MINUTES` (660) and `HOS__WINDOW_HOURS` (14).
    #[serde(default)]
    pub hos: HosConfig,
    /// Leaving an accepted job: the grace clock, the drop fee, waiting pay.
    /// `PENALTY__GRACE_MINUTES` (45), `PENALTY__DROP_FEE_PCT` (0),
    /// `PENALTY__WAITING_FEE_CENTS_PER_HOUR` (0),
    /// `PENALTY__DROP_COUNTS_AS_DECLINES` (1).
    #[serde(default)]
    pub penalty: PenaltyConfig,
}

/// Every money figure is 0 until a rate card sets it, the precedent set by
/// `CANCELLATION_POLICY__*`. The grace window defaults on because it protects
/// the driver: without it, leaving is always a drop.
#[derive(Debug, Deserialize, Clone)]
pub struct PenaltyConfig {
    #[serde(default = "default_grace_minutes")]
    pub grace_minutes: i64,
    #[serde(default)]
    pub drop_fee_pct: i64,
    /// Paid to the driver. Nothing charges the customer for it yet, so a
    /// nonzero rate is the tenant's own cost until payments does.
    #[serde(default)]
    pub waiting_fee_cents_per_hour: i64,
    #[serde(default = "default_drop_counts_as_declines")]
    pub drop_counts_as_declines: i64,
}

impl Default for PenaltyConfig {
    fn default() -> Self {
        Self {
            grace_minutes: default_grace_minutes(),
            drop_fee_pct: 0,
            waiting_fee_cents_per_hour: 0,
            drop_counts_as_declines: default_drop_counts_as_declines(),
        }
    }
}

fn default_grace_minutes() -> i64 { 45 }
fn default_drop_counts_as_declines() -> i64 { 1 }

#[derive(Debug, Deserialize, Clone)]
pub struct HosConfig {
    #[serde(default = "default_hos_max_on_duty_minutes")]
    pub max_on_duty_minutes: i64,
    #[serde(default = "default_hos_window_hours")]
    pub window_hours: i64,
}

impl Default for HosConfig {
    fn default() -> Self {
        Self {
            max_on_duty_minutes: default_hos_max_on_duty_minutes(),
            window_hours: default_hos_window_hours(),
        }
    }
}

fn default_hos_max_on_duty_minutes() -> i64 { 660 }
fn default_hos_window_hours() -> i64 { 14 }

#[derive(Debug, Deserialize, Clone, Default)]
pub struct IdentityConfig {
    pub internal_url: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct FcmConfig {
    pub project_id: String,
    /// Full Firebase service account JSON, base64-encoded.
    /// Set via DRIVER_OPS_FCM__SERVICE_ACCOUNT_JSON env var.
    pub service_account_json: String,
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
