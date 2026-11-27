-- Replay protection for vendor leg actions — ADR-0017 decision 8.
--
-- The guarded UPDATE in `leg_repo` already makes a duplicate transition a
-- no-op, and that covers two staff tapping Accept on two tablets: both are
-- first attempts, they carry different keys, and the loser is told the leg is
-- accepted.
--
-- This table covers the other failure, which the guard cannot: one request
-- arriving twice because a kitchen's Wi-Fi dropped the response. Today that is
-- harmless. It stops being harmless when acceptance triggers a payment capture,
-- and the endpoint contract should not change on the day money starts moving.

CREATE TABLE IF NOT EXISTS omnideliv.vendor_action_idempotency (
    tenant_id  UUID        NOT NULL,
    vendor_id  UUID        NOT NULL,
    key        TEXT        NOT NULL,
    leg_id     UUID        NOT NULL REFERENCES omnideliv.order_vendor_legs(id) ON DELETE CASCADE,
    action     TEXT        NOT NULL,
    response   JSONB       NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Keyed per vendor, not globally: two stores choosing the same key must not
    -- collide, and a store cannot probe another's keys by guessing.
    PRIMARY KEY (tenant_id, vendor_id, key)
);

-- Supports the sweep that ages these out. A replay older than a day is a new
-- request in practice, and without a sweep this grows for the life of the
-- platform.
CREATE INDEX IF NOT EXISTS idx_vendor_idem_created
    ON omnideliv.vendor_action_idempotency (created_at);
