-- Migration: 0004 — Cargo consolidation: truck_specs + consolidation_plans
--
-- truck_specs: ops-configurable vehicle/container interior dimensions for bin-packing.
--             Covers both road trucks (Van, L300, 6Wheeler…) and sea containers (FCL20, FCL40).
--
-- consolidation_plans: the full 3D packing result for a batch of items.
--             items     JSONB — input BoxItems (awb, weight_g, l/w/h in cm, estimated bool)
--             placements JSONB — output Placements (awb, x/y/z offsets in cm, l/w/h, rotated)
--             unplaced  JSONB — items that did not fit (awb, reason)

-- ── Truck / Container Specs ────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS hub_ops.truck_specs (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    name            TEXT        NOT NULL,
    transport_mode  TEXT        NOT NULL DEFAULT 'road'
                                CHECK (transport_mode IN ('road','sea','air')),
    -- Mirrors carrier.SizeClass: Van | L300 | 6Wheeler | 10Wheeler | Trailer | FCL20 | FCL40
    size_class      TEXT        NOT NULL DEFAULT 'van',
    -- Interior usable cargo bay in centimetres
    interior_length_cm  INTEGER NOT NULL CHECK (interior_length_cm > 0),
    interior_width_cm   INTEGER NOT NULL CHECK (interior_width_cm > 0),
    interior_height_cm  INTEGER NOT NULL CHECK (interior_height_cm > 0),
    max_payload_kg      NUMERIC(10,2) NOT NULL CHECK (max_payload_kg > 0),
    is_active       BOOLEAN     NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_truck_specs_tenant ON hub_ops.truck_specs (tenant_id, is_active);

ALTER TABLE hub_ops.truck_specs ENABLE ROW LEVEL SECURITY;
CREATE POLICY truck_spec_tenant ON hub_ops.truck_specs
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE TRIGGER trg_truck_specs_updated_at
    BEFORE UPDATE ON hub_ops.truck_specs
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();

-- ── Seed: standard Philippine freight vehicle specs ────────────────────────────
-- These are tenant-scoped so we cannot seed here without a real tenant_id.
-- Ops creates specs via POST /v1/consolidation/specs after tenant onboarding.

-- ── Consolidation Plans ────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS hub_ops.consolidation_plans (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    hub_id           UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    truck_spec_id    UUID        NOT NULL REFERENCES hub_ops.truck_specs(id),
    -- Linked container once the plan is confirmed and physical loading begins (optional)
    container_id     UUID        REFERENCES hub_ops.containers(id),
    -- Input items: [{awb, weight_g, length_cm, width_cm, height_cm, estimated}]
    items            JSONB       NOT NULL DEFAULT '[]',
    -- Packing result: [{awb, x, y, z, length_cm, width_cm, height_cm, weight_g, estimated, rotated}]
    placements       JSONB       NOT NULL DEFAULT '[]',
    -- Items that did not fit: [{awb, weight_g, reason}]
    unplaced         JSONB       NOT NULL DEFAULT '[]',
    -- Utilization stats (written by the optimizer)
    total_weight_kg  NUMERIC(10,2) NOT NULL DEFAULT 0,
    volume_used_cm3  BIGINT      NOT NULL DEFAULT 0,
    volume_total_cm3 BIGINT      NOT NULL DEFAULT 0,
    piece_count      INTEGER     NOT NULL DEFAULT 0,
    computed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_consolidation_plans_hub    ON hub_ops.consolidation_plans (hub_id, created_at DESC);
CREATE INDEX idx_consolidation_plans_tenant ON hub_ops.consolidation_plans (tenant_id);

ALTER TABLE hub_ops.consolidation_plans ENABLE ROW LEVEL SECURITY;
CREATE POLICY consolidation_plan_tenant ON hub_ops.consolidation_plans
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE TRIGGER trg_consolidation_plans_updated_at
    BEFORE UPDATE ON hub_ops.consolidation_plans
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();
