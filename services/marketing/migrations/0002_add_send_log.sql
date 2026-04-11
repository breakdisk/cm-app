-- Marketing: Send log and A/B test variants
CREATE TABLE IF NOT EXISTS marketing.send_log (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    campaign_id     UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    channel         TEXT        NOT NULL,
    template_id     UUID        NOT NULL,
    variant         TEXT        NOT NULL DEFAULT 'control',
    status          TEXT        NOT NULL DEFAULT 'queued',
    provider_msg_id TEXT,
    sent_at         TIMESTAMPTZ,
    delivered_at    TIMESTAMPTZ,
    opened_at       TIMESTAMPTZ,
    clicked_at      TIMESTAMPTZ,
    converted_at    TIMESTAMPTZ,
    error_message   TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_send_log_campaign  ON marketing.send_log(tenant_id, campaign_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_send_log_customer  ON marketing.send_log(tenant_id, customer_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_send_log_status    ON marketing.send_log(tenant_id, status) WHERE status IN ('queued', 'sending');
ALTER TABLE marketing.send_log ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON marketing.send_log;
CREATE POLICY tenant_rls ON marketing.send_log USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

CREATE TABLE IF NOT EXISTS marketing.ab_tests (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    campaign_id     UUID        NOT NULL,
    name            TEXT        NOT NULL,
    variants        JSONB       NOT NULL DEFAULT '[]'::jsonb,
    winner_variant  TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    concluded_at    TIMESTAMPTZ
);
ALTER TABLE marketing.ab_tests ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_rls ON marketing.ab_tests;
CREATE POLICY tenant_rls ON marketing.ab_tests USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
