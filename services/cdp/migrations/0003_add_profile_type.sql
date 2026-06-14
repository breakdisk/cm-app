-- CDP: add profile_type to distinguish senders from receivers.
-- Senders are the merchant's shipping clients (marketing targets).
-- Receivers are end consignees (delivery recipients).

ALTER TABLE cdp.customer_profiles
    ADD COLUMN IF NOT EXISTS profile_type TEXT NOT NULL DEFAULT 'unknown';

CREATE INDEX IF NOT EXISTS cdp_profiles_type
    ON cdp.customer_profiles (tenant_id, profile_type);

-- Phone lookup index for sender deduplication on shipment creation.
CREATE INDEX IF NOT EXISTS cdp_profiles_phone
    ON cdp.customer_profiles (tenant_id, phone)
    WHERE phone IS NOT NULL;
