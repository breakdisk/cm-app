-- Delivery Experience: public tracking events and notification opt-outs
CREATE TABLE IF NOT EXISTS delivery_experience.tracking_events (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    shipment_id     UUID        NOT NULL,
    tracking_number TEXT        NOT NULL,
    event_type      TEXT        NOT NULL,
    description     TEXT        NOT NULL,
    lat             FLOAT8,
    lng             FLOAT8,
    hub_name        TEXT,
    driver_name     TEXT,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_tracking_shipment ON delivery_experience.tracking_events(tenant_id, shipment_id, occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_tracking_number   ON delivery_experience.tracking_events(tracking_number, occurred_at DESC);
ALTER TABLE delivery_experience.tracking_events ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON delivery_experience.tracking_events USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS delivery_experience.delivery_preferences (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    preferred_time_start  TIME,
    preferred_time_end    TIME,
    delivery_instructions TEXT,
    safe_drop_allowed     BOOLEAN NOT NULL DEFAULT false,
    otp_required          BOOLEAN NOT NULL DEFAULT true,
    sms_notifications     BOOLEAN NOT NULL DEFAULT true,
    whatsapp_notifications BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, customer_id)
);
ALTER TABLE delivery_experience.delivery_preferences ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_rls ON delivery_experience.delivery_preferences USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
