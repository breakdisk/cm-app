-- Append-only order timeline, following the platform's telemetry directive.
-- Every state transition is a new row; nothing is ever updated or deleted.
--
-- device_timestamp vs server_timestamp: device_timestamp is the hardware clock
-- at the physical moment of the event (a courier's pickup scan). SLA and
-- transit-velocity queries use it where present, falling back to
-- server_timestamp only for server-generated events. Using server time alone
-- would silently attribute network latency to the courier.
CREATE TABLE IF NOT EXISTS omnideliv.order_telemetry_logs (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    order_id         UUID        NOT NULL,
    tenant_id        UUID        NOT NULL,
    event_type       TEXT        NOT NULL,
    device_timestamp TIMESTAMPTZ,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_id         UUID,
    payload          JSONB       NOT NULL DEFAULT '{}',
    PRIMARY KEY (id, server_timestamp)
);

CREATE INDEX IF NOT EXISTS idx_order_telemetry_order
    ON omnideliv.order_telemetry_logs (order_id, server_timestamp DESC);

-- Append-only, stated. Same caveat as the ledgers: services connect as the
-- schema owner, so this binds only once they do not.
REVOKE UPDATE, DELETE ON omnideliv.order_telemetry_logs FROM PUBLIC;

-- NOT converted to a TimescaleDB hypertable. The composite primary key is
-- hypertable-compatible so the conversion is a one-line follow-up, but
-- TimescaleDB is not provisioned for this schema and a migration that fails on
-- a missing extension blocks the service from starting — the failure mode that
-- pinned `engagement` to a stale image for seven weeks.
--
-- field-ops migration 0003 takes the other route: a guarded DO block that
-- degrades to a plain table. Either is defensible; what is not is an unguarded
-- create_hypertable.
