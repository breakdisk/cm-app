use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app: AppConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    pub order_intake: OrderIntakeConfig,
    #[serde(default)]
    pub identity: IdentityConfig,
    #[serde(default)]
    pub subscription: SubscriptionConfig,
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

/// Identity, for granting a tenant the tier a subscription payment bought.
///
/// Env: `IDENTITY__URL`, `IDENTITY__INTERNAL_SECRET`. Both empty disables the
/// tier grant entirely -- subscriptions can still be sold and recorded, and
/// `subscriptions.tier_synced_at` stays NULL so the retry sweep keeps the
/// unpaid-for entitlement visible rather than losing it.
#[derive(Debug, Deserialize, Clone, Default)]
pub struct IdentityConfig {
    #[serde(default)]
    pub url: String,
    /// The same shared secret identity already uses for
    /// `/v1/internal/auth/exchange-firebase`. Not a JWT: the tenant-facing tier
    /// route needs `tenants:manage`, which no role holds, and minting a token
    /// that cleared it would recreate the free self-upgrade this design avoids.
    #[serde(default)]
    pub internal_secret: String,
}

/// Where a merchant's browser lands after paying for a plan.
#[derive(Debug, Deserialize, Clone)]
pub struct SubscriptionConfig {
    #[serde(default = "default_subscription_return_url")]
    pub return_url_base: String,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self { return_url_base: default_subscription_return_url() }
    }
}

fn default_subscription_return_url() -> String {
    "https://os.cargomarket.net/settings/billing/return".into()
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
    /// Shared secret used to verify inbound webhook signatures via HMAC-SHA256
    /// over the raw body (the `x-ni-signature` header). This is one of two
    /// mutually exclusive webhook verification modes — see
    /// `webhook_header_key`/`webhook_header_value` for the other. Optional
    /// because a merchant using the static-header mode has no such secret;
    /// `Config::load` refuses to boot if *neither* mode ends up configured
    /// (see `NetworkInternationalConfig::has_webhook_verification_mode`).
    #[serde(default)]
    pub webhook_secret: Option<String>,
    /// NI outlet reference this tenant's charges post against.
    pub outlet_ref: String,
    /// Static header NI attaches to every webhook call, configured
    /// merchant-side in NI's portal as a "Header Key" / "Header Value" pair
    /// (shared-secret header auth, not a body signature). When both of
    /// these are set, this is the verification mode used instead of HMAC —
    /// see `verify_webhook` in `network_international.rs`.
    #[serde(default)]
    pub webhook_header_key: Option<String>,
    #[serde(default)]
    pub webhook_header_value: Option<String>,
    /// Optional AES-256-CBC key NI encrypts the webhook body with, when the
    /// merchant has set an "Encryption Key" in NI's portal (a setting
    /// independent of, and orthogonal to, the header/HMAC verification mode
    /// above — verification proves the caller is NI, encryption is a
    /// separate confidentiality layer on the body those checks cover).
    /// Per NI's webhook-encryption-decryption-guide: exactly 32 ASCII
    /// characters (AES-256), the ciphertext is base64 with a 16-byte IV
    /// prepended, PKCS5/PKCS7 padded. `Config::load` refuses to boot if this
    /// is set but not exactly 32 characters — see the length check there.
    #[serde(default)]
    pub webhook_encryption_key: Option<String>,
}

impl NetworkInternationalConfig {
    /// Whether this config specifies a webhook verification mode
    /// `verify_webhook` can actually use — either the static header pair or
    /// the HMAC secret. `Config::load` calls this and refuses to boot if it
    /// is false: accepting NI webhooks with **no** verification (silently
    /// trusting whatever any caller POSTs to the public webhook route) would
    /// be strictly worse than the original bug this file already guards
    /// against (crash-looping the whole service on a partial credential
    /// set), so the same "fail loud at boot" policy applies here.
    ///
    /// An empty string counts the same as absent, not "configured with a
    /// blank value" — matching the existing convention for optional secrets
    /// in this codebase (`order-intake`'s `GeocoderConfig::mapbox_access_token`,
    /// checked with `Some(token) if !token.is_empty()` at its call site in
    /// `services/order-intake/src/bootstrap.rs`). This matters here
    /// specifically because docker-compose's `${NI_WEBHOOK_HEADER_KEY:-}`
    /// substitution sets the container env var to an empty string, not an
    /// absent one, when the host variable is unset — without this, a
    /// default compose deployment with only `WEBHOOK_SECRET` set would have
    /// `webhook_header_key`/`webhook_header_value` both `Some("")`, and
    /// `verify_webhook` would take the (broken, empty-name) header branch
    /// instead of the working HMAC one.
    pub fn has_webhook_verification_mode(&self) -> bool {
        self.webhook_secret().is_some() || self.webhook_header_pair().is_some()
    }

