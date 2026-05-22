-- Add connector integration fields to shipments.
-- source_platform: which e-commerce platform originated this shipment (shopify, woocommerce, api, etc.)
-- external_order_id: platform-specific order ID for deduplication and cross-reference.
--
-- Note: merchant_reference already exists (added in 0001) but was never included in
-- SHIPMENT_COLS — it is now wired in the application layer (no schema change needed).

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS source_platform    TEXT,
    ADD COLUMN IF NOT EXISTS external_order_id  TEXT;

-- Composite index supports: lookup by platform + dedup by external ID per tenant
CREATE INDEX IF NOT EXISTS idx_shipments_connector
    ON order_intake.shipments (tenant_id, source_platform, external_order_id)
    WHERE external_order_id IS NOT NULL;

COMMENT ON COLUMN order_intake.shipments.source_platform
    IS 'Origin platform: shopify | woocommerce | api | null (direct API creation)';
COMMENT ON COLUMN order_intake.shipments.external_order_id
    IS 'Platform-native order ID (e.g. Shopify order.id or WooCommerce order.number). Used for deduplication.';
