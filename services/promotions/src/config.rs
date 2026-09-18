use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub env:  String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url:             String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    pub brokers: String,
}

/// The rules the design fixes and the tenant may tune. Every figure that
/// moves money defaults to the design's value only where it *limits* a
/// discount (the window, the ceiling). Nothing is discounted until a tenant
/// creates an offer, so the defaults on their own give nothing away.
#[derive(Debug, Clone, Deserialize)]
pub struct PromotionsConfig {
    /// First and last day of the month a windowed code can be redeemed.
    #[serde(default = "default_window_from_day")]
    pub window_from_day: u32,
    #[serde(default = "default_window_to_day")]
    pub window_to_day: u32,
    /// The ceiling: `min(ceiling_pct% of gross, ceiling_flat_units)`, the flat
    /// cap in the move's own currency units, never converted.
    #[serde(default = "default_ceiling_pct")]
    pub ceiling_pct: i64,
    #[serde(default = "default_ceiling_flat_units")]
    pub ceiling_flat_units: i64,
    /// The tenant's local day, as a fixed offset from UTC. The platform has no
    /// tenant time zone; a deployment serves one region. 480 = Manila.
    #[serde(default = "default_utc_offset_minutes")]
    pub utc_offset_minutes: i32,
    /// Credit paid to a referrer when their friend's first move completes,
    /// before the tier multiplier. 0 (the default) turns rewards off.
    #[serde(default)]
    pub referral_reward_cents: i64,
    /// The currency referral credit is held in.
    #[serde(default = "default_credit_currency")]
    pub credit_currency: String,
}

impl Default for PromotionsConfig {
    fn default() -> Self {
        Self {
            window_from_day: default_window_from_day(),
            window_to_day: default_window_to_day(),
            ceiling_pct: default_ceiling_pct(),
            ceiling_flat_units: default_ceiling_flat_units(),
            utc_offset_minutes: default_utc_offset_minutes(),
            referral_reward_cents: 0,
            credit_currency: default_credit_currency(),
        }
    }
}

fn default_window_from_day() -> u32 { 11 }
fn default_window_to_day() -> u32 { 24 }
fn default_ceiling_pct() -> i64 { 20 }
fn default_ceiling_flat_units() -> i64 { 50 }
fn default_utc_offset_minutes() -> i32 { 480 }
fn default_credit_currency() -> String { "PHP".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,
    /// `PROMOTIONS__WINDOW_FROM_DAY` (11), `PROMOTIONS__WINDOW_TO_DAY` (24),
    /// `PROMOTIONS__CEILING_PCT` (20), `PROMOTIONS__CEILING_FLAT_UNITS` (50),
    /// `PROMOTIONS__UTC_OFFSET_MINUTES` (480), `PROMOTIONS__REFERRAL_REWARD_CENTS`
    /// (0), `PROMOTIONS__CREDIT_CURRENCY` (PHP).
    #[serde(default)]
    pub promotions: PromotionsConfig,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8022)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
