-- The basket is the mesh's shared state. One row per customer session; one
-- sub_intent per fanned-out specialist; lines belong to a sub_intent so each
-- specialist's contribution stays attributable.

CREATE TABLE IF NOT EXISTS omnideliv.baskets (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'draft'
                                CHECK (status IN ('draft','proposed','awaiting_review','confirmed','abandoned')),
    -- The mesh run that produced this basket. Links the basket to its agent
    -- audit trail (agent_sessions) so any line can be traced to the turn that
    -- proposed it.
    mesh_session_id UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_basket_customer
    ON omnideliv.baskets (tenant_id, customer_id, created_at DESC);

-- One row per vertical the Concierge split the utterance into. This is what
-- makes "agents are roles instantiated per sub-intent" concrete: two grocery +
-- restaurant sub-intents mean two Nutritionist workers.
CREATE TABLE IF NOT EXISTS omnideliv.sub_intents (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    basket_id   UUID        NOT NULL REFERENCES omnideliv.baskets(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    vertical    TEXT        NOT NULL
                            CHECK (vertical IN ('restaurant','grocery','pharmacy','florist','retail')),
    vendor_hint TEXT,
    -- The slice of the customer's utterance this sub-intent came from. Kept for
    -- audit and for showing the user what the agent thought they asked for.
    raw_text    TEXT        NOT NULL,
    -- Budget, dietary, timing constraints lifted from the CDP profile.
    constraints JSONB       NOT NULL DEFAULT '{}',
    status      TEXT        NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending','satisfied','degraded','failed')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sub_intent_basket ON omnideliv.sub_intents (basket_id);

CREATE TABLE IF NOT EXISTS omnideliv.basket_lines (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    basket_id         UUID        NOT NULL REFERENCES omnideliv.baskets(id) ON DELETE CASCADE,
    sub_intent_id     UUID        NOT NULL REFERENCES omnideliv.sub_intents(id) ON DELETE CASCADE,
    tenant_id         UUID        NOT NULL,
    vendor_id         UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    item_id           UUID        NOT NULL REFERENCES omnideliv.catalog_items(id),
    qty               INT         NOT NULL CHECK (qty > 0),
    -- Price captured when the line was proposed. The catalog price may move
    -- before checkout; the customer pays what they were shown.
    unit_price_cents  BIGINT      NOT NULL CHECK (unit_price_cents >= 0),
    state             TEXT        NOT NULL DEFAULT 'proposed'
                                  CHECK (state IN ('proposed','accepted','substituted','rejected')),
    -- Set on a replacement line, pointing at the line it replaces. Self-FK so a
    -- substitution chain is walkable for the review UI and for audit.
    substitution_for  UUID        REFERENCES omnideliv.basket_lines(id),
    proposed_by_agent TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_basket_line_basket ON omnideliv.basket_lines (basket_id);
CREATE INDEX IF NOT EXISTS idx_basket_line_sub_intent ON omnideliv.basket_lines (sub_intent_id);
