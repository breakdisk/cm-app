CREATE SCHEMA IF NOT EXISTS pod;

-- Proof of delivery records — immutable evidence bundles
CREATE TABLE IF NOT EXISTS pod.proofs (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            UUID        NOT NULL,
    shipment_id          UUID        NOT NULL,
    task_id              UUID        NOT NULL,
    driver_id            UUID        NOT NULL,
    status               TEXT        NOT NULL DEFAULT 'draft'
                                     CHECK (status IN ('draft','submitted','verified','disputed')),
    signature_data       TEXT,                  -- Base64 PNG of signature pad
    recipient_name       TEXT        NOT NULL,
    photos               JSONB       NOT NULL DEFAULT '[]',   -- Array of PodPhoto objects
    capture_lat          DOUBLE PRECISION NOT NULL,
    capture_lng          DOUBLE PRECISION NOT NULL,
    geofence_verified    BOOLEAN     NOT NULL DEFAULT false,
    otp_verified         BOOLEAN     NOT NULL DEFAULT false,
    otp_id               UUID,
    cod_collected_cents  BIGINT,
    captured_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One POD per shipment (latest draft wins via ON CONFLICT in repo)
CREATE UNIQUE INDEX IF NOT EXISTS uq_pod_shipment
    ON pod.proofs (shipment_id)
    WHERE status IN ('draft', 'submitted', 'verified');

CREATE INDEX IF NOT EXISTS idx_pod_tenant ON pod.proofs (tenant_id);
CREATE INDEX IF NOT EXISTS idx_pod_driver ON pod.proofs (driver_id);

ALTER TABLE pod.proofs ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON pod.proofs;
CREATE POLICY tenant_isolation ON pod.proofs
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- OTP codes for high-value delivery confirmation
CREATE TABLE IF NOT EXISTS pod.otp_codes (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID        NOT NULL,
    shipment_id  UUID        NOT NULL,
    phone        TEXT        NOT NULL,
    code_hash    TEXT        NOT NULL,   -- SHA-256 of the 6-digit code
    is_used      BOOLEAN     NOT NULL DEFAULT false,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_otp_shipment ON pod.otp_codes (shipment_id, is_used, expires_at);

-- Auto-prune expired OTPs (run as a periodic cron or pg_cron job)
-- DELETE FROM pod.otp_codes WHERE expires_at < NOW() - INTERVAL '1 hour';
