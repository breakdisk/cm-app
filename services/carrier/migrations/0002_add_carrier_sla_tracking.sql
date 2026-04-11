-- Carrier: SLA tracking and allocation log
CREATE TABLE IF NOT EXISTS carrier.sla_records (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    carrier_id      UUID        NOT NULL REFERENCES carrier.carriers(id),
    shipment_id     UUID        NOT NULL,
    zone            TEXT        NOT NULL,
    service_level   TEXT        NOT NULL DEFAULT 'standard',
    promised_by     TIMESTAMPTZ NOT NULL,
    delivered_at    TIMESTAMPTZ,
    status          TEXT        NOT NULL DEFAULT 'in_transit',
    on_time         BOOLEAN,
    failure_reason  TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (carrier_id, shipment_id)
);
CREATE INDEX IF NOT EXISTS idx_sla_carrier_zone ON carrier.sla_records(carrier_id, zone, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_sla_tenant ON carrier.sla_records(tenant_id, created_at DESC);
ALTER TABLE carrier.sla_records ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON carrier.sla_records;
DROP POLICY IF EXISTS tenant_rls ON carrier.sla_records;
CREATE POLICY tenant_rls ON carrier.sla_records USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS carrier.allocation_log (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    shipment_id         UUID        NOT NULL,
    selected_carrier_id UUID        NOT NULL REFERENCES carrier.carriers(id),
    allocation_method   TEXT        NOT NULL DEFAULT 'ai',
    candidates          JSONB       NOT NULL DEFAULT '[]'::jsonb,
    selection_reason    TEXT,
    allocated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_alloc_tenant ON carrier.allocation_log(tenant_id, allocated_at DESC);
ALTER TABLE carrier.allocation_log ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON carrier.allocation_log;
DROP POLICY IF EXISTS tenant_rls ON carrier.allocation_log;
CREATE POLICY tenant_rls ON carrier.allocation_log USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
