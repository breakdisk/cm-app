-- Record whether the delivery PIN ever reached the recipient, so a delivery is
-- refused for a missing PIN only when the recipient actually had one.
-- Additive and nullable: the running image ignores these columns.
-- Rows created before this migration have delivered_via NULL (unknown).
ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_via TEXT;
ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_at TIMESTAMPTZ;
ALTER TABLE pod.proofs    ADD COLUMN IF NOT EXISTS pin_exception TEXT;

-- Backfill. A PIN issued before this migration has no delivery record, but it
-- may well have reached the recipient (Twilio SMS, or shown on the customer
-- app's booking screen), and PINs live up to 60 days. Leaving NULL would make
-- every such PIN skippable on a photo. 'legacy' counts as delivered, so those
-- deliveries keep the old rule: a missing PIN is refused.
UPDATE pod.otp_codes
   SET delivered_via = 'legacy', delivered_at = created_at
 WHERE delivered_via IS NULL;
