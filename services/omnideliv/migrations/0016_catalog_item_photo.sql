-- A product photo for a catalog item.
--
-- Stored as an object key, not a URL. The bucket endpoint is cluster-internal
-- (minio has no published port and no Traefik route), so a stored URL would be
-- unreachable from a browser and would also go stale the moment the backing
-- store changes. The service resolves the key and streams the bytes.
--
-- NULL means no photo. Deliberately not defaulted to a placeholder path: a
-- placeholder that looks like a real key is indistinguishable from a broken
-- upload when the object is missing.
ALTER TABLE omnideliv.catalog_items
    ADD COLUMN IF NOT EXISTS image_key TEXT;

-- Only the photo endpoint writes this column; the catalog upsert leaves it
-- alone on purpose, so re-syncing a Shopify or CSV catalog cannot wipe a photo
-- the vendor uploaded by hand. This index serves the "which of my items still
-- have no picture" question the storefront asks.
CREATE INDEX IF NOT EXISTS catalog_items_missing_photo
    ON omnideliv.catalog_items (tenant_id, vendor_id)
 WHERE image_key IS NULL;
