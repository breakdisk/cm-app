-- Unattended catalog sync: opt-in per connector, claimed atomically.
--
-- Three columns rather than a cron somewhere else, for one reason: the schedule
-- has to be visible to every replica. The platform runs rolling updates, so two
-- connectors instances is the normal case, not the exception — an in-memory
-- timer would have both syncing the same vendor at the same moment, and a
-- deploy would restart the clock and re-sync everything.

ALTER TABLE connectors.credentials
    -- NULL means auto-sync is OFF, and that is deliberately the default.
    -- Nightly-overwriting a vendor's catalog is something they should ask for:
    -- the sync owns name, price and listing, so a vendor who has edited those
    -- in the console would silently lose the edits on the next tick.
    ADD COLUMN IF NOT EXISTS sync_interval_mins INT,
    ADD COLUMN IF NOT EXISTS last_synced_at     TIMESTAMPTZ,
    -- Kept so a sync that has been failing for a week is visible without
    -- reading logs. Cleared on success, so a stale message cannot outlive the
    -- fault it described.
    ADD COLUMN IF NOT EXISTS last_sync_error    TEXT;

-- A floor, not a preference. Below this a sweep hammers a merchant's
-- WordPress — which is usually far more fragile than Shopify's API — and no
-- catalog changes often enough to justify it.
ALTER TABLE connectors.credentials
    DROP CONSTRAINT IF EXISTS credentials_sync_interval_check;
ALTER TABLE connectors.credentials
    ADD CONSTRAINT credentials_sync_interval_check
    CHECK (sync_interval_mins IS NULL OR sync_interval_mins >= 15);

-- The sweep's only query: rows that are enabled, active, and due. Partial so
-- the index stays the size of the opted-in set rather than the whole table —
-- most connectors will never enable this.
CREATE INDEX IF NOT EXISTS idx_connector_creds_due
    ON connectors.credentials (last_synced_at NULLS FIRST)
    WHERE is_active = true AND sync_interval_mins IS NOT NULL;

COMMENT ON COLUMN connectors.credentials.sync_interval_mins
    IS 'Minutes between unattended catalog syncs. NULL disables it. Claiming is atomic (SELECT ... FOR UPDATE SKIP LOCKED), so multiple replicas are safe.';
