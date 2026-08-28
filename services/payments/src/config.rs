use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    pub order_intake: OrderIntakeConfig,
    /// Network International (online card payment gateway) config. Optional:
    /// card payment is an additive capability layered on top of the core
    /// payments service (invoicing, COD, wallets, driver ledger, billing) —
    /// a deployment with no NI credentials set must still boot and serve
    /// everything else, not crash-loop on missing card-payment secrets.
    ///
    /// `#[serde(default)]` here turns out to be belt-and-suspenders, not
    /// load-bearing — verified empirically in
    /// `config_tests::load_succeeds_with_network_international_entirely_unset`
    /// below by temporarily deleting the attribute and re-running the test:
    /// it still passed. serde's derived `Deserialize` already special-cases
    /// `Option<T>` struct fields to default to `None` when the corresponding
    /// key is absent from the input, with no `#[serde(default)]` required —
    /// this is documented serde behavior, not something specific to the
    /// `config` crate's `Environment` source. `#[serde(default)]` is kept
    /// anyway so the "absent ⇒ None" contract is visible at the field
    /// declaration rather than relying on a reader already knowing that
    /// serde rule.
    ///
    /// A *partially* set NI config (e.g. `BASE_URL` set but `API_KEY` not)
    /// is a different case: `config`'s `Environment` source only omits the
    /// `network_international` key entirely when *zero* `NETWORK_INTERNATIONAL__*`
    /// vars are set. With even one set, the key is present as a partial map,
    /// and deserializing `NetworkInternationalConfig` from it fails on the
    /// still-missing required fields — same hard failure as before this
    /// change. That is intentional: a typo'd partial credential set should
    /// fail loud at boot, not silently disable the feature.
    #[serde(default)]
    pub network_international: Option<NetworkInternationalConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OrderIntakeConfig {
    /// Base URL of the order-intake service, e.g. http://order-intake:8004
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NetworkInternationalConfig {
    /// Base URL of NI's API — sandbox or production, set per environment.
    pub base_url: String,
    pub api_key: String,
    /// Shared secret used to verify inbound webhook signatures.
    pub webhook_secret: String,
    /// NI outlet reference this tenant's charges post against.
    pub outlet_ref: String,
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

#[cfg(test)]
mod config_tests {
    use super::*;

    /// The load-bearing regression test for this change: a fresh payments
    /// deployment with no `NETWORK_INTERNATIONAL__*` env vars set at all
    /// must still boot — this reproduces the exact crash-loop being fixed
    /// (`Config::load()` erroring out of `bootstrap::run()` before
    /// `axum::serve` ever binds, taking invoicing/COD/wallets down with it).
    ///
    /// One `#[test]` fn, not several: every assertion here mutates
    /// process-wide env, and separate `#[test]` fns would race each other
    /// under `cargo test`'s default parallelism — same reasoning as
    /// `services/identity/.../auth_service.rs::dev_otp_gate_tests`.
    #[test]
    fn load_succeeds_with_network_international_entirely_unset() {
        // SAFETY: single-threaded within this test; nothing else in this
        // crate's test binary reads or writes these env vars (checked: no
        // other `#[cfg(test)]` module touches `env::set_var`/`remove_var`).
        unsafe {
            std::env::set_var("APP__HOST", "0.0.0.0");
            std::env::set_var("APP__PORT", "8012");
            std::env::set_var("APP__ENV", "test");
            std::env::set_var("DATABASE__URL", "postgres://user:pass@localhost:5432/db");
            std::env::set_var("DATABASE__MAX_CONNECTIONS", "5");
            std::env::set_var("REDIS__URL", "redis://localhost:6379");
            std::env::set_var("KAFKA__BROKERS", "localhost:9092");
            std::env::set_var("KAFKA__GROUP_ID", "payments-test");
            std::env::set_var("ORDER_INTAKE__URL", "http://localhost:8004");
            std::env::remove_var("NETWORK_INTERNATIONAL__BASE_URL");
            std::env::remove_var("NETWORK_INTERNATIONAL__API_KEY");
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_SECRET");
            std::env::remove_var("NETWORK_INTERNATIONAL__OUTLET_REF");
        }

        let result = Config::load();

        // Clean up before asserting, so a failed assertion never leaves
        // process-wide env mutated for whatever test runs next.
        unsafe {
            std::env::remove_var("APP__HOST");
            std::env::remove_var("APP__PORT");
            std::env::remove_var("APP__ENV");
            std::env::remove_var("DATABASE__URL");
            std::env::remove_var("DATABASE__MAX_CONNECTIONS");
            std::env::remove_var("REDIS__URL");
            std::env::remove_var("KAFKA__BROKERS");
            std::env::remove_var("KAFKA__GROUP_ID");
            std::env::remove_var("ORDER_INTAKE__URL");
        }

        let cfg = result.expect(
            "Config::load() must succeed with every NETWORK_INTERNATIONAL__* var \
             unset — before this change this errored with \"missing field \
             `network_international`\" and crash-looped the whole service",
        );
        assert!(
            cfg.network_international.is_none(),
            "absent NI env vars must deserialize to None, not error and not \
             fabricate a config with empty-string fields"
        );
    }
}
