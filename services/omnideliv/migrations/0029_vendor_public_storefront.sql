-- A vendor's public, shareable storefront.
--
-- Until now the only customer-facing view of a catalog was the native app and
-- the table-QR diner page. A vendor had no link they could put in an Instagram
-- bio, print on a takeaway counter, or hand to a customer -- and no way to serve
-- their menu on their own domain.
--
-- Three new pieces of identity, all optional and all opt-in.

ALTER TABLE omnideliv.vendors
    -- The public handle. `/s/kanto-freestyle`.
    ADD COLUMN IF NOT EXISTS slug           TEXT,
    -- A domain the vendor points at us with a CNAME, e.g. `menu.kanto.ph`.
    -- Stored lowercase; a Host header is case-insensitive.
    ADD COLUMN IF NOT EXISTS custom_domain  TEXT,
    -- One line under the name, and the description a social card shows.
    ADD COLUMN IF NOT EXISTS tagline        TEXT,
    -- **Opt-in, and false for every vendor that already exists.**
    --
    -- Publishing is a decision, not a default. Flipping this on for the whole
    -- table would put every catalog on the platform -- prices included -- onto
    -- the open internet, retroactively, without anybody asking for it.
    ADD COLUMN IF NOT EXISTS public_enabled BOOLEAN NOT NULL DEFAULT FALSE;

-- UNIQUE PLATFORM-WIDE, not per tenant.
--
-- Same reasoning as `tables.token`, and the second instance of that exception:
-- a public URL and a Host header are both resolved BEFORE any tenant is known.
-- The tenant is an OUTPUT of these lookups, so a per-tenant unique would make
-- them ambiguous. See the note on `VendorRepository::find_public_storefront`.
--
-- Partial, so the columns stay NULL for the vendors that never publish.
CREATE UNIQUE INDEX IF NOT EXISTS idx_vendor_slug
    ON omnideliv.vendors (slug)
    WHERE slug IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_vendor_custom_domain
    ON omnideliv.vendors (lower(custom_domain))
    WHERE custom_domain IS NOT NULL;

-- A slug is in a URL and a Host is in DNS, so both are constrained to what
-- those actually permit: lowercase alphanumerics and hyphens, no leading or
-- trailing hyphen. Enforced here as well as in the domain type, because this
-- column is reachable by any future writer and a bad value here is a broken
-- public page rather than a rejected request.
ALTER TABLE omnideliv.vendors
    DROP CONSTRAINT IF EXISTS vendors_slug_shape;
ALTER TABLE omnideliv.vendors
    ADD CONSTRAINT vendors_slug_shape
    CHECK (slug IS NULL OR slug ~ '^[a-z0-9]([a-z0-9-]{1,48}[a-z0-9])$');

ALTER TABLE omnideliv.vendors
    DROP CONSTRAINT IF EXISTS vendors_custom_domain_shape;
ALTER TABLE omnideliv.vendors
    ADD CONSTRAINT vendors_custom_domain_shape
    CHECK (custom_domain IS NULL OR custom_domain ~ '^[a-z0-9.-]{4,253}$');
