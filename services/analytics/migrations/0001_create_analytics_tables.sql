-- Analytics: append-only event store.
-- Designed for aggregate queries (GROUP BY date, driver, tenant).
-- No UPDATE/DELETE on events — use Kafka replay to rebuild if needed.

CREATE SCHEMA IF NOT EXISTS analytics;

CREATE TABLE IF NOT EXISTS analytics.shipment_events (
    id                UUID        PRIMARY KEY,
    tenant_id         UUID        NOT NULL,
    shipment_id       UUID        NOT NULL,
    event_type        TEXT        NOT NULL,   -- 'created' | 'delivered' | 'failed' | 'cancelled'
    driver_id         UUID,
    service_type      TEXT,
    cod_amount_cents  BIGINT,
    on_time           BOOLEAN,
    delivery_hours    FLOAT8,
    occurred_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Primary analytics query pattern: tenant + date range
CREATE INDEX IF NOT EXISTS analytics_events_tenant_date
    ON analytics.shipment_events (tenant_id, occurred_at);

-- Driver performance queries
CREATE INDEX IF NOT EXISTS analytics_events_driver
    ON analytics.shipment_events (tenant_id, driver_id, occurred_at)
    WHERE driver_id IS NOT NULL;

-- Shipment lookup (for COD amount update)
CREATE INDEX IF NOT EXISTS analytics_events_shipment
    ON analytics.shipment_events (shipment_id);

-- ─── TimescaleDB hypertable (optional — enables time-based chunking) ─────────
-- Uncomment if TimescaleDB is installed:
-- SELECT create_hypertable('analytics.shipment_events', 'occurred_at',
--     chunk_time_interval => INTERVAL '7 days', if_not_exists => TRUE);
-- SELECT add_retention_policy('analytics.shipment_events', INTERVAL '2 years');

-- ─── Immutable event store — block mutations ──────────────────────────────────
CREATE OR REPLACE RULE no_update_shipment_events AS
    ON UPDATE TO analytics.shipment_events DO INSTEAD NOTHING;

CREATE OR REPLACE RULE no_delete_shipment_events AS
    ON DELETE TO analytics.shipment_events DO INSTEAD NOTHING;

-- ─── RLS ─────────────────────────────────────────────────────────────────────
ALTER TABLE analytics.shipment_events ENABLE ROW LEVEL SECURITY;

CREATE POLICY analytics_tenant_isolation ON analytics.shipment_events
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));
