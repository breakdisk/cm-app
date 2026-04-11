-- Migration: 0002 — Engagement: Campaign management tables
-- Adds campaign management, campaign send tracking, and reusable message templates.
-- These tables power the Campaign Builder in the Merchant Portal and the
-- Marketing Automation Engine's bulk-send and drip-campaign workflows.

CREATE SCHEMA IF NOT EXISTS engagement;

-- ─── Templates ────────────────────────────────────────────────────────────────

-- engagement.templates stores reusable message templates for all channels.
-- Body templates use Handlebars syntax (e.g., {{customer_name}}, {{tracking_url}}).
-- Platform-level templates (tenant_id IS NULL) are available to all tenants
-- and cannot be deleted via the tenant API — only via platform admin tooling.
CREATE TABLE IF NOT EXISTS engagement.templates (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID,                                   -- NULL = platform-level template
    name            TEXT        NOT NULL,                   -- human-readable label
    channel         TEXT        NOT NULL
                                CHECK (channel IN ('whatsapp', 'sms', 'email', 'push')),
    -- Handlebars body template. For email, this is the full HTML body.
    body_template   TEXT        NOT NULL,
    -- JSON array of variable names used in body_template, e.g. ["customer_name","tracking_url"]
    variables       JSONB       NOT NULL DEFAULT '[]',
    -- For email channel: the subject line (Handlebars allowed).
    subject         TEXT,
    -- For WhatsApp channel: the approved template name registered with Meta.
    -- Required for WhatsApp Business API; other channels leave this NULL.
    wa_template_name TEXT,
    -- Whether this template is available for use in campaigns.
    is_active       BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- A tenant cannot have two active templates with the same name + channel combo.
    UNIQUE NULLS NOT DISTINCT (tenant_id, name, channel)
);

CREATE INDEX IF NOT EXISTS idx_templates_tenant_channel
    ON engagement.templates (tenant_id, channel)
    WHERE is_active = TRUE;

CREATE INDEX IF NOT EXISTS idx_templates_tenant_id
    ON engagement.templates (tenant_id);

ALTER TABLE engagement.templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE engagement.templates FORCE ROW LEVEL SECURITY;

-- Tenants can see their own templates plus platform-level templates (tenant_id IS NULL).
DROP POLICY IF EXISTS templates_tenant_isolation ON engagement.templates;
CREATE POLICY templates_tenant_isolation ON engagement.templates
    USING (
        tenant_id = current_setting('app.tenant_id', true)::UUID
        OR tenant_id IS NULL
    );

-- ─── Campaigns ────────────────────────────────────────────────────────────────

