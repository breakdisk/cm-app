-- Extend dispatch_queue status to include 'dispatching' as an intermediate state.
-- 'dispatching' acts as an optimistic lock: quick_dispatch atomically moves a row
-- from 'pending' → 'dispatching' before creating routes/assignments/emitting events.
-- Only one concurrent caller can claim a row; others see 0 rows affected and bail early.
-- On success the row moves 'dispatching' → 'dispatched'; on failure it resets to 'pending'.

ALTER TABLE dispatch.dispatch_queue
    DROP CONSTRAINT IF EXISTS dispatch_queue_status_check;

ALTER TABLE dispatch.dispatch_queue
    ADD CONSTRAINT dispatch_queue_status_check
    CHECK (status IN ('pending', 'dispatching', 'dispatched', 'cancelled'));

-- Orphan-cleanup for stuck 'dispatching' rows: if quick_dispatch crashes mid-flight
-- the row stays 'dispatching' forever. The bootstrap cleanup tick handles this by
-- treating rows older than ORPHAN_CLEANUP_AGE_MINUTES that are still 'dispatching'
-- as stale and resetting them to 'pending'.
-- (No schema change needed — the cleanup tick query is updated in bootstrap.rs.)
