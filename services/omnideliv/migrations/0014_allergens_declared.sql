-- Separate "no allergens" from "nobody said".
--
-- `allergens TEXT[] NOT NULL DEFAULT '{}'` conflates two very different facts:
-- an item a vendor has confirmed contains none of the listed allergens, and an
-- item whose allergen field nobody ever filled in. They are stored identically.
--
-- The consequence is not cosmetic. A customer says "no peanuts"; the filter
-- excludes items whose `allergens` array contains peanuts; an undeclared peanut
-- dish has an empty array, passes the filter, and is proposed as safe. The
-- system reports that it screened the basket. It did not — it screened the
-- items somebody had bothered to describe.
--
-- NULL here means never declared. A timestamp means a vendor asserted the
-- contents at that moment, and an empty array alongside it now genuinely means
-- "contains none of them" rather than "unknown".
--
-- Deliberately nullable with no backfill. Every existing row is honestly
-- undeclared, and stamping them with a date would manufacture an attestation
-- nobody made — which is the precise failure this column exists to end.
ALTER TABLE omnideliv.catalog_items
    ADD COLUMN IF NOT EXISTS allergens_declared_at TIMESTAMPTZ;

COMMENT ON COLUMN omnideliv.catalog_items.allergens_declared_at IS
  'When a vendor last asserted this item''s allergen contents. NULL = never '
  'declared, which is NOT the same as "no allergens": reconcile refuses to '
  'serve an undeclared item to a customer who stated an allergy.';

CREATE INDEX IF NOT EXISTS idx_catalog_allergens_undeclared
    ON omnideliv.catalog_items (tenant_id, vendor_id)
    WHERE allergens_declared_at IS NULL;
