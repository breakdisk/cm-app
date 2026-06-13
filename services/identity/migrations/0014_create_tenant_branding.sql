-- White-label tenant branding. One row per tenant; absence (or a non-Enterprise
-- tier) means the tenant renders the default LogisticOS brand. Only Enterprise
-- tenants may write branding (entitlement enforced at the application layer via
-- SubscriptionTier::allows_white_label / Claims::can_use_white_label).
CREATE TABLE IF NOT EXISTS identity.tenant_branding (
    tenant_id        UUID        PRIMARY KEY REFERENCES identity.tenants(id) ON DELETE CASCADE,
    display_name     TEXT        NOT NULL,
    app_tagline      TEXT,
    logo_url         TEXT,                 -- light-background logo (R2/S3 URL)
    logo_dark_url    TEXT,                 -- dark-background logo
    favicon_url      TEXT,
    primary_color    VARCHAR(7),           -- "#0066FF"
    secondary_color  VARCHAR(7),
    accent_color     VARCHAR(7),
    support_email    TEXT,
    support_phone    TEXT,
    legal_text       JSONB       NOT NULL DEFAULT '{}',   -- { terms, privacy, splash }
    custom_domain    TEXT,                 -- nullable; activated in the custom-domain phase
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Custom-domain resolution must be unique across tenants (one host → one tenant).
CREATE UNIQUE INDEX IF NOT EXISTS idx_tenant_branding_custom_domain
    ON identity.tenant_branding (custom_domain)
    WHERE custom_domain IS NOT NULL;

ALTER TABLE identity.tenant_branding ENABLE ROW LEVEL SECURITY;

-- Defence-in-depth: queries always filter by tenant_id explicitly; this policy
-- mirrors the existing convention (see 0012_pickup_addresses) using the
-- app.current_tenant_id session GUC.
CREATE POLICY tenant_branding_tenant_isolation ON identity.tenant_branding
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
