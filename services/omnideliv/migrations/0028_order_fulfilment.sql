-- Dine-in orders — ADR-0017.
--
-- A dine-in order is an order with N vendor legs and ZERO courier legs: the
-- food crosses a room, not a city. Nothing dispatches, no delivery fee is
-- charged, and no courier is ever offered the job.
--
-- Recorded on the order rather than inferred later from "has no courier_task_id"
-- because those are different facts. An absent courier task also describes a
-- delivery order nobody has accepted yet, and a recovery sweep that could not
-- tell the two apart would either chase couriers for food already on a table or
-- silently ignore a delivery that never found one.
--
-- 'delivery' for every existing row, which is what every order to date was.

ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS fulfilment TEXT NOT NULL DEFAULT 'delivery';

ALTER TABLE omnideliv.orders
    DROP CONSTRAINT IF EXISTS orders_fulfilment_check;

ALTER TABLE omnideliv.orders
    ADD CONSTRAINT orders_fulfilment_check
    CHECK (fulfilment IN ('delivery', 'dine_in'));

-- A dine-in order must never carry delivery economics. The application enforces
-- this too; having it here means a bad write from any future path fails loudly
-- rather than quietly charging a diner for a courier who never existed — the
-- same reasoning as `leg_splits_exactly` in 0008.
ALTER TABLE omnideliv.orders
    DROP CONSTRAINT IF EXISTS dine_in_has_no_delivery_economics;

ALTER TABLE omnideliv.orders
    ADD CONSTRAINT dine_in_has_no_delivery_economics
    CHECK (
        fulfilment <> 'dine_in'
        OR (delivery_fee_cents = 0 AND courier_trip_cents = 0)
    );

-- The recovery sweep looks for orders still hunting a courier. A dine-in order
-- never is, so this keeps it out of that scan entirely rather than relying on
-- every future sweep author to remember to exclude it.
CREATE INDEX IF NOT EXISTS idx_order_awaiting_courier
    ON omnideliv.orders (placed_at)
    WHERE fulfilment = 'delivery' AND status IN ('placed', 'awaiting_courier');
