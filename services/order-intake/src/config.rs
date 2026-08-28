use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    #[serde(default)]
    pub geocoder: GeocoderConfig,
    /// Base URL of the payments service. Part of the optional "pay online
    /// at booking" capability, together with `quote_token_secret` and
    /// `app.public_base_url` — see the doc comment on `payment_config()`
    /// below for why these three are treated as one all-or-nothing unit
    /// rather than three independent switches. `#[serde(default)]` because,
    /// per serde's derive rules, a struct field of type `Option<T>` is
    /// already treated as `None` when its key is absent from the input —
    /// verified for this exact shape in payments'
    /// `config::config_tests::load_succeeds_with_network_international_
    /// entirely_unset` (`services/payments/src/config.rs`) — but kept here
    /// too so "absent ⇒ None" is visible at the field declaration.
    #[serde(default)]
    pub payments: Option<PaymentsConfig>,
    /// HMAC-SHA256 signing secret for short-TTL quote tokens
    /// (`domain::value_objects::quote_token`). A top-level field, so it is
    /// read from the env var QUOTE_TOKEN_SECRET directly — no `__` prefix,
    /// since the `__` separator only applies between nested struct fields
    /// (e.g. `database.url` -> DATABASE__URL).
    ///
    /// Optional, together with `payments` and `app.public_base_url`: online
    /// card payment at booking is an additive capability on top of order
    /// intake's core job (create/track/cancel a shipment) — a deployment
    /// with no payment config must still boot and take cash bookings.
    #[serde(default)]
    pub quote_token_secret: Option<String>,
}

/// The three fields required for the "pay online at booking" capability,
/// bundled so a caller can never observe two of the three configured and
/// the third missing. Returned by `Config::payment_config()`.
#[derive(Debug)]
pub struct PaymentConfig {
    pub payments_url: String,
    pub quote_token_secret: String,
    pub public_base_url: String,
}

impl Config {
    /// All three of `payments.url`, `quote_token_secret`, and
    /// `app.public_base_url` present ⇒ online payment is enabled; any one
    /// missing ⇒ disabled. Deliberately not "whichever are set, use those" —
    /// a deployment that set `quote_token_secret` but forgot
    /// `app.public_base_url` must not silently mint a quote it can never
    /// turn into a valid checkout return link. Returns the missing env var
    /// names (for the startup WARN) when disabled.
    pub fn payment_config(&self) -> Result<PaymentConfig, Vec<&'static str>> {
        let mut missing = Vec::new();
        if self.payments.is_none() {
            missing.push("PAYMENTS__URL");
        }
        if self.quote_token_secret.is_none() {
            missing.push("QUOTE_TOKEN_SECRET");
        }
        if self.app.public_base_url.is_none() {
            missing.push("APP__PUBLIC_BASE_URL");
        }
        match (&self.payments, &self.quote_token_secret, &self.app.public_base_url) {
            (Some(payments), Some(secret), Some(base_url)) => Ok(PaymentConfig {
                payments_url: payments.url.clone(),
                quote_token_secret: secret.clone(),
                public_base_url: base_url.clone(),
            }),
            _ => Err(missing),
        }
    }
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
    /// Base URL a merchant/customer is redirected back to after completing
    /// (or abandoning) a hosted checkout for a payment-gated shipment —
    /// passed to the payments service as the gateway `return_url`.
    /// e.g. APP__PUBLIC_BASE_URL=https://os.cargomarket.net
    ///
    /// Optional, together with `payments` and `quote_token_secret` on
    /// `Config` — see `Config::payment_config()`. An unset base URL alone
    /// must not crash-loop order-intake; it disables online payment only.
    #[serde(default)]
    pub public_base_url: Option<String>,
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

#[derive(Debug, Deserialize, Clone)]
pub struct PaymentsConfig {
    /// Base URL of the payments service, e.g. http://payments:8012
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

#[cfg(test)]
mod config_tests {
    use super::*;

