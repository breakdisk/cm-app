-- Promotions: the codes a tenant offers, and the ledger of their use.
CREATE SCHEMA IF NOT EXISTS promotions;

CREATE TABLE IF NOT EXISTS promotions.offers (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    -- Upper-case; matched case-insensitively by normalising on the way in.
    code             TEXT        NOT NULL,
    title            TEXT        NOT NULL,
    body             TEXT        NOT NULL DEFAULT '',
    -- 'percent_carriage' (percent_bps of carriage) or 'flat' (flat_cents).
    -- Checked in the service, not here: a new discount shape is code, and a
    -- CHECK would make it a migration too.
    discount_kind    TEXT        NOT NULL,
    percent_bps      BIGINT      NOT NULL DEFAULT 0,
    flat_cents       BIGINT      NOT NULL DEFAULT 0,
    -- The most this code takes off one move, before the ceiling. NULL: only
    -- the ceiling limits it.
    cap_cents        BIGINT,
    -- Subject to the monthly window and the one-code-a-month rule.
    windowed         BOOLEAN     NOT NULL DEFAULT TRUE,
    -- Usable once per account, ever (a welcome code).
    once_per_account BOOLEAN     NOT NULL DEFAULT FALSE,
    starts_at        TIMESTAMPTZ,
    ends_at          TIMESTAMPTZ,
    active           BOOLEAN     NOT NULL DEFAULT TRUE,
    -- Whose budget pays — finance attributes discount spend by it.
    budget_tag       TEXT        NOT NULL DEFAULT 'marketing',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT offers_code_per_tenant UNIQUE (tenant_id, code),
    CONSTRAINT offers_amounts_not_negative CHECK (percent_bps >= 0 AND flat_cents >= 0 AND (cap_cents IS NULL OR cap_cents >= 0)),
    CONSTRAINT offers_percent_at_most_whole CHECK (percent_bps <= 10000)
);

-- One row per code applied to a booking. The rules that protect money are
-- enforced here, not only in the service: two quotes redeemed at once must not
-- both succeed.
CREATE TABLE IF NOT EXISTS promotions.redemptions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    -- The booking user (order-intake's merchant_id).
    account_id       UUID        NOT NULL,
    offer_id         UUID        NOT NULL REFERENCES promotions.offers (id),
    code             TEXT        NOT NULL,
    -- One redemption per booking: a retried create finds its own row.
    shipment_id      UUID        NOT NULL UNIQUE,
    -- 'YYYY-MM' of the tenant's local date when it was redeemed.
    month            TEXT        NOT NULL,
    windowed         BOOLEAN     NOT NULL,
    once_per_account BOOLEAN     NOT NULL,
    discount_cents   BIGINT      NOT NULL CHECK (discount_cents > 0),
    currency         TEXT        NOT NULL,
    budget_tag       TEXT        NOT NULL,
    redeemed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    -- Set when the booking is cancelled: the code goes back to the customer.
    released_at      TIMESTAMPTZ
);

-- One windowed code per account per month, among redemptions still standing.
CREATE UNIQUE INDEX IF NOT EXISTS redemptions_one_windowed_per_month
    ON promotions.redemptions (tenant_id, account_id, month)
    WHERE windowed AND released_at IS NULL;

-- A once-per-account code, once.
CREATE UNIQUE INDEX IF NOT EXISTS redemptions_once_per_account
    ON promotions.redemptions (tenant_id, account_id, offer_id)
    WHERE once_per_account AND released_at IS NULL;

-- An account's history, for eligibility.
CREATE INDEX IF NOT EXISTS redemptions_by_account
    ON promotions.redemptions (tenant_id, account_id, redeemed_at DESC);
