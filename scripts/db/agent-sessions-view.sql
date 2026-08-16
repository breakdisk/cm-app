-- Unified read model over every agent session on the platform.
--
-- Per ADR-0012 each service owns its schema, so agent sessions live in two
-- places: ai.agent_sessions (LogisticOS agents, in svc_ai_layer) and
-- omnideliv.agent_sessions (mesh runs, in svc_omnideliv). That split is
-- deliberate — a product tier writing another service's tables is the coupling
-- ADR-0009 rule 2 exists to prevent. This view is the read-side union that
-- keeps the AI Agents dashboard querying one thing.
--
-- ── Why this is an operator script and not a service migration ───────────────
--
-- It spans two DATABASES, not two schemas, so a plain view cannot see both:
-- it needs postgres_fdw, which needs CREATE EXTENSION (superuser) and a stored
-- credential. A service migration that requires superuser fails on a database
-- where the service role is not one, and a migration that cannot apply pins the
-- service to its last-good image — silently, which is how engagement sat seven
-- weeks behind master. Reporting plumbing must not be able to stop ai-layer
-- from starting.
--
-- Apply by hand, against svc_ai_layer, as a superuser:
--   docker exec -i logisticos-postgres psql -U logisticos -d svc_ai_layer \
--     -v ON_ERROR_STOP=1 < scripts/db/agent-sessions-view.sql
--
-- Idempotent: safe to re-run after either service adds a column.

\set ON_ERROR_STOP on

CREATE EXTENSION IF NOT EXISTS postgres_fdw;

-- The remote is the same PostgreSQL instance, a different database. `postgres`
-- as host rather than localhost: this runs inside the compose network, where
-- that is the service name.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_foreign_server WHERE srvname = 'omnideliv_srv') THEN
        CREATE SERVER omnideliv_srv
            FOREIGN DATA WRAPPER postgres_fdw
            OPTIONS (host 'postgres', port '5432', dbname 'svc_omnideliv');
    END IF;
END $$;

-- One mapping for the connecting role. In production this should be a
-- read-only role rather than the owner — the dashboard only ever selects.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_user_mappings
         WHERE srvname = 'omnideliv_srv' AND usename = current_user
    ) THEN
        EXECUTE format(
            'CREATE USER MAPPING FOR %I SERVER omnideliv_srv OPTIONS (user %L, password %L)',
            current_user, 'logisticos', 'password'
        );
    END IF;
END $$;

CREATE SCHEMA IF NOT EXISTS omnideliv_remote;

-- Explicit column list rather than IMPORT FOREIGN SCHEMA: the two tables have
-- different columns (omnideliv has role_key, ai-layer has agent_type), and
-- importing everything would make the view's shape depend on whatever either
-- service last migrated.
DROP FOREIGN TABLE IF EXISTS omnideliv_remote.agent_sessions CASCADE;
CREATE FOREIGN TABLE omnideliv_remote.agent_sessions (
    id                UUID,
    tenant_id         UUID,
    role_key          TEXT,
    status            TEXT,
    trigger_data      JSONB,
    outcome           TEXT,
    escalation_reason TEXT,
    confidence_score  REAL,
    model_used        TEXT,
    parent_session_id UUID,
    started_at        TIMESTAMPTZ,
    completed_at      TIMESTAMPTZ
)
SERVER omnideliv_srv
OPTIONS (schema_name 'omnideliv', table_name 'agent_sessions');

-- `product` is what the dashboard groups and filters by; without it the two
-- sources are indistinguishable once unioned, and an operator cannot tell a
-- stalled mesh run from a stalled dispatch agent.
--
-- messages and actions are deliberately excluded. They are large JSONB blobs
-- pulled across the FDW for every row, and a list view never renders them —
-- the drill-down reads the owning service directly.
CREATE OR REPLACE VIEW ai.all_agent_sessions AS
    SELECT
        'logistics'::TEXT   AS product,
        s.id,
        s.tenant_id,
        s.agent_type        AS role_key,
        s.status,
        s.trigger_data,
        s.outcome,
        s.escalation_reason,
        s.confidence_score,
        s.model_used,
        s.parent_session_id,
        s.started_at,
        s.completed_at
      FROM ai.agent_sessions s
UNION ALL
    SELECT
        'omnideliv'::TEXT   AS product,
        o.id,
        o.tenant_id,
        o.role_key,
        o.status,
        o.trigger_data,
        o.outcome,
        o.escalation_reason,
        o.confidence_score,
        o.model_used,
        o.parent_session_id,
        o.started_at,
        o.completed_at
      FROM omnideliv_remote.agent_sessions o;

COMMENT ON VIEW ai.all_agent_sessions IS
  'Read-only union of ai.agent_sessions and omnideliv.agent_sessions (via FDW). '
  'Filter on tenant_id — this view has no RLS and, per ADR-0016, nothing else does either.';

-- Sanity check. A view that returns rows from only one side is the failure this
-- is most likely to have: the FDW resolves, the union succeeds, and half the
-- platform's agents are quietly invisible.
DO $$
DECLARE local_n BIGINT; remote_n BIGINT;
BEGIN
    SELECT count(*) INTO local_n  FROM ai.agent_sessions;
    SELECT count(*) INTO remote_n FROM omnideliv_remote.agent_sessions;
    RAISE NOTICE 'ai.all_agent_sessions: % logistics + % omnideliv session(s)', local_n, remote_n;
END $$;