    /// The load-bearing regression test for this change: a fresh
    /// order-intake deployment with `PAYMENTS__URL`, `QUOTE_TOKEN_SECRET`,
    /// and `APP__PUBLIC_BASE_URL` all unset must still boot — this
    /// reproduces the exact crash-loop being fixed (`Config::load()`
    /// erroring out of `bootstrap::run()` before `axum::serve` ever binds,
    /// taking shipment creation/tracking/cancellation down with it, none of
    /// which have anything to do with online card payment).
    ///
    /// One `#[test]` fn, not several: every assertion here mutates
    /// process-wide env, and separate `#[test]` fns would race each other
    /// under `cargo test`'s default parallelism — same reasoning as
    /// `services/identity/.../auth_service.rs::dev_otp_gate_tests` and
    /// payments' identical `config_tests` module
    /// (`services/payments/src/config.rs`).
    #[test]
    fn load_succeeds_with_the_payment_capability_entirely_unset() {
        // SAFETY: single-threaded within this test; nothing else in this
        // crate's test binary reads or writes these env vars.
        unsafe {
            std::env::set_var("APP__HOST", "0.0.0.0");
            std::env::set_var("APP__PORT", "8004");
            std::env::set_var("APP__ENV", "test");
            std::env::set_var("DATABASE__URL", "postgres://user:pass@localhost:5432/db");
            std::env::set_var("DATABASE__MAX_CONNECTIONS", "5");
            std::env::set_var("REDIS__URL", "redis://localhost:6379");
            std::env::set_var("KAFKA__BROKERS", "localhost:9092");
            std::env::set_var("KAFKA__GROUP_ID", "order-intake-test");
            std::env::remove_var("PAYMENTS__URL");
            std::env::remove_var("QUOTE_TOKEN_SECRET");
            std::env::remove_var("APP__PUBLIC_BASE_URL");
            std::env::remove_var("APP__CORS_ORIGINS");
            std::env::remove_var("GEOCODER__MAPBOX_ACCESS_TOKEN");
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
        }

        let cfg = result.expect(
            "Config::load() must succeed with PAYMENTS__URL, QUOTE_TOKEN_SECRET, and \
             APP__PUBLIC_BASE_URL all unset — before this change this errored with \
             \"missing field\" and crash-looped the whole service",
        );
        assert!(cfg.payments.is_none());
        assert!(cfg.quote_token_secret.is_none());
        assert!(cfg.app.public_base_url.is_none());

        let missing = cfg.payment_config().expect_err(
            "payment_config() must report the capability disabled, not fabricate a config",
        );
        assert_eq!(
            missing,
            vec!["PAYMENTS__URL", "QUOTE_TOKEN_SECRET", "APP__PUBLIC_BASE_URL"],
            "all three must be named as missing so the startup WARN is accurate",
        );
    }

    /// The "no inconsistent half-enabled state" guarantee `payment_config()`
    /// exists for: two of the three set is still disabled, not a broken
    /// partial capability.
    #[test]
    fn payment_config_is_disabled_when_only_some_of_the_three_fields_are_set() {
        let cfg = Config {
            app: AppConfig {
                host: "0.0.0.0".into(),
                port: 8004,
                env: "test".into(),
                cors_origins: None,
                public_base_url: Some("https://portal.test.local".into()),
            },
            database: DatabaseConfig { url: "postgres://x".into(), max_connections: 5 },
            redis: RedisConfig { url: "redis://x".into() },
            kafka: KafkaConfig { brokers: "x".into(), group_id: "x".into() },
            geocoder: GeocoderConfig::default(),
            payments: Some(PaymentsConfig { url: "http://payments:8012".into() }),
            // quote_token_secret deliberately missing — two of three set.
            quote_token_secret: None,
        };

        let missing = cfg.payment_config().expect_err("two of three set must still be disabled");
        assert_eq!(missing, vec!["QUOTE_TOKEN_SECRET"]);
    }

    /// The enabled case: all three present yields a usable `PaymentConfig`.
    #[test]
    fn payment_config_is_enabled_when_all_three_fields_are_set() {
        let cfg = Config {
            app: AppConfig {
                host: "0.0.0.0".into(),
                port: 8004,
                env: "test".into(),
                cors_origins: None,
                public_base_url: Some("https://portal.test.local".into()),
            },
            database: DatabaseConfig { url: "postgres://x".into(), max_connections: 5 },
            redis: RedisConfig { url: "redis://x".into() },
            kafka: KafkaConfig { brokers: "x".into(), group_id: "x".into() },
            geocoder: GeocoderConfig::default(),
            payments: Some(PaymentsConfig { url: "http://payments:8012".into() }),
            quote_token_secret: Some("shh".into()),
        };

        let enabled = cfg.payment_config().expect("all three set must enable the capability");
        assert_eq!(enabled.payments_url, "http://payments:8012");
        assert_eq!(enabled.quote_token_secret, "shh");
        assert_eq!(enabled.public_base_url, "https://portal.test.local");
    }
}
