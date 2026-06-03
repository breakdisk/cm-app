-- Migration: 0010 — hub_transfer_manifests (customs/freight document)
--
-- One per container. Holds MAWB/Bill of Lading, customs broker reference, duty
-- amounts, and the clearance audit trail. Customs clearance is a human gate:
-- cleared_by is required to move to 'cleared'.

CREATE TABLE IF NOT EXISTS hub_ops.hub_transfer_manifests (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    container_id        UUID        NOT NULL REFERENCES hub_ops.containers(id),
    origin_hub_id       UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    destination_hub_id  UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    transport_mode      TEXT        NOT NULL,
    carrier_booking_ref TEXT,
    customs_filing_ref  TEXT,
    customs_status      TEXT        NOT NULL DEFAULT 'not_required'
                                    CHECK (customs_status IN ('not_required','pending','hold','cleared')),
    duties_total_cents  BIGINT,
    broker_payload      JSONB,
    cleared_by          UUID,
    cleared_at          TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT manifest_container_unique UNIQUE (container_id)
);

CREATE INDEX idx_manifests_tenant_status ON hub_ops.hub_transfer_manifests (tenant_id, customs_status);
CREATE INDEX idx_manifests_dest_hub      ON hub_ops.hub_transfer_manifests (destination_hub_id);

ALTER TABLE hub_ops.hub_transfer_manifests ENABLE ROW LEVEL SECURITY;
CREATE POLICY manifest_tenant ON hub_ops.hub_transfer_manifests
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE TRIGGER trg_manifests_updated_at
    BEFORE UPDATE ON hub_ops.hub_transfer_manifests
    FOR EACH ROW EXECUTE FUNCTION hub_ops.set_updated_at();
