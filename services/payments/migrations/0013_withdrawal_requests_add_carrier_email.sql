-- Add carrier_email to withdrawal_requests so that the engagement notification
-- at disburse/reject time knows who to email without a cross-service lookup.
-- The column is optional: older rows and rows submitted before the partner portal
-- began sending the field will have NULL, which the notification service handles
-- gracefully (skips email channel, push notification still fires).

ALTER TABLE payments.withdrawal_requests
    ADD COLUMN IF NOT EXISTS carrier_email TEXT;
