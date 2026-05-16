-- Fix structural gaps in the engagement campaign tables introduced in 0002.
--
-- (a) Make template_id nullable on engagement.campaigns so that campaign rows
--     can be upserted from CAMPAIGN_TRIGGERED events where the template is
--     inline (not registered in engagement.templates).
--
-- (b) Drop the FK from campaign_sends.campaign_id → engagement.campaigns(id).
--     Campaign IDs are owned by the marketing service (marketing.campaigns),
--     not the engagement service.  The FK was causing every campaign fan-out
--     to fail with a constraint violation because nothing ever wrote to
--     engagement.campaigns before the send records were inserted.
--     Tenant isolation is enforced by filtering through the campaign_id which
--     is already scoped to a tenant by the marketing-service RLS policy.
ALTER TABLE engagement.campaigns
    ALTER COLUMN template_id DROP NOT NULL;

ALTER TABLE engagement.campaign_sends
    DROP CONSTRAINT IF EXISTS campaign_sends_campaign_id_fkey;
