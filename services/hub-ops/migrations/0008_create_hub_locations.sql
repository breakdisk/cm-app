-- Migration: 0008 — hub_locations (warehouse topology tree)
--
-- Structural slots inside a hub: zone → aisle → rack → shelf → bin. Created by
-- ops via the admin portal; tenant-scoped. Inventory references location by id.

CREATE TABLE IF NOT EXISTS hub_ops.hub_locations (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID        NOT NULL,
    hub_id        UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    zone_id       TEXT        NOT NULL,
    aisle         TEXT,
    rack          TEXT,
    shelf         TEXT,
    bin           TEXT,
    location_tag  TEXT        NOT NULL,
    capacity_cbm  DOUBLE PRECISION,
    is_active     BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT hub_locations_tag_unique UNIQUE (tenant_id, location_tag)
);

CREATE INDEX idx_hub_locations_hub_zone ON hub_ops.hub_locations (hub_id, zone_id);
CREATE INDEX idx_hub_locations_tenant   ON hub_ops.hub_locations (tenant_id);

ALTER TABLE hub_ops.hub_locations ENABLE ROW LEVEL SECURITY;
CREATE POLICY hub_location_tenant ON hub_ops.hub_locations
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);
