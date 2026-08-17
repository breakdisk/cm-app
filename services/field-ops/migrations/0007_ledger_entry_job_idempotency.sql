-- Idempotency that survives a period boundary.
--
-- The bug: `credit_courier` skipped a job it had already credited, but it
-- scanned only the ledger returned by `find_open(tenant, courier,
-- current_period())` -- and `current_period()` is the ISO week. A delivery
-- whose POST succeeded but whose response was lost (the normal case for an
-- offline driver queue) retries later; if that retry lands after the
-- Sunday->Monday UTC boundary, a *fresh* ledger opens, the guard finds nothing,
-- and the courier is credited a second time and the COD debited a second time.
--
-- The application guard now queries across periods. This index is the backstop:
-- any future path that credits a courier is covered whether or not its author
-- knew the rule.
--
-- Why two new columns: `courier_ledger_entries` keys on `ledger_id`, and a
-- ledger is per (tenant, courier, period). An index on `ledger_id` would
-- therefore be scoped *inside* the very boundary the bug crosses. The columns
-- are denormalised from the owning ledger for exactly that reason.
ALTER TABLE field_ops.courier_ledger_entries
    ADD COLUMN IF NOT EXISTS tenant_id  UUID,
    ADD COLUMN IF NOT EXISTS courier_id UUID;

-- Backfill from the owning ledger before the columns become authoritative.
-- The FK on `ledger_id` means there are no orphan entries, so this reaches
-- every row.
UPDATE field_ops.courier_ledger_entries e
   SET tenant_id  = l.tenant_id,
       courier_id = l.courier_id
  FROM field_ops.courier_ledgers l
 WHERE e.ledger_id = l.id
   AND (e.tenant_id IS NULL OR e.courier_id IS NULL);

ALTER TABLE field_ops.courier_ledger_entries
    ALTER COLUMN tenant_id  SET NOT NULL,
    ALTER COLUMN courier_id SET NOT NULL;

-- Partial: payouts, remittances and adjustments carry no job reference, and a
-- courier may legitimately have many of those.
--
-- This CREATE **fails** if any courier has already been double-credited. That
-- is the correct outcome, not an obstacle: the duplicates are real money and
-- which entry survives is a human decision. Silently de-duplicating would erase
-- the evidence that it happened. Find them with:
--
--   SELECT tenant_id, courier_id, kind, external_ref, COUNT(*)
--     FROM field_ops.courier_ledger_entries
--    WHERE external_ref IS NOT NULL
--    GROUP BY 1,2,3,4 HAVING COUNT(*) > 1;
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_ledger_entry_job
    ON field_ops.courier_ledger_entries (tenant_id, courier_id, kind, external_ref)
    WHERE external_ref IS NOT NULL;

COMMENT ON INDEX field_ops.uq_courier_ledger_entry_job IS
  'One entry of each kind per courier per job, across every period. Backstops '
  'DispatchService::credit_courier for retried deliveries.';
