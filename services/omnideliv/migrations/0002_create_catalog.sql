CREATE TABLE IF NOT EXISTS omnideliv.catalog_items (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    vendor_id      UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    sku            TEXT        NOT NULL,
    name           TEXT        NOT NULL,
    description    TEXT,
    price_cents    BIGINT      NOT NULL CHECK (price_cents >= 0),
    -- Size/extras/options. Shape varies per vertical, so JSONB rather than a
    -- normalised modifier table we would have to reshape per vertical.
    modifiers      JSONB       NOT NULL DEFAULT '[]',
    -- Allergen and dietary tags drive the Nutritionist's filtering. Arrays, not
    -- JSONB, because they are queried with `&&` (overlap) on the hot path.
    allergens      TEXT[]      NOT NULL DEFAULT '{}',
    dietary_tags   TEXT[]      NOT NULL DEFAULT '{}',
    -- Per-vertical extras (Rx schedule, floral stem count, retail dimensions).
    vertical_attrs JSONB       NOT NULL DEFAULT '{}',
    is_listed      BOOLEAN     NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_catalog_vendor_sku
    ON omnideliv.catalog_items (vendor_id, sku);

CREATE INDEX IF NOT EXISTS idx_catalog_vendor_listed
    ON omnideliv.catalog_items (tenant_id, vendor_id)
    WHERE is_listed;

-- Allergen exclusion is a filter on nearly every Nutritionist query.
CREATE INDEX IF NOT EXISTS idx_catalog_allergens
    ON omnideliv.catalog_items USING GIN (allergens);

-- Availability is a separate table, not a column on catalog_items, for one
-- reason: it is written far more often than the item it describes. A vendor
-- toggling stock all day must not churn the item row (and its GIN index).
--
-- updated_at is LOAD-BEARING, not bookkeeping. Stock here is vendor-declared,
-- so the age of the declaration is what tells the agent how much to trust it.
CREATE TABLE IF NOT EXISTS omnideliv.item_availability (
    item_id    UUID        PRIMARY KEY REFERENCES omnideliv.catalog_items(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    state      TEXT        NOT NULL DEFAULT 'available'
                           CHECK (state IN ('available','limited','out_of_stock')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID
);

CREATE INDEX IF NOT EXISTS idx_availability_stale
    ON omnideliv.item_availability (tenant_id, updated_at);
