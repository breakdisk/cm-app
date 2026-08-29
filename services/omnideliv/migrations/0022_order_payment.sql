-- Online prepaid checkout alongside COD — authorize-then-capture-or-void.
--
-- `payment_method` is the explicit branch: 'cod' (today's only behavior,
-- unchanged) or 'online' (opens an authorization hold at checkout, captures
-- it only once a courier actually accepts the job, voids it if none does
-- within the no-courier timeout). Every existing row is 'cod' by
-- construction — DEFAULT makes that explicit rather than leaving it to a
-- nullable column nobody set.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS payment_method TEXT NOT NULL DEFAULT 'cod'
        CHECK (payment_method IN ('cod', 'online')),
    -- Meaningless for a 'cod' row — cash never touches a gateway, so this
    -- simply never leaves 'pending' for one. See Order::PaymentStatus.
    ADD COLUMN IF NOT EXISTS payment_status TEXT NOT NULL DEFAULT 'pending'
        CHECK (payment_status IN ('pending', 'authorized', 'captured', 'voided', 'failed')),
    -- The payments service's payment_intents.id for this order's
    -- authorization hold. NULL for every 'cod' order.
    ADD COLUMN IF NOT EXISTS payment_intent_id UUID,
    -- How much of grand_total_cents was (or will be) taken online rather than
    -- left for the courier to collect at the door. 0 for every 'cod' order.
    -- Not necessarily equal to grand_total_cents for an 'online' order either
    -- — cod_amount_cents = grand_total_cents - prepaid_amount_cents is
    -- computed everywhere it matters (Order::cod_amount_cents) rather than
    -- stored redundantly, so a future partial-prepay product (goods online,
    -- tip in cash) is representable today by a different value here, not a
    -- schema change.
    ADD COLUMN IF NOT EXISTS prepaid_amount_cents BIGINT NOT NULL DEFAULT 0
        CHECK (prepaid_amount_cents >= 0 AND prepaid_amount_cents <= grand_total_cents),
    -- When the payment.intent.authorized webhook last landed. This, not
    -- placed_at, is the clock the no-courier void timeout counts from —
    -- placed_at predates authorization by however long the customer spent on
    -- the hosted checkout page.
    ADD COLUMN IF NOT EXISTS payment_authorized_at TIMESTAMPTZ,
    -- The exact offer card build_offer_card produced at checkout time, held
    -- here so the payment.intent.authorized consumer can offer the job to
    -- couriers with the identical card a COD order would have shown
    -- immediately, rather than trying to reconstruct one later from less
    -- information. NULL for 'cod' orders, which never defer the offer.
    ADD COLUMN IF NOT EXISTS pending_offer_card JSONB;

-- The recovery sweep's no-courier-timeout scan: online orders still holding
-- an authorization with nobody offered the job yet (or nobody who accepted).
CREATE INDEX IF NOT EXISTS idx_orders_payment_authorized
    ON omnideliv.orders (payment_authorized_at)
    WHERE payment_method = 'online' AND payment_status = 'authorized';

COMMENT ON COLUMN omnideliv.orders.prepaid_amount_cents IS
  'How much of grand_total_cents was taken online rather than left for the '
  'courier to collect at the door. 0 for every cod order. Not necessarily '
  'equal to grand_total_cents for an online order either.';
