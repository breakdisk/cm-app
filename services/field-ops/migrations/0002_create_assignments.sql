-- Courier assignments. The claim is cross-product: LogisticOS and OmniDeliv
-- both dispatch from the same courier pool, so "one active claim per courier"
-- must be enforced by the database, not by application convention.

-- The consumer registry. Adding a product is a data change, not a schema
-- change. `completion_topic` is why this is a table rather than a free-text
-- column: field-ops has to route a completion event somewhere, and a bare
-- string gives a label with no destination. The FK also forecloses the typo
-- failure a free-text column invites, where 'omnideliv ' and 'omnideliv'
-- silently become two products that no query joins.
CREATE TABLE IF NOT EXISTS field_ops.products (
    key              TEXT        PRIMARY KEY,
    display_name     TEXT        NOT NULL,
    completion_topic TEXT        NOT NULL,
    is_active        BOOLEAN     NOT NULL DEFAULT true,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO field_ops.products (key, display_name, completion_topic) VALUES
    ('logistics', 'LogisticOS',  'logistics.assignment.completed'),
    ('omnideliv', 'OmniDeliv AI', 'omnideliv.assignment.completed')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS field_ops.courier_assignments (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    courier_id      UUID        NOT NULL REFERENCES field_ops.couriers(id),
    -- Which product owns this assignment. field-ops does not interpret it
    -- beyond routing completion events home.
    --
    -- FK to a registry, NOT a CHECK enumeration: a platform tier that needs a
    -- migration to admit its third consumer is not a platform tier. Onboarding
    -- a product is an INSERT into field_ops.products.
    product         TEXT        NOT NULL REFERENCES field_ops.products(key),
    -- The product's own job id (shipment_id, order_id). field-ops does not
    -- interpret it — storing a typed FK here would couple the tier to a product.
    external_ref    UUID        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'offered'
                                CHECK (status IN ('offered','claimed','completed','released','expired')),
    offered_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    -- Claim heartbeat. A claim older than the TTL is reclaimable, so a crashed
    -- client cannot hold a courier hostage.
    heartbeat_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- THE INVARIANT: at most one live claim per courier, enforced by the database.
-- A partial unique index is the cheapest correct expression of this — it costs
-- nothing on non-claimed rows and makes a double-claim a constraint violation
-- rather than a race the application has to notice.
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_single_live_claim
    ON field_ops.courier_assignments (courier_id)
    WHERE status = 'claimed';

CREATE INDEX IF NOT EXISTS idx_assignment_tenant_status
    ON field_ops.courier_assignments (tenant_id, status);

CREATE INDEX IF NOT EXISTS idx_assignment_external_ref
    ON field_ops.courier_assignments (product, external_ref);

-- Reclaim sweep support: find claims whose heartbeat has gone stale.
CREATE INDEX IF NOT EXISTS idx_assignment_stale_claims
    ON field_ops.courier_assignments (heartbeat_at)
    WHERE status = 'claimed';
