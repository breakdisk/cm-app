-- Fix send_log.template_id column type: UUID → TEXT.
-- The frontend generates inline template IDs like "inline_1234567890" which
-- are not valid UUIDs. Campaigns also reference engagement service template keys
-- (free-form strings) rather than UUIDs.
ALTER TABLE marketing.send_log
    ALTER COLUMN template_id TYPE TEXT USING template_id::text;
