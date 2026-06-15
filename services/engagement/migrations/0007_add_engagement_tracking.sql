-- Migration: 0007 — Engagement tracking: open/click/read receipt columns
--
-- Adds delivery-receipt columns to campaign_sends so provider webhooks
-- (WhatsApp read receipts, Twilio SMS status, email open/click events)
-- can be written back to the row without a separate tracking table.
--
-- All new columns are NULLABLE — absence means the event has not occurred.

ALTER TABLE engagement.campaign_sends
    ADD COLUMN IF NOT EXISTS opened_at   TIMESTAMPTZ,  -- email/push open receipt
    ADD COLUMN IF NOT EXISTS clicked_at  TIMESTAMPTZ,  -- email click-through receipt
    ADD COLUMN IF NOT EXISTS clicked_url TEXT;          -- first clicked URL from email

-- Add 'read' status check if not already present (safe no-op if constraint exists).
-- The 'read' value is already in the CHECK list from migration 0002 — this adds
-- the corresponding timestamp column.
ALTER TABLE engagement.campaign_sends
    ADD COLUMN IF NOT EXISTS bounced_at  TIMESTAMPTZ;  -- hard-bounce timestamp

-- Index for provider-message-id lookups (receipt correlation).
-- Already created in 0002 but may be partial — recreate as full index.
CREATE INDEX IF NOT EXISTS idx_campaign_sends_provider_lookup
    ON engagement.campaign_sends (provider_message_id, tenant_id)
    WHERE provider_message_id IS NOT NULL;
