CREATE SCHEMA IF NOT EXISTS marketing;

CREATE TABLE IF NOT EXISTS marketing.campaigns (
    id               UUID        PRIMARY KEY,
    tenant_id        UUID        NOT NULL,
    name             TEXT        NOT NULL,
    description      TEXT,
    channel          TEXT        NOT NULL DEFAULT 'sms',
    template         JSONB       NOT NULL DEFAULT '{}'::jsonb,
    targeting        JSONB       NOT NULL DEFAULT '{}'::jsonb,
    status           TEXT        NOT NULL DEFAULT 'draft',
    scheduled_at     TIMESTAMPTZ,
    sent_at          TIMESTAMPTZ,
    completed_at     TIMESTAMPTZ,
    total_sent       BIGINT      NOT NULL DEFAULT 0,
    total_delivered  BIGINT      NOT NULL DEFAULT 0,
    total_failed     BIGINT      NOT NULL DEFAULT 0,
    created_by       UUID        NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS marketing_campaigns_tenant
    ON marketing.campaigns (tenant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS marketing_campaigns_status
    ON marketing.campaigns (tenant_id, status);

-- Scheduled campaigns poller
CREATE INDEX IF NOT EXISTS marketing_campaigns_scheduled
    ON marketing.campaigns (scheduled_at)
    WHERE status = 'scheduled' AND scheduled_at IS NOT NULL;

ALTER TABLE marketing.campaigns ENABLE ROW LEVEL SECURITY;

CREATE POLICY marketing_tenant_isolation ON marketing.campaigns
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

CREATE OR REPLACE FUNCTION marketing.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN NEW.updated_at = now(); RETURN NEW; END;
$$;

CREATE TRIGGER trg_campaigns_updated_at
    BEFORE UPDATE ON marketing.campaigns
    FOR EACH ROW EXECUTE FUNCTION marketing.set_updated_at();