    /// The HMAC secret, if meaningfully set (present and non-empty) — see
    /// the empty-string note on `has_webhook_verification_mode`.
    /// `verify_webhook` uses this instead of the raw field.
    pub fn webhook_secret(&self) -> Option<&str> {
        self.webhook_secret.as_deref().filter(|s| !s.is_empty())
    }

    /// The static header key/value pair, if both halves are meaningfully
    /// set (present and non-empty) — see the empty-string note on
    /// `has_webhook_verification_mode`. `verify_webhook` uses this instead
    /// of the raw fields.
    pub fn webhook_header_pair(&self) -> Option<(&str, &str)> {
        let key = self.webhook_header_key.as_deref().filter(|s| !s.is_empty())?;
        let value = self.webhook_header_value.as_deref().filter(|s| !s.is_empty())?;
        Some((key, value))
    }

    /// The webhook body encryption key, if meaningfully set (present and
    /// non-empty) — same empty-string-means-absent convention as
    /// `webhook_secret`/`webhook_header_pair` above, for the same
    /// docker-compose `${VAR:-}` reason. `verify_webhook` uses this instead
    /// of the raw field.
    pub fn webhook_encryption_key(&self) -> Option<&str> {
        self.webhook_encryption_key.as_deref().filter(|s| !s.is_empty())
    }
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
        let cfg: Config = config::Config::builder()
            .add_source(config::Environment::default().separator("__"))
            .build()?
            .try_deserialize()?;

        // Enforced here, not at `NetworkInternationalGateway::new` /
        // `NetworkInternationalGateway::verify_webhook`, for the same reason
        // the partial-credential-set case above already hard-fails in this
        // function: this is the earliest choke point (before the DB pool,
        // Kafka producer, or `axum::serve` even start), it's the one place
        // that already owns "is NI configured enough to be safe" validation,
        // and it keeps the check reachable by a plain config-level unit test
        // instead of requiring bootstrap wiring or a live HTTP call to
        // exercise.
        if let Some(ni) = &cfg.network_international {
            if !ni.has_webhook_verification_mode() {
                anyhow::bail!(
                    "Network International is configured (NETWORK_INTERNATIONAL__BASE_URL / \
                     API_KEY / OUTLET_REF are set) but no webhook verification mode is: set \
                     NETWORK_INTERNATIONAL__WEBHOOK_SECRET (HMAC-signature mode), or both \
                     NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY and \
                     NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE (static-header mode). \
                     Booting with neither would accept every inbound NI webhook unverified — \
                     refusing to start instead."
                );
            }

            // Same "fail loud at boot" policy as the verification-mode check
            // above, for the same reason: a wrong-length encryption key
            // would silently fail to decrypt *every* real webhook at
            // runtime (caught late, per-request, as a decrypt error) rather
            // than refusing to boot once, up front. NI's
            // webhook-encryption-decryption-guide specifies AES-256-CBC,
            // which requires an exactly-32-ASCII-character key.
            if let Some(key) = ni.webhook_encryption_key() {
                if key.len() != 32 {
                    anyhow::bail!(
                        "NETWORK_INTERNATIONAL__WEBHOOK_ENCRYPTION_KEY is set but is {} \
                         characters long, not the 32 AES-256 requires. A wrong-length key \
                         would fail to decrypt every real webhook at runtime — refusing to \
                         start instead.",
                        key.len()
                    );
                }
            }
        }

        Ok(cfg)
    }
}

#[cfg(test)]
mod config_tests {
    use super::*;

    /// The load-bearing regression test for both the original crash-loop fix
    /// and the header/HMAC dual-mode change, combined into **one** `#[test]`
    /// fn: every assertion here mutates process-wide env, and separate
    /// `#[test]` fns touching these same `NETWORK_INTERNATIONAL__*` /
    /// `APP__*` / etc. vars would race each other under `cargo test`'s
    /// default parallelism — same reasoning as
    /// `services/identity/.../auth_service.rs::dev_otp_gate_tests`. (This
    /// was learned the hard way: an earlier version of this change split
    /// case 1 below into its own `#[test]` fn, which non-deterministically
    /// failed both that test and the original entirely-unset test above by
    /// racing on the shared env vars.)
    ///
    /// Cases, in order:
    /// 1. No `NETWORK_INTERNATIONAL__*` vars set at all -> boots, `None`.
    ///    Reproduces the exact crash-loop being fixed (`Config::load()`
    ///    erroring out of `bootstrap::run()` before `axum::serve` ever
    ///    binds, taking invoicing/COD/wallets down with it).
    /// 2. `BASE_URL`/`API_KEY`/`OUTLET_REF` set, neither webhook
    ///    verification mode set -> must refuse to boot rather than silently
    ///    accept unverified webhooks.
    /// 3. + `WEBHOOK_SECRET` only -> boots (HMAC mode sufficient alone).
    /// 4. `WEBHOOK_SECRET` removed, header key+value set instead -> boots
    ///    (header mode sufficient alone).
    /// 5. Header value removed, only the key remains -> refuses to boot
    ///    again (half a pair is still neither mode).
    #[test]
    fn network_international_config_gates_boot_on_a_webhook_verification_mode() {
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
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY");
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE");
        }

