-- Migration: 0008 — Ensure dispatched_at column exists (safeguard for VPS deployments)
-- This is a defensive migration in case earlier migrations weren't run.
-- If the column already exists, ALTER TABLE IF NOT EXISTS will safely skip.

ALTER TABLE IF EXISTS dispatch.dispatch_queue
    ADD COLUMN IF NOT EXISTS dispatched_at TIMESTAMPTZ;

-- Verify the column exists
SELECT column_name FROM information_schema.columns
WHERE table_name='dispatch_queue' AND column_name='dispatched_at';
