-- Highest whole-days-overdue value already announced on
-- `logisticos.payments.invoice.overdue` for this invoice. -1 = never notified.
-- Additive with a default: the previous image never reads or writes it, so it
-- keeps working while the new one rolls out (ADR-0012).
ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS overdue_days_notified INTEGER NOT NULL DEFAULT -1;

-- Sweep scans: issued-and-past-due, and overdue-with-notices-pending.
CREATE INDEX IF NOT EXISTS idx_invoices_overdue_sweep
    ON payments.invoices (status, due_at)
    WHERE status IN ('issued', 'overdue');
