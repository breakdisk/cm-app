-- Payment on a marketplace (spot truck) booking, and the merchant who placed it.
--
-- Until now `carrier.marketplace_bookings` had a `quoted_price_cents` and no
-- payment of any kind, because nothing created a booking at all: the carrier
-- side could list, accept, reject and record a pickup on rows that no code path
-- could produce. `create_booking` existed on the repository and had zero
-- callers. The buy side is what was missing, and a price nobody pays is not a
-- product.
--
-- `booked_by_user_id` is why a booking can be listed back to the merchant who
-- made it. Nullable, because every row that existed before this migration was
-- created by nothing and belongs to nobody.
ALTER TABLE carrier.marketplace_bookings
    ADD COLUMN IF NOT EXISTS booked_by_user_id UUID,

    -- 'invoice' bills the merchant on their existing invoice run and moves the
    -- booking straight to the carrier, which is what the carrier-side handlers
    -- have always implicitly assumed. 'online' opens an authorization hold on a
    -- card first.
    ADD COLUMN IF NOT EXISTS payment_method TEXT NOT NULL DEFAULT 'invoice'
        CHECK (payment_method IN ('invoice', 'online')),

    -- Always 'pending' for an 'invoice' booking: no gateway is involved.
    ADD COLUMN IF NOT EXISTS payment_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (payment_status IN ('pending', 'authorized', 'captured', 'voided', 'failed')),

    ADD COLUMN IF NOT EXISTS payment_intent_id UUID,
    ADD COLUMN IF NOT EXISTS payment_checkout_url TEXT,

    -- When the hold landed. This, and not created_at, is the clock the
    -- carrier-response window counts from: created_at predates authorization by
    -- however long the merchant spent on the hosted card page, and a booking is
    -- not offered to the carrier at all until the money is ring-fenced.
    ADD COLUMN IF NOT EXISTS payment_authorized_at TIMESTAMPTZ,

    -- Copied off the listing at booking time. The listing's own value can be
    -- edited afterwards, and a window that moves under a booking already placed
    -- is a window nobody can hold anyone to.
    ADD COLUMN IF NOT EXISTS response_window_mins INTEGER NOT NULL DEFAULT 15,

    -- What the price was computed from, so a dispute has the inputs and not
    -- just the answer. The rate card itself lives on the listing and is
    -- mutable; these two are the quantities the merchant declared.
    ADD COLUMN IF NOT EXISTS distance_km REAL NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_marketplace_bookings_booked_by
    ON carrier.marketplace_bookings (booked_by_user_id, created_at DESC);

-- The response-window sweep: online bookings holding an authorization that no
-- carrier has answered yet. Partial, because that is a small slice of a table
-- that is mostly finished bookings.
CREATE INDEX IF NOT EXISTS idx_marketplace_bookings_awaiting_response
    ON carrier.marketplace_bookings (payment_authorized_at)
    WHERE status = 'pending' AND payment_method = 'online' AND payment_status = 'authorized';

COMMENT ON COLUMN carrier.marketplace_bookings.payment_checkout_url IS
  'NI hosted-checkout page for this booking''s authorization. Disclosed only to '
  'the merchant who placed the booking, and only while payment_status = pending.';
