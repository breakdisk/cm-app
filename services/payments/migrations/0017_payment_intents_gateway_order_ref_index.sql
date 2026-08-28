-- Migration: 0017 — Payments: index gateway_order_ref for webhook lookup
--
-- Supports `PaymentIntentRepository::find_by_gateway_order_ref`, the new
-- fallback path in `PaymentIntentService::find_by_order_ref`. That method
-- used to assume NI's webhook `orderReference` always echoes back our own
-- `merchant_order_reference` (our intent id) — unverified against a live NI
-- sandbox. It now also tries looking the webhook's reference up against the
-- `gateway_order_ref` column (NI's own `reference`, stored here by
-- `create_session` — see `network_international.rs::create_session` and
-- `CreateSessionRequest`/`GatewaySession`), so the integration is correct
-- regardless of which convention NI actually uses.
--
-- Not UNIQUE: `gateway_order_ref` is expected 1:1 with the intent it was
-- minted for (one hosted-checkout session per intent), but that isn't
-- enforced elsewhere in this table today (unlike `gateway_payment_ref`,
-- which already has a unique partial index for capture idempotency), and
-- adding a new uniqueness constraint is out of scope for what this lookup
-- needs — a plain index is sufficient for `find_by_gateway_order_ref` to
-- avoid a sequential scan.

CREATE INDEX idx_payment_intents_gateway_order_ref
    ON payments.payment_intents (gateway_order_ref)
    WHERE gateway_order_ref IS NOT NULL;
