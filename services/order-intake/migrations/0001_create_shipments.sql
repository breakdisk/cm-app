-- Migration: 0001 — Order Intake: Shipments table

CREATE TABLE IF NOT EXISTS order_intake.shipments (
    id                      UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id               UUID        NOT NULL,
    merchant_id             UUID        NOT NULL,
    customer_id             UUID        NOT NULL,
    tracking_number         TEXT        NOT NULL UNIQUE,
    status                  TEXT        NOT NULL DEFAULT 'pending'
                                        CHECK (status IN (
                                            'pending','confirmed','pickup_assigned','picked_up',
                                            'in_transit','at_hub','out_for_delivery',
                                            'delivered','failed','cancelled','returned'
                                        )),
    service_type            TEXT        NOT NULL CHECK (service_type IN ('standard','express','same_day','balikbayan')),

    -- Origin
    origin_line1            TEXT        NOT NULL,
    origin_line2            TEXT,
    origin_barangay         TEXT,
    origin_city             TEXT        NOT NULL,
    origin_province         TEXT        NOT NULL,
    origin_postal_code      TEXT        NOT NULL,
    origin_country_code     TEXT        NOT NULL DEFAULT 'PH',
    origin_lat              DOUBLE PRECISION,
    origin_lng              DOUBLE PRECISION,
    origin_point            GEOGRAPHY(POINT, 4326),

    -- Destination
    dest_line1              TEXT        NOT NULL,
    dest_line2              TEXT,
    dest_barangay           TEXT,
    dest_city               TEXT        NOT NULL,
    dest_province           TEXT        NOT NULL,
    dest_postal_code        TEXT        NOT NULL,
    dest_country_code       TEXT        NOT NULL DEFAULT 'PH',
    dest_lat                DOUBLE PRECISION,
    dest_lng                DOUBLE PRECISION,
    dest_point              GEOGRAPHY(POINT, 4326),

    -- Parcel details
    weight_grams            INTEGER     NOT NULL CHECK (weight_grams > 0),
    length_cm               INTEGER,
    width_cm                INTEGER,
    height_cm               INTEGER,
    declared_value_cents    BIGINT,
    cod_amount_cents        BIGINT,
    special_instructions    TEXT,
    merchant_reference      TEXT,

    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Spatial indexes for dispatch clustering queries
CREATE INDEX IF NOT EXISTS idx_shipments_dest_point       ON order_intake.shipments USING GIST (dest_point);
CREATE INDEX IF NOT EXISTS idx_shipments_origin_point     ON order_intake.shipments USING GIST (origin_point);
CREATE INDEX IF NOT EXISTS idx_shipments_tenant_status    ON order_intake.shipments (tenant_id, status);
CREATE INDEX IF NOT EXISTS idx_shipments_tracking         ON order_intake.shipments (tracking_number);
CREATE INDEX IF NOT EXISTS idx_shipments_merchant         ON order_intake.shipments (tenant_id, merchant_id);
CREATE INDEX IF NOT EXISTS idx_shipments_created_at       ON order_intake.shipments (tenant_id, created_at DESC);

-- Auto-update dest_point/origin_point from lat/lng
CREATE OR REPLACE FUNCTION order_intake.sync_shipment_points()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.dest_lat IS NOT NULL AND NEW.dest_lng IS NOT NULL THEN
        NEW.dest_point = ST_SetSRID(ST_MakePoint(NEW.dest_lng, NEW.dest_lat), 4326)::geography;
    END IF;
    IF NEW.origin_lat IS NOT NULL AND NEW.origin_lng IS NOT NULL THEN
        NEW.origin_point = ST_SetSRID(ST_MakePoint(NEW.origin_lng, NEW.origin_lat), 4326)::geography;
    END IF;
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS shipments_sync_points ON order_intake.shipments;
CREATE TRIGGER shipments_sync_points
    BEFORE INSERT OR UPDATE ON order_intake.shipments
    FOR EACH ROW EXECUTE FUNCTION order_intake.sync_shipment_points();

-- RLS
ALTER TABLE order_intake.shipments ENABLE ROW LEVEL SECURITY;
ALTER TABLE order_intake.shipments FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON order_intake.shipments;
CREATE POLICY tenant_isolation ON order_intake.shipments
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
