-- CDP: Customer segments and consent management
CREATE TABLE IF NOT EXISTS cdp.segments (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    name            TEXT        NOT NULL,
    description     TEXT        NOT NULL DEFAULT '',
    filter_criteria JSONB       NOT NULL DEFAULT '{}'::jsonb,
    customer_count  INTEGER     NOT NULL DEFAULT 0,
    is_dynamic      BOOLEAN     NOT NULL DEFAULT true,
    last_computed   TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_segments_tenant ON cdp.segments(tenant_id);
ALTER TABLE cdp.segments ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON cdp.segments;
DROP POLICY IF EXISTS tenant_rls ON cdp.segments;
CREATE POLICY tenant_rls ON cdp.segments USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS cdp.segment_members (
    segment_id   UUID NOT NULL REFERENCES cdp.segments(id) ON DELETE CASCADE,
    customer_id  UUID NOT NULL,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (segment_id, customer_id)
);
CREATE INDEX IF NOT EXISTS idx_segment_members_customer ON cdp.segment_members(customer_id);

CREATE TABLE IF NOT EXISTS cdp.consent_records (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    consent_type    TEXT        NOT NULL,
    granted         BOOLEAN     NOT NULL,
    channel         TEXT,
    ip_address      TEXT,
    granted_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at      TIMESTAMPTZ,
    UNIQUE (tenant_id, customer_id, consent_type)
);
CREATE INDEX IF NOT EXISTS idx_consent_customer ON cdp.consent_records(tenant_id, customer_id);
ALTER TABLE cdp.consent_records ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON cdp.consent_records;
DROP POLICY IF EXISTS tenant_rls ON cdp.consent_records;
CREATE POLICY tenant_rls ON cdp.consent_records USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS cdp.churn_scores (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    score           FLOAT4      NOT NULL CHECK (score >= 0.0 AND score <= 1.0),
    tier            TEXT        NOT NULL DEFAULT 'healthy',
    top_signals     JSONB       NOT NULL DEFAULT '[]'::jsonb,
    model_version   TEXT        NOT NULL DEFAULT 'v1',
    computed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_churn_tenant_customer ON cdp.churn_scores(tenant_id, customer_id, computed_at DESC);
ALTER TABLE cdp.churn_scores ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON cdp.churn_scores;
DROP POLICY IF EXISTS tenant_rls ON cdp.churn_scores;
CREATE POLICY tenant_rls ON cdp.churn_scores USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
