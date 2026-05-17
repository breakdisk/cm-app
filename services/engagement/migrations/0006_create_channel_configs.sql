-- Migration: 0006 — Engagement: Per-tenant notification channel configuration
--
-- Stores provider credentials and enable/disable flags per tenant per channel.
-- Auth tokens are stored as-is here; production should encrypt them at rest via
-- Vault or pgcrypto (see ADR for secrets management).

CREATE TABLE engagement.tenant_channel_configs (
    id                   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id            UUID        NOT NULL UNIQUE,

    -- WhatsApp (Twilio)
    whatsapp_enabled     BOOLEAN     NOT NULL DEFAULT false,
    whatsapp_account_sid TEXT,
    whatsapp_auth_token  TEXT,
    whatsapp_from_number TEXT,

    -- SMS (Twilio)
    sms_enabled          BOOLEAN     NOT NULL DEFAULT false,
    sms_account_sid      TEXT,
    sms_auth_token       TEXT,
    sms_from_number      TEXT,

    -- Email (AWS SES — from address only; AWS credentials come from IAM)
    email_enabled        BOOLEAN     NOT NULL DEFAULT false,
    email_from_address   TEXT,
    email_from_name      TEXT,

    -- Push (Expo — no extra credentials needed, tokens resolved via identity svc)
    push_enabled         BOOLEAN     NOT NULL DEFAULT false,

    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_channel_configs_tenant ON engagement.tenant_channel_configs (tenant_id);

ALTER TABLE engagement.tenant_channel_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE engagement.tenant_channel_configs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON engagement.tenant_channel_configs
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
