-- Hub Ops: dock scheduling and sort scan log
CREATE TABLE IF NOT EXISTS hub_ops.dock_slots (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    hub_id          UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    dock_number     SMALLINT    NOT NULL,
    dock_type       TEXT        NOT NULL DEFAULT 'inbound',
    vehicle_id      UUID,
    carrier_code    TEXT,
    scheduled_at    TIMESTAMPTZ NOT NULL,
    arrived_at      TIMESTAMPTZ,
    departed_at     TIMESTAMPTZ,
    parcel_count    INTEGER     NOT NULL DEFAULT 0,
    status          TEXT        NOT NULL DEFAULT 'scheduled',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_dock_hub_time ON hub_ops.dock_slots(hub_id, scheduled_at);
ALTER TABLE hub_ops.dock_slots ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON hub_ops.dock_slots USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS hub_ops.sort_scans (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    hub_id          UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    induction_id    UUID,
    barcode         TEXT        NOT NULL,
    shipment_id     UUID,
    destination_hub_id UUID,
    sort_zone       TEXT,
    belt_lane       SMALLINT,
    scanned_by      UUID,
    scanned_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_sort_scans_hub    ON hub_ops.sort_scans(hub_id, scanned_at DESC);
CREATE INDEX IF NOT EXISTS idx_sort_scans_barcode ON hub_ops.sort_scans(barcode);
ALTER TABLE hub_ops.sort_scans ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON hub_ops.sort_scans USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
