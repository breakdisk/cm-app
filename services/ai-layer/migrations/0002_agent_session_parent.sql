-- A mesh run is a parent session with one child per specialist. Without this
-- link the AI Agents dashboard shows N unrelated sessions per order.
--
-- Schema is `ai`, not `ai_layer` — see 0001_create_agent_tables.sql and the
-- `SET search_path TO ai, public` in bootstrap.rs.
ALTER TABLE ai.agent_sessions
    ADD COLUMN IF NOT EXISTS parent_session_id UUID REFERENCES ai.agent_sessions(id);

-- Children of a run, for the dashboard drill-down. Partial: most sessions are
-- roots and would only bloat the index.
CREATE INDEX IF NOT EXISTS idx_agent_session_parent
    ON ai.agent_sessions (parent_session_id)
    WHERE parent_session_id IS NOT NULL;
