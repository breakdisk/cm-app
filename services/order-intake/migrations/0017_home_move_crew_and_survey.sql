-- Migration: 0017 — the crew a home move needs, and its survey deposit
--
-- The crew is decided by the property size (lead + helpers), at least two
-- helpers a truck once there is more than one, plus the access rules. It is
-- what the accepting lead must bring, and what the customer is told.
--
-- The survey fee is a deposit credited toward the move. Cancelling more than
-- the refund cutoff before the survey returns it; after that, or once the
-- survey is done, it is kept (and owed to the lead who did or went to do it).

ALTER TABLE order_intake.home_moves
    ADD COLUMN IF NOT EXISTS trucks              INTEGER     NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS helpers             INTEGER     NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS crew_total          INTEGER     NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS large_estate        BOOLEAN     NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS international       BOOLEAN     NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS survey_cents        BIGINT      NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS survey_submitted_at TIMESTAMPTZ;
