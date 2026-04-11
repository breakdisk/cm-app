-- Migration: 0002 — Order Intake: Shipment status event log (immutable audit trail)

CREATE TABLE IF NOT EXISTS order_intake.shipment_events (
    id            UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id     UUID        NOT NULL,
    shipment_id   UUID        NOT NULL REFERENCES order_intake.shipments(id),
    event_type    TEXT        NOT NULL,   -- "status_changed" | "assigned" | "rescheduled" | etc.
    from_status   TEXT,
    to_status     TEXT,
    actor_id      UUID,                   -- user_id or driver_id who triggered it
    actor_type    TEXT,                   -- "user" | "driver" | "system" | "ai_agent"
    metadata      JSONB       NOT NULL DEFAULT '{}',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_shipment_events_shipment ON order_intake.shipment_events (shipment_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_shipment_events_tenant   ON order_intake.shipment_events (tenant_id, created_at DESC);

ALTER TABLE order_intake.shipment_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE order_intake.shipment_events FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON order_intake.shipment_events;
CREATE POLICY tenant_isolation ON order_intake.shipment_events
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
