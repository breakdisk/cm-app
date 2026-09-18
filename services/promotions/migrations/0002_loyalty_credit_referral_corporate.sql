-- Part D, the rest: loyalty tiers, credit, referrals, corporate rates, and the
-- record of what came off each booking.

-- The loyalty ladder. Tenant config: no rows, no loyalty programme.
-- A tier takes a share off accessorials only — the design's rule that no tier
-- touches distance or weight, so a long, heavy move costs what the rate card
-- says — and never more than its cap on one move.
CREATE TABLE IF NOT EXISTS promotions.tiers (
    id                  UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID    NOT NULL,
    name                TEXT    NOT NULL,
    -- Completed moves in the last 12 months to reach this tier.
    min_moves           INT     NOT NULL CHECK (min_moves >= 0),
    perk                TEXT    NOT NULL DEFAULT '',
    accessorial_bps     BIGINT  NOT NULL DEFAULT 0 CHECK (accessorial_bps BETWEEN 0 AND 10000),
    -- The most this tier takes off one move, in the move's own currency.
    cap_cents           BIGINT  NOT NULL DEFAULT 0 CHECK (cap_cents >= 0),
    -- Gold's "double referral credit".
    referral_multiplier INT     NOT NULL DEFAULT 1 CHECK (referral_multiplier BETWEEN 1 AND 5),
    CONSTRAINT tiers_one_per_threshold UNIQUE (tenant_id, min_moves),
    CONSTRAINT tiers_one_per_name UNIQUE (tenant_id, name)
);

-- Moves per account, projected from shipment.created and delivery.completed.
-- The loyalty count is read here rather than asked of order-intake: order-intake
-- calls promotions at quote time, and a call back would be a cycle.
CREATE TABLE IF NOT EXISTS promotions.moves (
    shipment_id  UUID        PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    account_id   UUID        NOT NULL,
    booked_at    TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS moves_by_account
    ON promotions.moves (tenant_id, account_id, completed_at DESC);

-- Credit: a non-cash entitlement (the 2026-08-11 no-customer-wallet decision
-- stands). Never withdrawn, never topped up; it only comes off a move. The
-- balance is the sum of the entries, and entries are never updated.
CREATE TABLE IF NOT EXISTS promotions.credit_entries (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    account_id   UUID        NOT NULL,
    -- Positive adds, negative spends.
    amount_cents BIGINT      NOT NULL CHECK (amount_cents <> 0),
    currency     TEXT        NOT NULL,
    -- 'referral_reward' | 'grant' | 'applied' | 'returned'
    kind         TEXT        NOT NULL,
    shipment_id  UUID,
    referral_id  UUID,
    note         TEXT        NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS credit_by_account
    ON promotions.credit_entries (tenant_id, account_id, currency, created_at DESC);
-- A booking spends credit once and gets it back once.
CREATE UNIQUE INDEX IF NOT EXISTS credit_applied_once
    ON promotions.credit_entries (shipment_id) WHERE kind = 'applied';
CREATE UNIQUE INDEX IF NOT EXISTS credit_returned_once
    ON promotions.credit_entries (shipment_id) WHERE kind = 'returned';
-- A referral pays once.
CREATE UNIQUE INDEX IF NOT EXISTS credit_referral_paid_once
    ON promotions.credit_entries (referral_id) WHERE kind = 'referral_reward';

-- Each account's own code to share.
CREATE TABLE IF NOT EXISTS promotions.referral_codes (
    tenant_id  UUID        NOT NULL,
    account_id UUID        NOT NULL,
    code       TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, account_id),
    CONSTRAINT referral_code_unique UNIQUE (tenant_id, code)
);

-- Who joined with whose code. An account is referred once, never by itself.
CREATE TABLE IF NOT EXISTS promotions.referrals (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    referrer_account_id UUID        NOT NULL,
    invitee_account_id  UUID        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the invitee's first move completed and the referrer was paid.
    rewarded_at         TIMESTAMPTZ,
    reward_cents        BIGINT,
    CONSTRAINT referrals_once_per_invitee UNIQUE (tenant_id, invitee_account_id),
    CONSTRAINT referrals_not_self CHECK (referrer_account_id <> invitee_account_id)
);
CREATE INDEX IF NOT EXISTS referrals_by_referrer
    ON promotions.referrals (tenant_id, referrer_account_id, created_at DESC);

-- Corporate rates: a tariff, not a promotion. The window does not gate it, and
-- it competes with a promo code rather than adding to one.
CREATE TABLE IF NOT EXISTS promotions.corporate_accounts (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    code         TEXT        NOT NULL,
    firm_name    TEXT        NOT NULL,
    percent_bps  BIGINT      NOT NULL CHECK (percent_bps BETWEEN 1 AND 10000),
    -- A work email on this domain is offered the rate without the code.
    email_domain TEXT,
    active       BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT corporate_code_unique UNIQUE (tenant_id, code)
);

CREATE TABLE IF NOT EXISTS promotions.corporate_links (
    tenant_id    UUID        NOT NULL,
    account_id   UUID        NOT NULL,
    corporate_id UUID        NOT NULL REFERENCES promotions.corporate_accounts (id),
    linked_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, account_id)
);

-- What came off each booking, line by line — the record finance attributes
-- discount spend by. Written with the redemption, in one transaction.
CREATE TABLE IF NOT EXISTS promotions.booking_discounts (
    shipment_id  UUID        NOT NULL,
    tenant_id    UUID        NOT NULL,
    account_id   UUID        NOT NULL,
    -- 'code' | 'corporate' | 'tier' | 'credit'
    kind         TEXT        NOT NULL,
    label        TEXT        NOT NULL,
    amount_cents BIGINT      NOT NULL CHECK (amount_cents > 0),
    currency     TEXT        NOT NULL,
    budget_tag   TEXT        NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (shipment_id, kind)
);
