-- A category for a catalog item: "Mains", "Beverages", "Sides".
--
-- A real column rather than a key inside `vertical_attrs`. That JSONB is for
-- attributes whose *shape* differs per vertical — a pharmacy's dosage form has
-- nothing in common with a florist's stem count. Category is the opposite: the
-- same concept in every vertical, and the thing the storefront and the customer
-- app both need to GROUP BY. Grouping on a JSON key means no usable index and a
-- cast in every query that reads it.
--
-- NULL means uncategorised, which is a real state and not a defect: a CSV or
-- Shopify import brings no category, and inventing one ("Other") would put a
-- guess in front of a customer as though a person had made it.
ALTER TABLE omnideliv.catalog_items
    ADD COLUMN IF NOT EXISTS category TEXT;

-- Serves both "group my menu" in the console and the customer app's browse.
-- Partial on is_listed because an unlisted item is never browsed.
CREATE INDEX IF NOT EXISTS catalog_items_vendor_category
    ON omnideliv.catalog_items (tenant_id, vendor_id, category)
 WHERE is_listed = true;
