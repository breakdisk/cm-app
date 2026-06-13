use chrono::{DateTime, Utc};
use logisticos_types::TenantId;
use serde::{Deserialize, Serialize};

/// White-label branding for a tenant. Served to portals and mobile apps so they
/// can render the carrier's logo, palette, and legal copy at runtime — no
/// per-carrier rebuild required. Absence of a row, or a non-Enterprise tier,
/// resolves to [`Branding::default_logisticos`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branding {
    pub tenant_id:       TenantId,
    pub display_name:    String,
    pub app_tagline:     Option<String>,
    pub logo_url:        Option<String>,
    pub logo_dark_url:   Option<String>,
    pub favicon_url:     Option<String>,
    pub primary_color:   Option<String>,
    pub secondary_color: Option<String>,
    pub accent_color:    Option<String>,
    pub support_email:   Option<String>,
    pub support_phone:   Option<String>,
    /// Free-form legal copy: `{ "terms": "...", "privacy": "...", "splash": "..." }`.
    pub legal_text:      serde_json::Value,
    pub custom_domain:   Option<String>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Branding {
    /// The canonical default brand returned when a tenant has no branding row or
    /// is not white-label-entitled. Values mirror the design-system tokens
    /// (`packages/ui/src/design-system/tokens.ts`) so the platform looks
    /// identical to the hardcoded baseline.
    pub fn default_logisticos(tenant_id: TenantId) -> Self {
        let now = Utc::now();
        Self {
            tenant_id,
            display_name:    "LogisticOS".to_string(),
            app_tagline:     Some("AI Agentic Last-Mile Delivery".to_string()),
            logo_url:        None,
            logo_dark_url:   None,
            favicon_url:     None,
            primary_color:   Some("#00E5FF".to_string()), // cyan.neon
            secondary_color: Some("#A855F7".to_string()), // purple.plasma
            accent_color:    Some("#00FF88".to_string()), // green.signal
            support_email:   None,
            support_phone:   None,
            legal_text:      serde_json::json!({}),
            custom_domain:   None,
            created_at:      now,
            updated_at:      now,
        }
    }

    /// True when this branding is the synthetic default (not a persisted,
    /// tenant-customised row). Used by callers that want to signal "no custom
    /// brand" to clients.
    pub fn is_default(&self) -> bool {
        self.display_name == "LogisticOS" && self.logo_url.is_none()
    }

    /// Validate a single hex colour (`#RGB` or `#RRGGBB`). Returns the input on
    /// success so it can be used in a `.map(...).transpose()` chain.
    pub fn validate_hex(color: &str) -> Result<(), &'static str> {
        let bytes = color.as_bytes();
        let valid_len = bytes.len() == 4 || bytes.len() == 7;
        if !valid_len || bytes[0] != b'#' {
            return Err("color must be a hex string like #00E5FF");
        }
        if !bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
            return Err("color must contain only hex digits after '#'");
        }
        Ok(())
    }
}
