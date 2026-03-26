-- Migration: 0001 — Engagement: Notifications and templates

CREATE TABLE engagement.notification_templates (
    id            UUID    PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id     UUID,                   -- NULL = platform-level default
    template_id   TEXT    NOT NULL,       -- slug: "delivery_confirmed"
    channel       TEXT    NOT NULL CHECK (channel IN ('whatsapp','sms','email','push')),
    language      TEXT    NOT NULL DEFAULT 'en',
    subject       TEXT,
    body          TEXT    NOT NULL,
    variables     TEXT[]  NOT NULL DEFAULT '{}',
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, template_id, channel, language)
);

CREATE INDEX idx_templates_tenant_id ON engagement.notification_templates (tenant_id, template_id);

-- Seed platform-level default templates
INSERT INTO engagement.notification_templates (template_id, channel, language, subject, body, variables) VALUES
    ('shipment_confirmation', 'whatsapp', 'en', NULL,
     'Hi {{customer_name}}! Your shipment {{tracking_number}} has been confirmed. Track it here: {{tracking_url}}',
     ARRAY['customer_name','tracking_number','tracking_url']),

    ('delivery_confirmed', 'whatsapp', 'en', NULL,
     'Your package {{tracking_number}} has been delivered! Thank you for shipping with us. 📦',
     ARRAY['tracking_number']),

    ('delivery_confirmed', 'email', 'en', 'Your package has been delivered - {{tracking_number}}',
     '<h2>Delivery Confirmed</h2><p>Hi {{customer_name}},</p><p>Your shipment <strong>{{tracking_number}}</strong> has been successfully delivered.</p>',
     ARRAY['customer_name','tracking_number']),

    ('delivery_failed_reschedule', 'whatsapp', 'en', NULL,
     'Hi {{customer_name}}, we attempted delivery of {{tracking_number}} but were unable to complete it. Reason: {{failed_reason}}. We will retry tomorrow. To reschedule: {{tracking_url}}',
     ARRAY['customer_name','tracking_number','failed_reason','tracking_url']),

    ('pickup_scheduled', 'whatsapp', 'en', NULL,
     'Hi {{customer_name}}! A rider will pick up your shipment today. Tracking: {{tracking_url}}',
     ARRAY['customer_name','tracking_url']);

-- Notifications log
CREATE TABLE engagement.notifications (
    id                    UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    tenant_id             UUID        NOT NULL,
    customer_id           UUID        NOT NULL,
    channel               TEXT        NOT NULL,
    recipient             TEXT        NOT NULL,
    template_id           TEXT        NOT NULL,
    rendered_body         TEXT        NOT NULL,
    subject               TEXT,
    status                TEXT        NOT NULL DEFAULT 'queued'
                                      CHECK (status IN ('queued','sending','sent','delivered','failed','bounced')),
    priority              INTEGER     NOT NULL DEFAULT 2,
    provider_message_id   TEXT,
    error_message         TEXT,
    retry_count           INTEGER     NOT NULL DEFAULT 0,
    queued_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at               TIMESTAMPTZ,
    delivered_at          TIMESTAMPTZ,
    opened_at             TIMESTAMPTZ
);

CREATE INDEX idx_notifications_tenant_status ON engagement.notifications (tenant_id, status, queued_at);
CREATE INDEX idx_notifications_customer      ON engagement.notifications (tenant_id, customer_id);

ALTER TABLE engagement.notifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE engagement.notifications FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON engagement.notifications
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
