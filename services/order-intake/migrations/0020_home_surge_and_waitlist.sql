-- Migration: 0020 — surge on a move window, and the priority waitlist
--
-- Surge: a window whose remaining teams fall below the threshold (20% by
-- default) is priced 1.2x–1.5x. The customer saw the multiplier before
-- booking; the surge is passed to the lead whole, as a High-Demand Bonus
-- (inside lead_payout_cents, and named here).
--
-- Waitlist: when every window is full, a customer queues for a date, first
-- come first served. When a window opens the next in line is offered it with
-- a 5-minute hold that counts against capacity for everyone else. A hold not
-- claimed in time lapses and the next in line is offered.

ALTER TABLE order_intake.home_moves
    ADD COLUMN IF NOT EXISTS surge_bps        INTEGER NOT NULL DEFAULT 10000,
    ADD COLUMN IF NOT EXISTS surge_cents      BIGINT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS lead_bonus_cents BIGINT  NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS order_intake.home_waitlist (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    account_id      UUID        NOT NULL,
    -- The priced move (the quote's payload), re-priced when a window is offered.
    quote           JSONB       NOT NULL,
    -- The local date the customer wants.
    wanted_date     DATE        NOT NULL,
    large_estate    BOOLEAN     NOT NULL,
    international   BOOLEAN     NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'waiting'
                    CHECK (status IN ('waiting', 'offered', 'booked', 'lapsed', 'withdrawn')),
    offered_move_at TIMESTAMPTZ,
    hold_expires_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One live place per customer per date.
CREATE UNIQUE INDEX IF NOT EXISTS home_waitlist_one_live
    ON order_intake.home_waitlist (account_id, wanted_date) WHERE status IN ('waiting', 'offered');
-- The queue, in order.
CREATE INDEX IF NOT EXISTS home_waitlist_queue
    ON order_intake.home_waitlist (tenant_id, wanted_date, created_at) WHERE status = 'waiting';
-- Holds that count against capacity.
CREATE INDEX IF NOT EXISTS home_waitlist_holds
    ON order_intake.home_waitlist (tenant_id, hold_expires_at) WHERE status = 'offered';