        // Case 1: NI entirely unset.
        let entirely_unset = Config::load();

        // Case 2: NI's non-verification fields set, neither verification
        // mode configured -> must refuse to boot.
        unsafe {
            std::env::set_var("NETWORK_INTERNATIONAL__BASE_URL", "https://example.invalid");
            std::env::set_var("NETWORK_INTERNATIONAL__API_KEY", "key");
            std::env::set_var("NETWORK_INTERNATIONAL__OUTLET_REF", "outlet-1");
        }
        let neither_mode = Config::load();

        // Case 3: HMAC mode only.
        unsafe {
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_SECRET", "shh");
        }
        let hmac_only = Config::load();

        // Case 4: header pair only (HMAC secret removed).
        unsafe {
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_SECRET");
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY", "X-Merchant-Secret");
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE", "shh");
        }
        let header_only = Config::load();

        // Case 5: only half the header pair set -> still neither mode.
        unsafe {
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE");
        }
        let half_header_pair = Config::load();

        // Case 6: header pair restored to a valid verification mode, plus a
        // 31-character encryption key -> must refuse to boot on the key
        // length specifically, not the (now valid again) verification mode.
        unsafe {
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE", "shh");
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_ENCRYPTION_KEY", "a".repeat(31));
        }
        let bad_encryption_key_length = Config::load();

        // Case 7: same, but the encryption key is exactly 32 characters -> boots.
        unsafe {
            std::env::set_var("NETWORK_INTERNATIONAL__WEBHOOK_ENCRYPTION_KEY", "a".repeat(32));
        }
        let good_encryption_key_length = Config::load();

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
            std::env::remove_var("NETWORK_INTERNATIONAL__BASE_URL");
            std::env::remove_var("NETWORK_INTERNATIONAL__API_KEY");
            std::env::remove_var("NETWORK_INTERNATIONAL__OUTLET_REF");
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_KEY");
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_HEADER_VALUE");
            std::env::remove_var("NETWORK_INTERNATIONAL__WEBHOOK_ENCRYPTION_KEY");
        }

        let cfg = entirely_unset.expect(
            "Config::load() must succeed with every NETWORK_INTERNATIONAL__* var \
             unset — before this change this errored with \"missing field \
             `network_international`\" and crash-looped the whole service",
        );
        assert!(
            cfg.network_international.is_none(),
            "absent NI env vars must deserialize to None, not error and not \
             fabricate a config with empty-string fields"
        );

        let err = neither_mode.expect_err(
            "Config::load() must refuse to boot when NI is configured with no webhook \
             verification mode at all -- accepting unverified webhooks is worse than \
             the crash-loop case 1 above already guards against",
        );
        assert!(
            err.to_string().contains("webhook verification mode"),
            "error should name the actual problem, got: {err}"
        );

        let cfg = hmac_only.expect("HMAC secret alone must be a sufficient verification mode");
        assert!(cfg.network_international.unwrap().has_webhook_verification_mode());

        let cfg = header_only.expect("header pair alone must be a sufficient verification mode");
        assert!(cfg.network_international.unwrap().has_webhook_verification_mode());

        let err = half_header_pair.expect_err(
            "a header key with no matching value must not count as a configured mode",
        );
        assert!(err.to_string().contains("webhook verification mode"));

        let err = bad_encryption_key_length.expect_err(
            "a 31-character encryption key must refuse to boot -- a wrong-length key \
             would otherwise fail to decrypt every real webhook at runtime instead of \
             failing once, loudly, at startup",
        );
        assert!(
            err.to_string().contains("32"),
            "error should name the actual problem (wrong key length), got: {err}"
        );

        let cfg = good_encryption_key_length
            .expect("a 32-character encryption key must be accepted at boot");
        assert_eq!(
            cfg.network_international.unwrap().webhook_encryption_key().map(str::len),
            Some(32),
        );
    }
}
