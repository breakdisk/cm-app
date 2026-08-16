-- Mesh run audit. OmniDeliv keeps its own sessions rather than writing into
-- ai.agent_sessions: ADR-0012 gives each service its own schema, and a product
-- tier reaching into another service's tables is the coupling ADR-0009 rule 2
-- exists to prevent. The shape mirrors ai.agent_sessions so a future unified
-- dashboard can read both without reconciling two models.
CREATE TABLE IF NOT EXISTS omnideliv.agent_sessions (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    -- The AgentRole key: concierge, nutritionist, fleet.
    role_key          TEXT        NOT NULL,
    status            TEXT        NOT NULL,
    trigger_data      JSONB       NOT NULL DEFAULT '{}',
    messages          JSONB       NOT NULL DEFAULT '[]',
    actions           JSONB       NOT NULL DEFAULT '[]',
    outcome           TEXT,
    escalation_reason TEXT,
    confidence_score  REAL,
    model_used        TEXT        NOT NULL,
    -- One parent per mesh run, one child per specialist.
    parent_session_id UUID        REFERENCES omnideliv.agent_sessions(id),
    started_at        TIMESTAMPTZ NOT NULL,
    completed_at      TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_omnideliv_session_tenant
    ON omnideliv.agent_sessions (tenant_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_omnideliv_session_parent
    ON omnideliv.agent_sessions (parent_session_id)
    WHERE parent_session_id IS NOT NULL;
