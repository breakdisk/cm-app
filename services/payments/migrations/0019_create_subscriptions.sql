-- SaaS subscription billing.
--
-- Until now a tenant's `subscription_tier` could not be changed by anybody.
-- `PUT /v1/tenants/:id/tier` requires `tenants:manage`, and no role grants it
-- -- deliberately, because the same permission rewrites the platform-wide
-- pricing matrix and would be a free self-upgrade to Enterprise. So the tier a
-- tenant signed up on was the tier they kept, and every price on the public
-- pricing page was decoration.
--
-- The resolution is not to grant that permission. A tier change is a
-- CONSEQUENCE of a captured payment, never an API call a tenant can make: see
-- `SubscriptionService::activate_from_payment`. This table is where that
-- consequence is recorded.

-- ── Plan catalogue ───────────────────────────────────────────────────────────
--
-- Seeded from the published pricing page (apps/landing/src/components/Pricing.tsx)
-- so the two cannot disagree silently. Amounts are in the row's own currency:
-- the public page quotes USD, and a deployment whose Network International
-- outlet settles in another currency seeds its own rows rather than relying on
-- a conversion nothing here performs.
--
-- Starter has no row. It is free, it is what a tenant starts on, and it is what
-- an expired subscription falls back to -- there is nothing to charge for and
-- so nothing to sell.
--
-- Enterprise has no row either. The pricing page says "Custom pricing --
-- contact sales", and a self-serve checkout that invents a number for it would
-- be selling something nobody agreed. `checkout` refuses it by name.
CREATE TABLE IF NOT EXISTS payments.subscription_plans (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tier           TEXT        NOT NULL CHECK (tier IN ('growth', 'business')),
    -- 'monthly' or 'annual'. An annual plan bills once for twelve months at the
    -- discounted monthly rate; `amount_cents` is the whole charge, not the rate.
    interval       TEXT        NOT NULL CHECK (interval IN ('monthly', 'annual')),
    currency       TEXT        NOT NULL,
    amount_cents   BIGINT      NOT NULL CHECK (amount_cents > 0),
    period_days    INTEGER     NOT NULL CHECK (period_days > 0),
    is_active      BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tier, interval, currency)
);

INSERT INTO payments.subscription_plans (tier, interval, currency, amount_cents, period_days)
VALUES
    -- $149/month.
    ('growth',   'monthly', 'USD',  14900,  30),
    -- $99/month billed annually = $1,188 once.
    ('growth',   'annual',  'USD', 118800, 365),
    -- $499/month.
    ('business', 'monthly', 'USD',  49900,  30),
    -- $349/month billed annually = $4,188 once.
    ('business', 'annual',  'USD', 418800, 365)
ON CONFLICT (tier, interval, currency) DO NOTHING;

-- ── Subscriptions ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS payments.subscriptions (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    plan_id             UUID        NOT NULL REFERENCES payments.subscription_plans(id),
    tier                TEXT        NOT NULL,

    -- pending_payment : created, nothing captured yet. The tenant is still on
    --                   whatever tier they had.
    -- active          : paid, inside its period.
    -- past_due        : the period ended and no renewal was paid. Still on the
    --                   paid tier -- this is the grace window, not a downgrade.
    -- cancelled       : the tenant asked to stop. Runs to period end, then
    --                   lapses like any other.
    -- lapsed          : grace exhausted. Tier reverted to starter.
    status              TEXT        NOT NULL DEFAULT 'pending_payment'
        CHECK (status IN ('pending_payment', 'active', 'past_due', 'cancelled', 'lapsed')),

    currency            TEXT        NOT NULL,
    amount_cents        BIGINT      NOT NULL,

    current_period_start TIMESTAMPTZ,
    -- When the paid period ends. NULL until the first payment lands: an unpaid
    -- subscription has no period, and a NULL here is what stops the renewal
    -- sweep from treating a never-paid row as overdue.
    current_period_end   TIMESTAMPTZ,

    -- The intent whose capture last extended this subscription. Kept so a
    -- redelivered `payment.intent.captured` can be recognised as the one
    -- already applied rather than extending the period a second time -- Kafka
    -- is at-least-once, and a duplicate here is a free month.
    last_payment_intent_id UUID,

    -- Set when a renewal notice has been published for the current period, so
    -- the sweep sends one notice and not one per tick.
    renewal_notice_sent_at TIMESTAMPTZ,

    -- Whether `identity` has been told about the current tier. The tier lives
    -- in another service, so a payment can be captured here and the grant fail
    -- there; this is the durable record that the two disagree, and what the
    -- retry sweep works from. Without it a tenant pays and silently gets
    -- nothing.
    tier_synced_at      TIMESTAMPTZ,

    cancelled_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One live subscription per tenant. Partial, so a tenant's history of lapsed
-- and cancelled rows is kept rather than overwritten -- a billing record that
-- deletes itself on downgrade is not a billing record.
CREATE UNIQUE INDEX IF NOT EXISTS uq_subscriptions_one_live_per_tenant
    ON payments.subscriptions (tenant_id)
    WHERE status IN ('pending_payment', 'active', 'past_due', 'cancelled');

-- The renewal / dunning sweep.
CREATE INDEX IF NOT EXISTS idx_subscriptions_period_end
    ON payments.subscriptions (current_period_end)
    WHERE status IN ('active', 'past_due', 'cancelled');

-- The tier-sync retry sweep: paid subscriptions identity has not been told about.
CREATE INDEX IF NOT EXISTS idx_subscriptions_tier_unsynced
    ON payments.subscriptions (updated_at)
    WHERE tier_synced_at IS NULL AND status IN ('active', 'lapsed');

COMMENT ON COLUMN payments.subscriptions.tier_synced_at IS
  'When identity was last told this subscription''s tier. NULL means the money '
  'moved and the entitlement did not -- the retry sweep''s work list.';