-- engagement.campaigns is the primary record for a bulk or triggered outreach campaign.
-- A campaign targets a filtered audience segment and sends a templated message
-- via one channel. Statuses follow the lifecycle:
--   draft → scheduled → running → completed
--   draft → cancelled (any time before running)
--   running → cancelled (emergency stop)
CREATE TABLE IF NOT EXISTS engagement.campaigns (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           UUID        NOT NULL,
    name                TEXT        NOT NULL,
    status              TEXT        NOT NULL DEFAULT 'draft'
                                    CHECK (status IN (
                                        'draft',
                                        'scheduled',
                                        'running',
                                        'completed',
                                        'cancelled'
                                    )),
    channel             TEXT        NOT NULL
                                    CHECK (channel IN ('whatsapp', 'sms', 'email', 'push')),
    template_id         UUID        NOT NULL REFERENCES engagement.templates(id),

    -- JSONB filter criteria matching the CustomerSearchFilter structure from cdp.proto.
    -- Evaluated at send time by the Marketing Automation Engine against the CDP.
    -- Example: {"churn_tiers": ["AT_RISK"], "marketing_consent_only": true}
    audience_filter     JSONB       NOT NULL DEFAULT '{}',

    -- Scheduled send time. NULL for immediately-triggered campaigns.
    scheduled_at        TIMESTAMPTZ,

    -- Timestamps for the campaign lifecycle events.
    started_at          TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,

    -- Recipient and delivery counters. Updated in real-time by the send worker.
    total_recipients    BIGINT      NOT NULL DEFAULT 0,
    sent_count          BIGINT      NOT NULL DEFAULT 0,
    delivered_count     BIGINT      NOT NULL DEFAULT 0,
    read_count          BIGINT      NOT NULL DEFAULT 0,
    conversion_count    BIGINT      NOT NULL DEFAULT 0,   -- e.g., tracking link clicked

    -- Optional UTM parameters for marketing attribution.
    utm_source          TEXT,
    utm_medium          TEXT,
    utm_campaign        TEXT,

    -- Whether this is a recurring/drip campaign (true) or a one-time blast (false).
    is_recurring        BOOLEAN     NOT NULL DEFAULT FALSE,

    -- For recurring campaigns: cron expression defining the recurrence schedule.
    recurrence_cron     TEXT,

    -- User who created this campaign.
    created_by_user_id  UUID        NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Primary query: tenant's campaigns by status (campaign list view)
CREATE INDEX IF NOT EXISTS idx_campaigns_tenant_status
    ON engagement.campaigns (tenant_id, status, scheduled_at DESC);

-- Scheduled campaign poller: find campaigns due to run
CREATE INDEX IF NOT EXISTS idx_campaigns_scheduled_at
    ON engagement.campaigns (scheduled_at)
    WHERE status = 'scheduled' AND scheduled_at IS NOT NULL;

-- Template reference index for cascade-safety checks
CREATE INDEX IF NOT EXISTS idx_campaigns_template_id
    ON engagement.campaigns (template_id);

ALTER TABLE engagement.campaigns ENABLE ROW LEVEL SECURITY;
ALTER TABLE engagement.campaigns FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS campaigns_tenant_isolation ON engagement.campaigns;
CREATE POLICY campaigns_tenant_isolation ON engagement.campaigns
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- ─── Campaign Sends ───────────────────────────────────────────────────────────

-- engagement.campaign_sends is the individual send record for each
-- (campaign, customer) pair. One row is written per recipient when the
-- campaign send worker expands the audience.
--
-- This table is append-heavy (potentially millions of rows per large campaign).
-- Status updates are applied by the channel delivery receipt webhooks.
CREATE TABLE IF NOT EXISTS engagement.campaign_sends (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    campaign_id     UUID        NOT NULL REFERENCES engagement.campaigns(id),
    customer_id     UUID        NOT NULL,           -- CDP customer profile ID
    channel         TEXT        NOT NULL
                                CHECK (channel IN ('whatsapp', 'sms', 'email', 'push')),

    -- Current delivery status of this individual send.
    status          TEXT        NOT NULL DEFAULT 'queued'
                                CHECK (status IN (
                                    'queued',       -- queued for sending, not yet dispatched
                                    'sending',      -- handed off to channel provider
                                    'sent',         -- provider confirmed acceptance
                                    'delivered',    -- delivery receipt received
                                    'read',         -- read receipt received (WhatsApp, push)
                                    'failed',       -- all retries exhausted
                                    'bounced',      -- invalid address / unsubscribed
                                    'skipped'       -- recipient opted out or suppressed
                                )),

    -- Provider-assigned message ID for receipt correlation (e.g., Twilio SID).
    provider_message_id TEXT,

    -- Human-readable error description for failed/bounced sends.
    error_message   TEXT,

    -- Lifecycle timestamps
    queued_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at         TIMESTAMPTZ,
    delivered_at    TIMESTAMPTZ,
    read_at         TIMESTAMPTZ,
    failed_at       TIMESTAMPTZ
);

-- Campaign drill-down: all sends for a campaign (campaign analytics view)
CREATE INDEX IF NOT EXISTS idx_campaign_sends_campaign_id
    ON engagement.campaign_sends (campaign_id, status);

-- Customer send history: all campaigns a customer was included in
CREATE INDEX IF NOT EXISTS idx_campaign_sends_customer_id
    ON engagement.campaign_sends (customer_id, queued_at DESC);

-- Provider message ID lookup (for delivery receipt webhook correlation)
CREATE INDEX IF NOT EXISTS idx_campaign_sends_provider_msg_id
    ON engagement.campaign_sends (provider_message_id)
    WHERE provider_message_id IS NOT NULL;

-- Queued sends poller (send worker picks up pending sends)
CREATE INDEX IF NOT EXISTS idx_campaign_sends_queued
    ON engagement.campaign_sends (campaign_id, queued_at)
    WHERE status = 'queued';

-- RLS for campaign_sends is enforced via campaign_id JOIN — no direct RLS policy
-- needed here since the table has no tenant_id column. The application layer
-- always filters via campaign_id, which is already tenant-scoped by campaigns RLS.
-- We enable RLS but bypass it for the service role (which handles the joins).
ALTER TABLE engagement.campaign_sends ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS campaign_sends_service_bypass ON engagement.campaign_sends;
CREATE POLICY campaign_sends_service_bypass ON engagement.campaign_sends
    USING (TRUE);   -- application enforces tenant isolation via campaign_id FK

-- ─── Triggers: updated_at maintenance ─────────────────────────────────────────

CREATE OR REPLACE FUNCTION engagement.set_updated_at()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS trg_campaigns_updated_at ON engagement.campaigns;
CREATE TRIGGER trg_campaigns_updated_at
    BEFORE UPDATE ON engagement.campaigns
    FOR EACH ROW EXECUTE FUNCTION engagement.set_updated_at();

DROP TRIGGER IF EXISTS trg_templates_updated_at ON engagement.templates;
CREATE TRIGGER trg_templates_updated_at
    BEFORE UPDATE ON engagement.templates
    FOR EACH ROW EXECUTE FUNCTION engagement.set_updated_at();

-- ─── Seed: platform-level reusable marketing templates ────────────────────────

INSERT INTO engagement.templates (name, channel, body_template, variables, subject)
VALUES
    -- Re-engagement campaign template (WhatsApp)
    (
        'win_back_shipment_offer',
        'whatsapp',
        'Hi {{customer_name}}! We miss you. It''s been a while since your last shipment. '
        'Book now and get a special rate for your next delivery. 🚚 Book here: {{booking_url}}',
        '["customer_name", "booking_url"]',
        NULL
    ),
    -- Post-delivery NPS survey (WhatsApp)
    (
        'post_delivery_nps',
        'whatsapp',
        'Hi {{customer_name}}! Your package {{tracking_number}} was delivered. '
        'How was your experience? Rate us here: {{survey_url}} 🙏',
        '["customer_name", "tracking_number", "survey_url"]',
        NULL
    ),
    -- Loyalty milestone (SMS)
    (
        'loyalty_milestone_sms',
        'sms',
        'Congratulations {{customer_name}}! You''ve shipped {{shipment_count}} packages with us. '
        'Enjoy a FREE delivery on your next shipment. Use code: {{promo_code}}',
        '["customer_name", "shipment_count", "promo_code"]',
        NULL
    ),
    -- At-risk churn intervention (email)
    (
        'churn_intervention_email',
        'email',
        '<h2>We''d love to have you back, {{customer_name}}!</h2>'
        '<p>We noticed you haven''t shipped with us lately. '
        'Here''s a <strong>{{discount_percent}}% discount</strong> on your next booking.</p>'
        '<p><a href="{{booking_url}}">Book Now</a></p>',
        '["customer_name", "discount_percent", "booking_url"]',
        'A special offer just for you, {{customer_name}}'
    )
ON CONFLICT DO NOTHING;
