-- Cash the courier collects at the door.
--
-- Declared by the offering product, exactly like `trip_cents`: field-ops cannot
-- know what an order is worth any more than it can know a product's tariff.
-- Today OmniDeliv sets it to the order's grand total, because every order is
-- cash-on-delivery. When a prepaid rail lands this becomes 0 for those orders
-- and the same code path keeps working — which is why this is an amount rather
-- than a `payment_method` enum with one variant.
--
-- Zero means nothing to collect, not "unknown". A courier is never asked to
-- guess, and the delivery handler skips the ledger entry entirely.
ALTER TABLE field_ops.courier_assignments
    ADD COLUMN IF NOT EXISTS cod_amount_cents BIGINT NOT NULL DEFAULT 0
        CHECK (cod_amount_cents >= 0);

COMMENT ON COLUMN field_ops.courier_assignments.cod_amount_cents IS
  'Cash to collect on delivery, declared by the offering product. Recorded '
  'against the courier ledger as a negative entry on delivery: the courier is '
  'holding the platform''s money until they remit it.';

-- The ledger's kind CHECK predates COD and would reject the two new entry
-- kinds at write time — a courier delivering a cash order would fail on the
-- ledger insert *after* the delivery was recorded, which is the worst place to
-- discover a constraint. Widened here rather than in a later migration so the
-- column and the kinds that use it land together.
ALTER TABLE field_ops.courier_ledger_entries
    DROP CONSTRAINT IF EXISTS courier_ledger_entries_kind_check;
ALTER TABLE field_ops.courier_ledger_entries
    ADD CONSTRAINT courier_ledger_entries_kind_check
    CHECK (kind IN ('trip_earning','tip','adjustment','payout','cod_collected','cod_remitted'));
