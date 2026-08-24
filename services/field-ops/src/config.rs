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

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,

    /// Whether a courier compliance verdict actually blocks dispatch.
    ///
    /// Ships **false**, and that is a rollout decision, not timidity. No
    /// courier in production has a compliance profile — nothing has ever
    /// published `driver.registered`, so none was ever created — and a new
    /// profile starts at `pending_submission`, which compliance does not
    /// consider assignable. Turning this on the same day profiles start being
    /// created would take the live fleet off the road as their profiles
    /// appeared, one courier at a time, with the cause several services away.
    ///
    /// With it false the verdict is still consumed, stored and shown on the ops
    /// roster, and `offer_to_nearest` logs every courier it *would* have
    /// refused. Flip it once that log is quiet and the roster shows the fleet
    /// cleared.
    #[serde(default)]
    pub enforce_compliance: bool,

    /// The jurisdiction a newly registered courier's compliance profile is
    /// opened under; decides which documents they are required to hold.
    ///
    /// Configuration rather than a constant, for the same reason the courier
    /// app's default country code is: it belongs to the tenant, and a launch
    /// market baked into platform code is how "PH" ends up demanding an LTO
    /// licence from a courier in Dubai.
    #[serde(default = "default_jurisdiction")]
    pub default_jurisdiction: String,

    /// A courier's claim is released if no heartbeat arrives within this window,
    /// so a crashed client cannot hold a courier hostage forever.
    #[serde(default = "default_claim_ttl_secs")]
    pub claim_ttl_secs: i64,

    /// Sanity bounds on what a product may declare a courier will earn.
    ///
    /// ADR-0015 makes pay a product decision — field-ops credits what it is
    /// told and never computes a tariff. These are not a pricing policy; they
    /// are the guard that a product *bug* cannot pay a courier one peso or a
    /// million. A declaration outside them is rejected at the offer, before
    /// anything is stored or credited.
    #[serde(default = "default_min_trip_cents")]
    pub min_trip_cents: i64,
    #[serde(default = "default_max_trip_cents")]
    pub max_trip_cents: i64,
    #[serde(default = "default_max_tip_cents")]
    pub max_tip_cents: i64,
}

/// ₱20. Below this a paid trip is almost certainly a units error — cents read
/// as pesos, or an uninitialised field.
fn default_min_trip_cents() -> i64 { 2_000 }
/// ₱2,000. Well above any real single-trip fee, low enough to catch a
/// fat-finger or a multiplication that ran twice.
fn default_max_trip_cents() -> i64 { 200_000 }
/// ₱5,000. Generous tips are real; a tip larger than most orders is not.
fn default_max_tip_cents() -> i64 { 500_000 }

fn default_claim_ttl_secs() -> i64 { 120 }

/// Matches the fallback the compliance service's own lazy profile creation
/// uses, so a courier onboarded through either path lands in one jurisdiction.
fn default_jurisdiction() -> String { "PH".to_owned() }

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8090)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
