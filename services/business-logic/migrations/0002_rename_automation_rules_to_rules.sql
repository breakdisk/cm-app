-- =============================================================================
-- Migration: 0002_rename_automation_rules_to_rules
-- Service:   Business Logic & Automation Engine
-- Description:
--   Aligns the database table name with the runtime query layer
--   (services/business-logic/src/infrastructure/db/mod.rs), which
--   queries `business_logic.rules`. Migration 0001 created the table
--   as `automation_rules`; this migration renames it so the existing
--   Rust queries resolve correctly.
--
--   Idempotent — only renames when the source table exists and the
--   target name is free.
-- =============================================================================

BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_tables
         WHERE schemaname = 'business_logic' AND tablename = 'automation_rules'
    ) AND NOT EXISTS (
        SELECT 1 FROM pg_tables
         WHERE schemaname = 'business_logic' AND tablename = 'rules'
    ) THEN
        ALTER TABLE business_logic.automation_rules RENAME TO rules;
    END IF;
END
$$;

COMMIT;
