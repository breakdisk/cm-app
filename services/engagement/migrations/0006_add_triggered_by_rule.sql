-- Migration 0006: Track which automation rule triggered a campaign send.
-- Enables the communication history tab in the merchant portal to show
-- "Auto-triggered by rule: <rule_name>" alongside manually-sent campaigns.

ALTER TABLE engagement.campaign_sends
    ADD COLUMN IF NOT EXISTS triggered_by_rule_id   UUID,
    ADD COLUMN IF NOT EXISTS triggered_by_rule_name  TEXT;

CREATE INDEX IF NOT EXISTS idx_campaign_sends_rule_id
    ON engagement.campaign_sends (triggered_by_rule_id)
    WHERE triggered_by_rule_id IS NOT NULL;
