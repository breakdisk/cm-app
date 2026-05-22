use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Shopify,
    Woocommerce,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Shopify      => "shopify",
            Platform::Woocommerce  => "woocommerce",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "shopify"     => Some(Platform::Shopify),
            "woocommerce" => Some(Platform::Woocommerce),
            _             => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectorCredentials {
    pub id:             Uuid,
    pub tenant_id:      Uuid,
    pub merchant_id:    Uuid,
    pub tenant_slug:    String,
    pub platform:       Platform,
    pub webhook_secret: String,
    pub config:         serde_json::Value,
    pub is_active:      bool,
    pub created_at:     DateTime<Utc>,
}

impl ConnectorCredentials {
    // ── Config accessors ────────────────────────────────────────────────────

    fn config_str(&self, key: &str) -> Option<&str> {
        self.config.get(key)?.as_str()
    }

    pub fn shopify_shop_domain(&self) -> Option<&str> {
        self.config_str("shop_domain")
    }

    pub fn shopify_admin_token(&self) -> Option<&str> {
        self.config_str("admin_api_token")
    }

    pub fn woo_store_url(&self) -> Option<&str> {
        self.config_str("store_url")
    }

    pub fn woo_consumer_key(&self) -> Option<&str> {
        self.config_str("consumer_key")
    }

    pub fn woo_consumer_secret(&self) -> Option<&str> {
        self.config_str("consumer_secret")
    }

    pub fn default_service_type(&self) -> &str {
        self.config_str("default_service_type").unwrap_or("standard")
    }

    pub fn default_weight_grams(&self) -> u32 {
        self.config
            .get("default_weight_grams")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32)
            .unwrap_or(500)
    }

    /// Shopify gateway names that should be treated as COD.
    /// Defaults to ["cash_on_delivery", "manual"] if not configured.
    pub fn shopify_cod_gateways(&self) -> Vec<&str> {
        match self.config.get("cod_gateways").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
            None      => vec!["cash_on_delivery", "manual"],
        }
    }

    /// WooCommerce payment_method slugs that should be treated as COD.
    /// Defaults to ["cod"] if not configured.
    pub fn woo_cod_payment_methods(&self) -> Vec<&str> {
        match self.config.get("cod_payment_methods").and_then(|v| v.as_array()) {
            Some(arr) => arr.iter().filter_map(|v| v.as_str()).collect(),
            None      => vec!["cod"],
        }
    }

    /// Derive the 3-char AWB tenant code from the stored tenant_slug.
    /// Mirrors the algorithm in order-intake's HTTP handler.
    pub fn derive_tenant_code(&self) -> String {
        self.tenant_slug
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() && *c != 'O' && *c != 'I')
            .take(3)
            .collect::<String>()
            .to_uppercase()
    }

    /// Extract origin address config as an AddressInput-compatible map.
    pub fn origin_address(&self) -> Option<&serde_json::Value> {
        self.config.get("origin_address")
    }
}
