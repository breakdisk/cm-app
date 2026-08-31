-- QR table ordering — ADR-0017.
--
-- A venue is separate from a vendor because a venue is a PLACE WITH TABLES and
-- a vendor is a BUSINESS THAT SELLS. A mall foodcourt is one venue with many
-- vendors; a standalone restaurant is one venue with one. Collapsing them would
-- make the foodcourt case unrepresentable, and the foodcourt is half the point.

CREATE TABLE IF NOT EXISTS omnideliv.venues (
    id         UUID        PRIMARY KEY,
    tenant_id  UUID        NOT NULL,
    name       TEXT        NOT NULL,
    kind       TEXT        NOT NULL DEFAULT 'standalone'
                           CHECK (kind IN ('standalone', 'foodcourt')),
    -- Opening hours, as [{"dow":1,"open":"09:00","close":"22:00"}, ...].
    -- JSONB rather than columns because a venue's week is irregular — split
    -- shifts, one late night, a closed Monday — and a column-per-day schema
    -- cannot express any of that without a second table nobody would populate.
    hours      JSONB       NOT NULL DEFAULT '[]',
    -- Hours above are LOCAL to the venue, and this is how local is resolved.
    --
    -- A fixed offset, not an IANA zone: both current markets are DST-free
    -- (Philippines UTC+8 year-round, UAE UTC+4), and pulling `chrono-tz` into
    -- the workspace rebuilds every service image for a feature neither market
    -- needs. Default 480 = UTC+8.
    --
    -- LIMITATION, stated rather than discovered: a venue in a DST-observing
    -- country will be an hour wrong for half the year. Switch this column to an
    -- IANA zone name before onboarding one — do not paper over it by having
    -- someone edit the offset twice a year.
    utc_offset_minutes INT  NOT NULL DEFAULT 480,
    status     TEXT        NOT NULL DEFAULT 'active'
                           CHECK (status IN ('active', 'paused', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_venue_tenant ON omnideliv.venues (tenant_id);

-- Which vendors sell at a venue. A join table even though a standalone venue
-- has exactly one: the foodcourt is the reason this table exists, and modelling
-- standalone as a nullable column on `venues` would have to be migrated away
-- the day the first foodcourt is onboarded.
CREATE TABLE IF NOT EXISTS omnideliv.venue_vendors (
    venue_id   UUID        NOT NULL REFERENCES omnideliv.venues(id) ON DELETE CASCADE,
    vendor_id  UUID        NOT NULL REFERENCES omnideliv.vendors(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (venue_id, vendor_id)
);

CREATE INDEX IF NOT EXISTS idx_venue_vendor_vendor ON omnideliv.venue_vendors (vendor_id);

CREATE TABLE IF NOT EXISTS omnideliv.tables (
    id         UUID        PRIMARY KEY,
    venue_id   UUID        NOT NULL REFERENCES omnideliv.venues(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    -- What is painted on the table: "A-14", "Window 3".
    label      TEXT        NOT NULL,
    -- The printed secret. Opaque, random, rotatable.
    --
    -- UNIQUE across the whole platform, not per venue: a scan resolves the
    -- token before it knows which venue it belongs to, and a per-venue unique
    -- would make that lookup ambiguous. It also means a rotated token can never
    -- collide with a live one somewhere else.
    token      TEXT        NOT NULL UNIQUE,
    status     TEXT        NOT NULL DEFAULT 'open'
                           CHECK (status IN ('open', 'closed')),
    -- When its current code was last put on paper. An operator rotating a token
    -- needs to know which tables still carry the old one.
    printed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE (venue_id, label)
);

CREATE INDEX IF NOT EXISTS idx_table_venue ON omnideliv.tables (venue_id);

-- An open party at a table.
--
-- `id` doubles as the synthetic `user_id` on the minted JWT, which is what lets
-- `orders.customer_id` stay NOT NULL for a diner who has no account. Nothing
-- downstream — tracking, legs, the ledger — needs to learn that anonymous
-- customers exist.
CREATE TABLE IF NOT EXISTS omnideliv.table_sessions (
    id         UUID        PRIMARY KEY,
    table_id   UUID        NOT NULL REFERENCES omnideliv.tables(id) ON DELETE CASCADE,
    venue_id   UUID        NOT NULL REFERENCES omnideliv.venues(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    -- Set when a person closes the tab or staff clears the table. A session is
    -- live while it is unended and unexpired; both conditions matter, because
    -- an abandoned session must age out on its own.
    ended_at   TIMESTAMPTZ
);

-- The concurrent-session cap counts live sessions for one table, so that is
-- what the index serves.
CREATE INDEX IF NOT EXISTS idx_table_session_live
    ON omnideliv.table_sessions (table_id, expires_at)
    WHERE ended_at IS NULL;
