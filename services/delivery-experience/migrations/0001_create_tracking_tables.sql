-- Delivery Experience: shipment tracking read model.
-- This is an event-sourced projection — authoritative source is order-intake.
-- Public tracking lookups use tracking_number; all other queries use shipment_id.

CREATE SCHEMA IF NOT EXISTS tracking;

CREATE TABLE IF NOT EXISTS tracking.shipment_tracking (
    shipment_id          UUID            PRIMARY KEY,
    tenant_id            UUID            NOT NULL,
    tracking_number      TEXT            NOT NULL UNIQUE,

    current_status       TEXT            NOT NULL DEFAULT 'pending',

    -- Immutable chronological event log, JSONB array of StatusEvent
    status_history       JSONB           NOT NULL DEFAULT '[]'::jsonb,

    -- Display addresses
    origin_address       TEXT            NOT NULL DEFAULT '',
    destination_address  TEXT            NOT NULL DEFAULT '',

    -- Driver info (populated once assigned)
    driver_id            UUID,
    driver_name          TEXT,
    driver_phone         TEXT,
    driver_position      JSONB,          -- {lat, lng, updated_at}

    -- Timing
    estimated_delivery   TIMESTAMPTZ,
    delivered_at         TIMESTAMPTZ,

    -- POD
    pod_id               UUID,
    recipient_name       TEXT,

    -- Attempts
    attempt_number       SMALLINT        NOT NULL DEFAULT 0,
    next_attempt_at      TIMESTAMPTZ,

    created_at           TIMESTAMPTZ     NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ     NOT NULL DEFAULT now()
);

-- Fast public tracking lookup
CREATE INDEX IF NOT EXISTS tracking_by_number
    ON tracking.shipment_tracking (tracking_number);

-- Merchant dashboard: list shipments by tenant ordered by recency
CREATE INDEX IF NOT EXISTS tracking_by_tenant_created
    ON tracking.shipment_tracking (tenant_id, created_at DESC);

-- Active deliveries filter
CREATE INDEX IF NOT EXISTS tracking_active_by_tenant
    ON tracking.shipment_tracking (tenant_id, current_status)
    WHERE current_status NOT IN ('delivered', 'cancelled', 'returned');

-- Driver position index (optional: used if we query by driver)
CREATE INDEX IF NOT EXISTS tracking_by_driver
    ON tracking.shipment_tracking (driver_id)
    WHERE driver_id IS NOT NULL;

-- ─── RLS ─────────────────────────────────────────────────────────────────────
-- Note: public tracking endpoint bypasses RLS (no session tenant set).
-- Authenticated endpoints set app.tenant_id.

ALTER TABLE tracking.shipment_tracking ENABLE ROW LEVEL SECURITY;

CREATE POLICY tracking_tenant_isolation ON tracking.shipment_tracking
    USING (
        current_setting('app.tenant_id', true) = ''
        OR tenant_id = (current_setting('app.tenant_id', true)::UUID)
    );

-- ─── updated_at trigger ───────────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION tracking.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER trg_shipment_tracking_updated_at
    BEFORE UPDATE ON tracking.shipment_tracking
    FOR EACH ROW EXECUTE FUNCTION tracking.set_updated_at();
