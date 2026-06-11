-- Migration: 0011 — hub_routing_configs (last-mile routing rules)
--
-- Per (tenant, hub, destination_zone) rule deciding own-driver vs. 3PL at
-- container release time. Auto mode falls back to a carrier after a time window.

CREATE TABLE IF NOT EXISTS hub_ops.hub_routing_configs (
    id                        UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id                 UUID        NOT NULL,
    hub_id                    UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    destination_zone          TEXT        NOT NULL,
    routing_type              TEXT        NOT NULL CHECK (routing_type IN ('own_driver','carrier','auto')),
    carrier_id                UUID,
    auto_fallback_window_mins INTEGER     NOT NULL DEFAULT 0
                                          CHECK (auto_fallback_window_mins >= 0),
    created_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT routing_config_unique UNIQUE (tenant_id, hub_id, destination_zone)
);

CREATE INDEX idx_routing_configs_hub ON hub_ops.hub_routing_configs (hub_id);

ALTER TABLE hub_ops.hub_routing_configs ENABLE ROW LEVEL SECURITY;
CREATE POLICY routing_config_tenant ON hub_ops.hub_routing_configs
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE TRIGGER trg_routing_configs_updated_at
    BEFORE UPDATE ON hub_ops.hub_routing_configs
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();
