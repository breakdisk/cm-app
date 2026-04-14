-- =============================================================================
-- Migration: 0003_align_rules_runtime_schema
-- Service:   Business Logic & Automation Engine
-- Description:
--   Aligns business_logic.rules and business_logic.rule_executions with
--   the columns expected by services/business-logic/src/infrastructure/db/mod.rs.
--
--   Migration 0001 created a richer schema with domain modeling in mind
--   (trigger_type enum, trigger_filter, trigger_event_id, duration_ms, …),
--   but the Rust runtime was written against a simpler schema:
--     - rules: trigger_def (JSONB) instead of trigger_type (TEXT)
--     - rule_executions: kafka_topic, shipment_id, conditions_passed, outcome,
--       fired_at instead of trigger_event_id / status / conditions_met /
--       executed_at.
--
--   Rather than rewriting 0001 (which would break any env that already
--   applied it), this migration adds the runtime-expected columns and
--   relaxes NOT NULL constraints on legacy columns so INSERTs from the
--   runtime succeed.
--
--   Idempotent — safe to re-run.
-- =============================================================================

BEGIN;

-- ---------------------------------------------------------------------------
-- business_logic.rules
-- ---------------------------------------------------------------------------

-- Add trigger_def (runtime uses JSONB for the serialized RuleTrigger)
ALTER TABLE business_logic.rules
    ADD COLUMN IF NOT EXISTS trigger_def JSONB NOT NULL DEFAULT '{}'::jsonb;

-- Best-effort backfill from legacy trigger_type for rows created under 0001
UPDATE business_logic.rules
   SET trigger_def = jsonb_build_object('event_type', trigger_type)
 WHERE trigger_def = '{}'::jsonb
   AND trigger_type IS NOT NULL;

-- Relax NOT NULL on legacy columns the runtime does not populate on INSERT
ALTER TABLE business_logic.rules
    ALTER COLUMN trigger_type DROP NOT NULL;

-- ---------------------------------------------------------------------------
-- business_logic.rule_executions
-- Runtime log_execution() inserts: id, rule_id, tenant_id, kafka_topic,
-- shipment_id, conditions_passed, actions_executed, outcome, error_message,
-- fired_at.  Add the missing columns.
-- ---------------------------------------------------------------------------

ALTER TABLE business_logic.rule_executions
    ADD COLUMN IF NOT EXISTS kafka_topic       TEXT,
    ADD COLUMN IF NOT EXISTS shipment_id       UUID,
    ADD COLUMN IF NOT EXISTS conditions_passed BOOLEAN,
    ADD COLUMN IF NOT EXISTS outcome           TEXT,
    ADD COLUMN IF NOT EXISTS fired_at          TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Helpful indexes matching the runtime query patterns
CREATE INDEX IF NOT EXISTS idx_rule_executions_rule_fired_at
    ON business_logic.rule_executions (rule_id, fired_at DESC);

CREATE INDEX IF NOT EXISTS idx_rule_executions_shipment_id
    ON business_logic.rule_executions (shipment_id)
    WHERE shipment_id IS NOT NULL;

COMMIT;
