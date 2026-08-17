-- Who the courier is delivering to.
--
-- An order identified its customer by a bare UUID. CheckoutRequest carries
-- basket_id, a tip and a lat/lng and nothing else, so the driver manifest's
-- dropoff would have been a map pin with nobody to ask for and no number to
-- call -- the most common last-mile failure, with no recovery path in the app.
--
-- Snapshotted at checkout rather than resolved on read. The manifest is polled,
-- and a cross-service identity lookup per refresh would put a courier's screen
-- on identity's availability.
--
-- Nullable, and staying nullable: orders placed before this exist and are
-- legitimate. The manifest renders a dropoff without a name rather than
-- refusing to load.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS customer_name  TEXT,
    ADD COLUMN IF NOT EXISTS customer_phone TEXT;

COMMENT ON COLUMN omnideliv.orders.customer_phone IS
  'Snapshotted at checkout from the authenticated caller, never from the '
  'request body. This is also what eventually unblocks SMS and WhatsApp '
  'notifications, which are push-only today because an OmniDeliv order carried '
  'no phone.';

-- No backfill, deliberately. The phone is derivable from a customer''s login
-- address only for OTP-minted accounts, and doing it here would mean this
-- migration reaching into the identity service''s table -- a cross-schema read
-- that ADR-0012 exists to prevent. Existing orders are honestly unknown, and a
-- courier is better told "no number on file" than handed one nobody verified
-- belongs to this delivery.
