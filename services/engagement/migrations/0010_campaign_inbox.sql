-- Migration: 0010 — the customer's campaign inbox
--
-- Every campaign message a customer is sent is also kept for them to read in
-- the app, whatever channel carried it. A push that was dismissed, or an SMS
-- on a phone they no longer have, is still in the inbox.
--
-- The rendered title and body are stored on the send itself: a campaign's
-- template can be edited later, and the inbox must show what was sent.
--
-- `tenant_id` is new here too. campaign_sends was scoped only through
-- campaign_id -> campaigns, and that projection row is upserted best-effort
-- (a failed upsert still fans out). The inbox reads by tenant directly, so a
-- missing projection row cannot hide a customer's message or show it to the
-- wrong tenant.

ALTER TABLE engagement.campaign_sends
    ADD COLUMN IF NOT EXISTS tenant_id       UUID,
    ADD COLUMN IF NOT EXISTS inbox_title     TEXT,
    ADD COLUMN IF NOT EXISTS inbox_body      TEXT,
    ADD COLUMN IF NOT EXISTS inbox_deep_link TEXT,
    ADD COLUMN IF NOT EXISTS inbox_read_at   TIMESTAMPTZ;

-- Rows written before this migration have no title and never appear.
CREATE INDEX IF NOT EXISTS idx_campaign_sends_inbox
    ON engagement.campaign_sends (tenant_id, customer_id, queued_at DESC)
    WHERE inbox_title IS NOT NULL;
