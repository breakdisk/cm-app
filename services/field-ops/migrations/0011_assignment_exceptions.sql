-- A delivery that could not be completed.
--
-- WHY A ROW, WHEN `Arrived` IS PUBLISHED AND NEVER PERSISTED.
-- The milestone events are informational: they change no state, and anything
-- that missed one can reconcile from the next. An exception is the opposite.
-- It is the start of work somebody has to finish — a refund decision, a return
-- leg, a re-dispatch — and the courier who raised it has already walked away.
-- A published-only exception would be a task that exists for as long as the
-- topic's retention window and then silently stops existing.
--
-- WHY THIS DOES NOT TOUCH courier_assignments.status.
-- Decided 2026-08-30 (D2). The courier's report is a claim, not a verdict: the
-- goods are still in their bag and the money question is unanswered. Closing
-- the assignment here would credit or strand real money on one tap from a
-- phone that may be offline and retrying. `resolved_at IS NULL` is the open
-- queue; ops closes it, and Phase 2 is what moves anything.
--
-- WHY client_ref IS NOT NULL AND UNIQUE PER ASSIGNMENT.
-- The courier app queues writes offline and replays them, so this endpoint
-- WILL be called twice with the same intent. Without a client-supplied key,
-- the retry that a flaky connection guarantees becomes a second open exception
-- for ops to triage. The app generates it once, at the moment the courier taps
-- confirm, and reuses it for every replay of that same tap.
--
-- Dual timestamps per the platform's device_timestamp contract: the hardware
-- clock at the tap, and the cluster clock at receipt. SLA and response-time
-- questions use device_timestamp where it is present.
CREATE TABLE IF NOT EXISTS field_ops.assignment_exceptions (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    assignment_id     UUID        NOT NULL REFERENCES field_ops.courier_assignments(id),
    courier_id        UUID        NOT NULL REFERENCES field_ops.couriers(id),

    -- Closed set, validated in Rust before it reaches here. TEXT rather than a
    -- Postgres enum so adding a reason is a deploy, not a migration with a
    -- lock — the set is expected to grow as ops learns what it is triaging.
    reason            TEXT        NOT NULL,
    note              TEXT,

    -- D4: where the goods ended up, in the courier's own words, until a return
    -- leg exists to model it properly.
    goods_disposition TEXT,

    capture_lat       DOUBLE PRECISION,
    capture_lng       DOUBLE PRECISION,
    client_ref        UUID        NOT NULL,

    device_timestamp  TIMESTAMPTZ,
    server_timestamp  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- Written by Phase 2 only. Present now because `resolved_at IS NULL` is
    -- what makes the open queue a query rather than a scan of everything.
    resolved_at       TIMESTAMPTZ,
    resolved_by       UUID,
    resolution        TEXT
);

-- The idempotency guarantee the offline queue depends on.
CREATE UNIQUE INDEX IF NOT EXISTS assignment_exceptions_client_ref_key
    ON field_ops.assignment_exceptions (assignment_id, client_ref);

-- The ops queue: open exceptions for a tenant, oldest first, because the
-- longest-waiting customer is the one to answer next.
CREATE INDEX IF NOT EXISTS assignment_exceptions_open_idx
    ON field_ops.assignment_exceptions (tenant_id, server_timestamp)
    WHERE resolved_at IS NULL;
