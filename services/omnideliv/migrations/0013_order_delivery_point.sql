-- Where the order is actually going.
--
-- Checkout has always received `delivery_lat`/`delivery_lng`, used them for the
-- courier offer, and then thrown them away. That made re-dispatch impossible:
-- the recovery sweep could see an order stuck with no courier but had no
-- destination to re-offer, so its Retry branch could only log and wait for the
-- escalation window to expire. An order that cannot say where it is going
-- cannot be dispatched twice.
--
-- Nullable, because orders placed before this migration genuinely do not know.
-- The sweep skips those rather than guessing a point — re-offering to the wrong
-- coordinates would send a courier to the wrong address, which is worse than
-- escalating to a human who can read the basket.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS delivery_lat DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS delivery_lng DOUBLE PRECISION;

COMMENT ON COLUMN omnideliv.orders.delivery_lat IS
  'Destination latitude as given at checkout. NULL for orders placed before '
  'migration 0013; the recovery sweep escalates those instead of re-offering.';
