CREATE SCHEMA IF NOT EXISTS connectors;

CREATE TABLE IF NOT EXISTS connectors.credentials (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    merchant_id    UUID        NOT NULL,
    tenant_slug    TEXT        NOT NULL,
    platform       TEXT        NOT NULL,  -- 'shopify' | 'woocommerce'
    webhook_secret TEXT        NOT NULL,  -- HMAC-SHA256 signing secret (store-specific)
    config         JSONB       NOT NULL DEFAULT '{}',
    -- Shopify config keys:
    --   shop_domain:          "myshop.myshopify.com"
    --   admin_api_token:      "shpat_xxx"         (for fulfillment push-back)
    --   default_service_type: "standard"
    --   cod_gateways:         ["cash_on_delivery", "manual"]
    --   origin_address:       { line1, city, province, postal_code, country_code }
    --   default_weight_grams: 500
    -- WooCommerce config keys:
    --   store_url:            "https://mystore.com"
    --   consumer_key:         "ck_xxx"
    --   consumer_secret:      "cs_xxx"
    --   default_service_type: "standard"
    --   cod_payment_methods:  ["cod"]
    --   origin_address:       { line1, city, province, postal_code, country_code }
    --   default_weight_grams: 500
    is_active      BOOL        NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT uq_connector_tenant_platform UNIQUE (tenant_id, platform)
);

CREATE INDEX IF NOT EXISTS idx_connector_creds_tenant
    ON connectors.credentials (tenant_id, platform)
    WHERE is_active = true;

COMMENT ON TABLE connectors.credentials
    IS 'Per-tenant per-platform connector configuration: HMAC secrets, API tokens, and platform-specific settings.';
