-- Let a customer finish a payment they walked away from.
--
-- `checkout_url` was returned once, in the checkout response, and then
-- forgotten. Anything that took the customer off the hosted page before they
-- paid -- backgrounding the app, a phone call, tapping Back -- left an order
-- that could never be paid for and no way back to the page. It was not even
-- visibly broken: the order sat `placed`/`pending` looking like any other,
-- until payments' 30-minute intent TTL expired and cancelled it.
--
-- Storing it makes the order resumable for exactly as long as the intent it
-- points at is alive. Past that, NI serves its own expired-session page and
-- the expiry sweep cancels the order -- so no separate expiry column is
-- needed here: `payment_status` is already the gate, and a second clock could
-- only disagree with it.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS payment_checkout_url TEXT;

COMMENT ON COLUMN omnideliv.orders.payment_checkout_url IS
  'NI hosted-checkout page for this order''s authorization. Disclosed only to '
  'the order''s own customer, and only while payment_status = pending -- past '
  'that it is spent, and an order whose hold is already authorized must never '
  'offer a second chance to pay.';
