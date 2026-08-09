-- Ingest provenance on items, and the split between "a machine touched this"
-- and "a human attested to it" on availability.
--
-- Why the split matters: before this, `item_availability.updated_at` was both
-- the audit stamp and the trust input. That is safe only while the sole writer
-- is a vendor tapping a button. The moment a POS or Shopify sync can write the
-- table, an overnight reconciliation would stamp NOW() on every row and the
-- whole catalog would read as freshly confirmed — the freshness model reporting
-- maximum confidence at exactly the moment it had none.

ALTER TABLE omnideliv.catalog_items
    ADD COLUMN IF NOT EXISTS source      TEXT NOT NULL DEFAULT 'manual',
    ADD COLUMN IF NOT EXISTS external_id TEXT,
    ADD COLUMN IF NOT EXISTS synced_at   TIMESTAMPTZ;

-- Closed set, deliberately. An unknown source is a typo that silently creates a
-- new provenance bucket nobody queries; adding a real one is a migration line,
-- which is honest, since it also needs an adapter to exist.
ALTER TABLE omnideliv.catalog_items
    DROP CONSTRAINT IF EXISTS catalog_items_source_check;
ALTER TABLE omnideliv.catalog_items
    ADD CONSTRAINT catalog_items_source_check
    CHECK (source IN ('manual','shopify','woocommerce','csv','pos'));

-- Idempotent re-sync: an adapter re-running must update the row it wrote last
-- time, not insert a duplicate. Scoped to (vendor, source) so the same product
-- id in two different vendors' Shopify stores does not collide, and a vendor
-- who later adds a second source does not fight over one key.
CREATE UNIQUE INDEX IF NOT EXISTS uq_catalog_external
    ON omnideliv.catalog_items (vendor_id, source, external_id)
    WHERE external_id IS NOT NULL;

-- The human attestation clock. NULL = nobody has ever confirmed this item.
ALTER TABLE omnideliv.item_availability
    ADD COLUMN IF NOT EXISTS confirmed_at TIMESTAMPTZ;

-- Backfill: every row that exists today was written by the vendor console or
-- the dev seed, both of which are human declarations. Copying updated_at across
-- preserves current behaviour exactly — without it, this migration would flip
-- every existing catalog to "never confirmed" and the mesh would start
-- substituting the entire seeded demo.
UPDATE omnideliv.item_availability
   SET confirmed_at = updated_at
 WHERE confirmed_at IS NULL;

-- The staleness sweep now asks "when did a human last look", so the index
-- follows the column the query actually filters on. The old one stays: it still
-- serves "when did the sync last run", which is the ops question.
CREATE INDEX IF NOT EXISTS idx_availability_confirmed
    ON omnideliv.item_availability (tenant_id, confirmed_at);
