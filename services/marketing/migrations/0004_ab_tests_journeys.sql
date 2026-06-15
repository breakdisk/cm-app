-- Phase 4: Journey Builder tables
-- Note: marketing.ab_tests + send_log.variant already exist from migration 0002.

-- Journey Builder ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS marketing.journeys (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    name        TEXT        NOT NULL,
    description TEXT,
    trigger     JSONB       NOT NULL DEFAULT '{}'::jsonb,
    status      TEXT        NOT NULL DEFAULT 'draft',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_journeys_tenant
    ON marketing.journeys(tenant_id, status);

ALTER TABLE marketing.journeys ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON marketing.journeys
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TRIGGER trg_journeys_updated_at
    BEFORE UPDATE ON marketing.journeys
    FOR EACH ROW EXECUTE FUNCTION marketing.set_updated_at();

-- Ordered steps within a journey.
-- step_type: 'send_campaign' | 'wait' | 'condition'
CREATE TABLE IF NOT EXISTS marketing.journey_steps (
    id                    UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    journey_id            UUID    NOT NULL REFERENCES marketing.journeys(id) ON DELETE CASCADE,
    step_order            INTEGER NOT NULL,
    step_type             TEXT    NOT NULL,
    campaign_id           UUID,                -- send_campaign / condition check
    wait_days             INTEGER,             -- wait step
    condition_type        TEXT,                -- 'opened' | 'clicked' | 'not_opened'
    condition_campaign_id UUID,               -- campaign to check engagement on
    yes_next_order        INTEGER,            -- step_order to advance to on true
    no_next_order         INTEGER,            -- step_order to advance to on false
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (journey_id, step_order)
);

CREATE INDEX IF NOT EXISTS idx_journey_steps_journey
    ON marketing.journey_steps(journey_id, step_order ASC);

-- Enrollment tracks each customer's progress through an active journey.
CREATE TABLE IF NOT EXISTS marketing.journey_enrollments (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    journey_id         UUID        NOT NULL REFERENCES marketing.journeys(id),
    tenant_id          UUID        NOT NULL,
    customer_id        UUID        NOT NULL,
    current_step_order INTEGER,                -- NULL = not started yet
    status             TEXT        NOT NULL DEFAULT 'active',  -- active | completed | exited
    next_action_at     TIMESTAMPTZ,
    enrolled_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (journey_id, customer_id)
);

-- Index powers the execution scheduler's due-enrollment query.
CREATE INDEX IF NOT EXISTS idx_journey_enrollments_due
    ON marketing.journey_enrollments(next_action_at)
    WHERE status = 'active' AND next_action_at IS NOT NULL;

ALTER TABLE marketing.journey_enrollments ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON marketing.journey_enrollments
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
