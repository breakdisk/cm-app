-- Migration: 0007 — hub_scans (append-only chain-of-custody scan log)
--
-- Every barcode read at a hub creates exactly one immutable row. Never updated
-- or deleted — enforced at the application layer (repository only INSERTs) and
-- defended here by revoking UPDATE/DELETE from the app role where it exists.
--
-- Dual-timestamp contract: device_timestamp (hardware clock, SLA basis) +
-- server_timestamp (backend receipt). SLA queries use device_timestamp.

CREATE TABLE IF NOT EXISTS hub_ops.hub_scans (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    hub_id           UUID        NOT NULL REFERENCES hub_ops.hubs(id),
    piece_awb        TEXT        NOT NULL,
    master_awb       TEXT        NOT NULL,
    shipment_id      UUID        NOT NULL,
    scan_type        TEXT        NOT NULL CHECK (scan_type IN (
                                    'inbound_receive','pallet_assign','outbound_load',
                                    'container_deconsolidate','local_sort_assign','exception_flag'
                                 )),
    scanned_by       UUID        NOT NULL,
    device_timestamp TIMESTAMPTZ NOT NULL,
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    pallet_id        UUID,
    container_id     UUID,
    exception        TEXT        CHECK (exception IN ('missing','damaged','weight_mismatch'))
);

CREATE INDEX idx_hub_scans_shipment ON hub_ops.hub_scans (shipment_id, server_timestamp DESC);
CREATE INDEX idx_hub_scans_hub      ON hub_ops.hub_scans (hub_id, server_timestamp DESC);
CREATE INDEX idx_hub_scans_piece    ON hub_ops.hub_scans (piece_awb);

ALTER TABLE hub_ops.hub_scans ENABLE ROW LEVEL SECURITY;
CREATE POLICY hub_scan_tenant ON hub_ops.hub_scans
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Append-only defense-in-depth: revoke mutation from the app role if present.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'app_role') THEN
        EXECUTE 'REVOKE UPDATE, DELETE ON hub_ops.hub_scans FROM app_role';
    END IF;
END $$;
