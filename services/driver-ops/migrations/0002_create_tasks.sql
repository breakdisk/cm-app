-- Driver tasks — one per shipment stop, created when dispatch assigns a route.
CREATE TABLE IF NOT EXISTS driver_ops.tasks (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    driver_id            UUID        NOT NULL REFERENCES driver_ops.drivers(id),
    route_id             UUID        NOT NULL,     -- FK to dispatch.routes (cross-schema)
    shipment_id          UUID        NOT NULL,     -- FK to order_intake.shipments (cross-schema)
    task_type            TEXT        NOT NULL CHECK (task_type IN ('pickup', 'delivery')),
    sequence             INTEGER     NOT NULL,
    status               TEXT        NOT NULL DEFAULT 'pending'
                                     CHECK (status IN ('pending','in_progress','completed','failed','skipped')),
    -- Delivery address (denormalized from shipment for offline access)
    address_line1        TEXT        NOT NULL,
    address_line2        TEXT,
    city                 TEXT        NOT NULL,
    province             TEXT        NOT NULL DEFAULT '',
    postal_code          TEXT        NOT NULL DEFAULT '',
    country              TEXT        NOT NULL DEFAULT 'PH',
    lat                  DOUBLE PRECISION,
    lng                  DOUBLE PRECISION,
    customer_name        TEXT        NOT NULL,
    customer_phone       TEXT        NOT NULL,
    cod_amount_cents     BIGINT,
    special_instructions TEXT,
    -- Completion fields
    pod_id               UUID,                     -- FK to pod.proofs (cross-schema)
    started_at           TIMESTAMPTZ,
    completed_at         TIMESTAMPTZ,
    failed_reason        TEXT
);

CREATE INDEX IF NOT EXISTS idx_tasks_driver_id    ON driver_ops.tasks(driver_id, status);
CREATE INDEX IF NOT EXISTS idx_tasks_route_id     ON driver_ops.tasks(route_id);
CREATE INDEX IF NOT EXISTS idx_tasks_shipment_id  ON driver_ops.tasks(shipment_id);
