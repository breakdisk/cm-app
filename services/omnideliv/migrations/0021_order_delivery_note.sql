-- A customer's note to the courier: "unit 12B, gate code 4417, ring twice".
--
-- Until this existed the dropoff on a courier's manifest was a customer name
-- and a pair of coordinates. An order carries no street address at all —
-- checkout captures a point, not a line of text — so this is the only field in
-- which anyone can say *where the door actually is*.
--
-- Nullable, and staying that way. Most orders will not have one, and a note is
-- never a condition of placing an order.
--
-- No length constraint here on purpose: the bound lives in
-- `clean_delivery_note` (280 characters, measured in characters rather than
-- bytes) so an over-long note is trimmed at the boundary rather than rejecting
-- a checkout that has already been paid for.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS delivery_note TEXT;

COMMENT ON COLUMN omnideliv.orders.delivery_note IS
  'Customer-supplied instruction shown to the courier at the dropoff. '
  'Cleaned and bounded by clean_delivery_note before it is stored.';
