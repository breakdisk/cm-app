-- Migration: 0005 — Reconcile duplicate template tables
--
-- Context
-- -------
-- Migration 0001 created `engagement.notification_templates` whose column names
-- (template_id TEXT, body TEXT, language TEXT, variables TEXT[]) match the Rust
-- TemplateRow struct and all application queries.
--
-- Migration 0002 accidentally created a second table, `engagement.templates`,
-- with incompatible column names (name TEXT, body_template TEXT, no language column,
-- variables JSONB).  The 0002 table was never successfully populated — the code's
-- INSERT statements reference column names that don't exist on it.
--
-- Fix
-- ---
-- 1. Drop the FK constraint linking campaigns.template_id → templates(id).
--    (template_id is already nullable after migration 0004, but the FK reference
--    itself must go before the table can be dropped.)
-- 2. Drop engagement.templates (empty, incompatible schema).
-- 3. Redirect is handled in code (db/mod.rs) — all queries now target
--    engagement.notification_templates.

-- Step 1: fix the priority column on engagement.notifications.
-- Migration 0001 defined it as INTEGER (DEFAULT 2) but all application code
-- writes and reads it as TEXT ("high" | "normal" | "low").  Convert in place.
ALTER TABLE engagement.notifications
    ALTER COLUMN priority TYPE TEXT
    USING CASE priority
        WHEN 1 THEN 'high'
        WHEN 3 THEN 'low'
        ELSE 'normal'
    END;

ALTER TABLE engagement.notifications
    ALTER COLUMN priority SET DEFAULT 'normal';

-- Step 2: remove the FK from campaigns so we can drop the referenced table.
ALTER TABLE engagement.campaigns
    DROP CONSTRAINT IF EXISTS campaigns_template_id_fkey;

-- Step 3: drop the redundant / incompatible table.
-- IF NOT EXISTS is not valid for DROP TABLE; guard with a DO block instead.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'engagement'
           AND table_name   = 'templates'
    ) THEN
        DROP TABLE engagement.templates CASCADE;
    END IF;
END $$;
