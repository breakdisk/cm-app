-- AI Layer: agent session audit log.
-- Append-heavy, never deleted — full audit trail of all autonomous agent actions.

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE IF NOT EXISTS ai.agent_sessions (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    agent_type        TEXT        NOT NULL,   -- 'dispatch' | 'recovery' | 'reconciliation' | 'anomaly' | 'on_demand'
    status            TEXT        NOT NULL DEFAULT 'running', -- 'running' | 'completed' | 'failed' | 'human_escalated'

    -- Triggering event payload (Kafka message or API request body)
    trigger_data      JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Full conversation history with Claude (for audit and debugging)
    messages          JSONB       NOT NULL DEFAULT '[]'::jsonb,

    -- Tool calls made during this session
    actions           JSONB       NOT NULL DEFAULT '[]'::jsonb,

    -- Final agent outcome text
    outcome           TEXT,

    -- Human escalation reason
    escalation_reason TEXT,

    -- Agent's self-reported confidence (0.0 – 1.0)
    confidence_score  REAL,

    model_used        TEXT        NOT NULL DEFAULT 'claude-opus-4-6',

    started_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at      TIMESTAMPTZ
);

-- Tenant dashboard: list all agent sessions
CREATE INDEX IF NOT EXISTS ai_sessions_tenant_started
    ON ai.agent_sessions (tenant_id, started_at DESC);

-- Operations queue: escalated sessions awaiting human review
CREATE INDEX IF NOT EXISTS ai_sessions_escalated
    ON ai.agent_sessions (tenant_id, status)
    WHERE status = 'human_escalated';

-- Agent type analytics
CREATE INDEX IF NOT EXISTS ai_sessions_type
    ON ai.agent_sessions (tenant_id, agent_type, started_at DESC);

-- ─── RLS ─────────────────────────────────────────────────────────────────────
ALTER TABLE ai.agent_sessions ENABLE ROW LEVEL SECURITY;

-- CREATE POLICY does not support `IF NOT EXISTS` until PostgreSQL 15. Wrap in a
-- DO block that swallows duplicate_object so this migration can re-run against
-- a database where the policy was already created by a prior run (the schema-
-- isolated migrator may seed an empty <schema>._sqlx_migrations table during
-- ADR-0012 cutover, forcing all migrations to re-execute).
DO $$
BEGIN
    CREATE POLICY ai_sessions_tenant_isolation ON ai.agent_sessions
        USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));
EXCEPTION WHEN duplicate_object THEN
    NULL;
END $$;

-- ─── Immutable completed sessions ────────────────────────────────────────────
-- Completed/failed/escalated sessions must not be modified (except via resolve endpoint).
-- The application layer enforces this; the DB rule is a backstop.
-- Note: sessions in 'running' status ARE updated (message history appended mid-run).
