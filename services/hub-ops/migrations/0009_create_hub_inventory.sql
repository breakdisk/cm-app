-- Migration: 0009 — hub_inventory (current location ledger)
--
-- The only mutable hub entity. One row per piece or consolidated pallet;
-- location_id is updated in place when inventory moves. A pallet move is a single
-- row update regardless of piece count (Consolidated unit supersedes Piece rows).

CREATE TABLE IF NOT EXISTS hub_ops.hub_inventory (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    hub_id         UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    location_id    UUID        NOT NULL REFERENCES hub_ops.hub_locations(id),
    -- InventoryUnit is stored as (unit_kind, unit_ref):
    --   piece        → unit_ref = ChildAwb string
    --   consolidated → unit_ref = PalletId uuid (as text)
    unit_kind      TEXT        NOT NULL CHECK (unit_kind IN ('piece','consolidated')),
    unit_ref       TEXT        NOT NULL,
    batch_number   TEXT,
    checked_in_by  UUID        NOT NULL,
    checked_in_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A given physical unit has at most one active inventory record per tenant.
    CONSTRAINT hub_inventory_unit_unique UNIQUE (tenant_id, unit_kind, unit_ref)
);

CREATE INDEX idx_hub_inventory_location ON hub_ops.hub_inventory (location_id);
CREATE INDEX idx_hub_inventory_hub      ON hub_ops.hub_inventory (hub_id);

ALTER TABLE hub_ops.hub_inventory ENABLE ROW LEVEL SECURITY;
CREATE POLICY hub_inventory_tenant ON hub_ops.hub_inventory
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE TRIGGER trg_hub_inventory_updated_at
    BEFORE UPDATE ON hub_ops.hub_inventory
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();
