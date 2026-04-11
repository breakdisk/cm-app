-- Migration: 0003 — Hub Ops: pallets, pallet_pieces, containers
--
-- Physical consolidation layer:
--   Piece (order_intake) → loaded onto → Pallet → loaded into → Container
--
-- Billing stays at AWB/piece level — pallets and containers are
-- invisible to merchants on invoices.

-- ── Pallets ───────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS hub_ops.pallets (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    origin_hub_id       UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    destination_hub_id  UUID        REFERENCES hub_ops.hubs(id),  -- NULL = local last-mile

    -- Weight (grams) — accumulated as pieces are loaded
    total_weight_grams  INTEGER     NOT NULL DEFAULT 0
                                    CHECK (total_weight_grams >= 0),

    status              TEXT        NOT NULL DEFAULT 'open'
                                    CHECK (status IN (
                                        'open','sealed','loaded',
                                        'in_transit','arrived','broken'
                                    )),

    -- Sealing audit
    sealed_at           TIMESTAMPTZ,
    sealed_by           UUID,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pallets_hub_open
    ON hub_ops.pallets (origin_hub_id, status)
    WHERE status = 'open';

CREATE INDEX IF NOT EXISTS idx_pallets_tenant ON hub_ops.pallets (tenant_id);

ALTER TABLE hub_ops.pallets ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS pallet_tenant ON hub_ops.pallets;
CREATE POLICY pallet_tenant ON hub_ops.pallets
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

DROP TRIGGER IF EXISTS trg_pallets_updated_at ON hub_ops.pallets;
CREATE TRIGGER trg_pallets_updated_at
    BEFORE UPDATE ON hub_ops.pallets
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();

-- ── Pallet pieces ─────────────────────────────────────────────────────────────
-- Many-to-one: piece_awb → pallet. A piece belongs to at most one active pallet.

CREATE TABLE IF NOT EXISTS hub_ops.pallet_pieces (
    id          UUID    PRIMARY KEY DEFAULT gen_random_uuid(),
    pallet_id   UUID    NOT NULL REFERENCES hub_ops.pallets(id) ON DELETE CASCADE,
    tenant_id   UUID    NOT NULL,
    -- AWBs are globally unique — references order_intake.shipment_pieces.piece_awb
    -- Cross-service join avoided per architecture principle; we store the AWB string.
    piece_awb   TEXT    NOT NULL,
    loaded_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT pallet_pieces_awb_unique UNIQUE (piece_awb)
);

CREATE INDEX IF NOT EXISTS idx_pallet_pieces_pallet ON hub_ops.pallet_pieces (pallet_id);
CREATE INDEX IF NOT EXISTS idx_pallet_pieces_awb    ON hub_ops.pallet_pieces (piece_awb);

ALTER TABLE hub_ops.pallet_pieces ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS pallet_piece_tenant ON hub_ops.pallet_pieces;
CREATE POLICY pallet_piece_tenant ON hub_ops.pallet_pieces
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ── Containers ────────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS hub_ops.containers (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,

    -- Transport metadata
    transport_mode      TEXT        NOT NULL
                                    CHECK (transport_mode IN ('road','sea','air')),
    carrier_ref         TEXT,       -- Bill of lading / MAWB

    origin_hub_id       UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    destination_hub_id  UUID        NOT NULL REFERENCES hub_ops.hubs(id),

    -- Master AWBs in this container — denormalized for bulk Kafka events.
    -- Stored as an array for fast membership checks without joins.
    master_awbs         TEXT[]      NOT NULL DEFAULT '{}',

    status              TEXT        NOT NULL DEFAULT 'planning'
                                    CHECK (status IN (
                                        'planning','manifested','loading','sealed',
                                        'in_transit','arrived_at_port','customs',
                                        'released','delivered'
                                    )),

    departed_at         TIMESTAMPTZ,
    estimated_arrival   TIMESTAMPTZ,
    arrived_at          TIMESTAMPTZ,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_containers_tenant_status ON hub_ops.containers (tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_containers_origin        ON hub_ops.containers (origin_hub_id);
CREATE INDEX IF NOT EXISTS idx_containers_destination   ON hub_ops.containers (destination_hub_id);
-- GIN index for fast `piece_awb = ANY(master_awbs)` queries
CREATE INDEX IF NOT EXISTS idx_containers_master_awbs   ON hub_ops.containers USING GIN (master_awbs);

ALTER TABLE hub_ops.containers ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS container_tenant ON hub_ops.containers;
CREATE POLICY container_tenant ON hub_ops.containers
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

DROP TRIGGER IF EXISTS trg_containers_updated_at ON hub_ops.containers;
CREATE TRIGGER trg_containers_updated_at
    BEFORE UPDATE ON hub_ops.containers
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();

-- ── Container pallets ─────────────────────────────────────────────────────────
-- Many-to-one: pallet → container.

CREATE TABLE IF NOT EXISTS hub_ops.container_pallets (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    container_id UUID NOT NULL REFERENCES hub_ops.containers(id) ON DELETE CASCADE,
    pallet_id    UUID NOT NULL REFERENCES hub_ops.pallets(id),
    tenant_id    UUID NOT NULL,
    loaded_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT container_pallets_unique UNIQUE (container_id, pallet_id)
);

CREATE INDEX IF NOT EXISTS idx_container_pallets_container ON hub_ops.container_pallets (container_id);
CREATE INDEX IF NOT EXISTS idx_container_pallets_pallet    ON hub_ops.container_pallets (pallet_id);

ALTER TABLE hub_ops.container_pallets ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS container_pallet_tenant ON hub_ops.container_pallets;
CREATE POLICY container_pallet_tenant ON hub_ops.container_pallets
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ── Container loose pieces ────────────────────────────────────────────────────
-- Pieces loaded directly into a container without a pallet.

CREATE TABLE IF NOT EXISTS hub_ops.container_loose_pieces (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    container_id UUID NOT NULL REFERENCES hub_ops.containers(id) ON DELETE CASCADE,
    tenant_id    UUID NOT NULL,
    piece_awb    TEXT NOT NULL,
    loaded_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT container_loose_piece_unique UNIQUE (piece_awb)
);

CREATE INDEX IF NOT EXISTS idx_container_loose_container ON hub_ops.container_loose_pieces (container_id);

ALTER TABLE hub_ops.container_loose_pieces ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS container_loose_tenant ON hub_ops.container_loose_pieces;
CREATE POLICY container_loose_tenant ON hub_ops.container_loose_pieces
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);
