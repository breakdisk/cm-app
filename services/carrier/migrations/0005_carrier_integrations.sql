-- Migration: carrier integration credentials per tenant
-- Each row stores the encrypted API credentials for one 3PL carrier
-- for one tenant.  Multiple tenants can have independent credentials
-- for the same carrier (e.g. different DHL accounts per region).
--
-- Credentials are stored as AES-256-GCM encrypted JSONB (encrypted at
-- the application layer using the tenant's Vault-managed key before INSERT).
-- The `carrier_code` values match the AdapterRegistry keys:
--   DHL | FEDEX | UPS | TNT | DPD | ARAMEX
--
-- NOTE: This table does NOT replace the carrier.carriers table (3PL partner
-- profiles for SLA/rate management).  carrier_integrations stores the API
-- credentials used by the adapter layer; carrier.carriers stores the
-- operational relationship.  A single carrier_code row can coexist with
-- zero or more carriers rows.

CREATE TABLE IF NOT EXISTS carrier.integration_credentials (
    id           UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id    UUID        NOT NULL,
    carrier_code TEXT        NOT NULL,  -- DHL | FEDEX | UPS | TNT | DPD | ARAMEX
    -- AES-256-GCM encrypted JSON payload (base64url).
    -- Decrypted at runtime by the carrier service using tenant's Vault key.
    credentials_enc TEXT     NOT NULL,
    -- Clear-text metadata safe to store (no secrets here)
    account_label  TEXT,               -- Human-readable label (e.g. "DHL PH Production")
    base_url       TEXT,               -- Carrier API base URL override (sandbox vs prod)
    is_active      BOOLEAN  NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uix_integration_tenant_carrier
    ON carrier.integration_credentials (tenant_id, carrier_code)
    WHERE is_active = true;

-- Telemetry: booking log for all carrier API bookings made via MCP tools.
-- Append-only; never UPDATE or DELETE.
CREATE TABLE IF NOT EXISTS carrier.carrier_bookings (
    id              UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    tenant_id       UUID        NOT NULL,
    carrier_code    TEXT        NOT NULL,
    shipment_id     UUID,                -- platform shipment_id if booked from order context
    awb             TEXT,                -- platform AWB used as carrier customer reference
    service_code    TEXT        NOT NULL,
    booking_ref     TEXT        NOT NULL, -- carrier-issued booking reference
    tracking_number TEXT        NOT NULL,
    label_url       TEXT,
    -- Serialised BookingRequest for audit/replay
    request_payload JSONB       NOT NULL DEFAULT '{}',
    -- Serialised BookingConfirmation from carrier
    response_payload JSONB      NOT NULL DEFAULT '{}',
    booked_by_actor UUID,               -- actor_uid from MCP JWT
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_carrier_bookings_shipment  ON carrier.carrier_bookings (shipment_id) WHERE shipment_id IS NOT NULL;
CREATE INDEX idx_carrier_bookings_tracking  ON carrier.carrier_bookings (tracking_number);
CREATE INDEX idx_carrier_bookings_tenant_ts ON carrier.carrier_bookings (tenant_id, created_at DESC);

-- Prevent modification of the audit log at the DB level
REVOKE UPDATE, DELETE ON carrier.carrier_bookings FROM app_role;
