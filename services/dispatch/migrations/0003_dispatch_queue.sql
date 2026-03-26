-- dispatch_queue: shipments awaiting driver assignment.
-- Populated by consuming SHIPMENT_CREATED events from order-intake.
CREATE TABLE IF NOT EXISTS dispatch.dispatch_queue (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    shipment_id         UUID        NOT NULL UNIQUE,
    -- Customer info (denormalized from SHIPMENT_CREATED event)
    customer_name       TEXT        NOT NULL,
    customer_phone      TEXT        NOT NULL,
    -- Destination
    dest_address_line1  TEXT        NOT NULL,
    dest_city           TEXT        NOT NULL,
    dest_province       TEXT        NOT NULL DEFAULT '',
    dest_postal_code    TEXT        NOT NULL DEFAULT '',
    dest_lat            DOUBLE PRECISION,
    dest_lng            DOUBLE PRECISION,
    -- Parcel
    cod_amount_cents    BIGINT,
    special_instructions TEXT,
    service_type        TEXT        NOT NULL DEFAULT 'standard',
    -- Queue state
    status              TEXT        NOT NULL DEFAULT 'pending'
                                    CHECK (status IN ('pending','dispatched','cancelled')),
    queued_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dispatched_at       TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_dispatch_queue_tenant_status
    ON dispatch.dispatch_queue (tenant_id, status, queued_at);
