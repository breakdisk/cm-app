-- Record whether the delivery PIN ever reached the recipient, so a delivery is
-- refused for a missing PIN only when the recipient actually had one.
-- Additive and nullable: the running image ignores these columns.
-- Rows created before this migration have delivered_via NULL (unknown).
ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_via TEXT;
ALTER TABLE pod.otp_codes ADD COLUMN IF NOT EXISTS delivered_at TIMESTAMPTZ;
ALTER TABLE pod.proofs    ADD COLUMN IF NOT EXISTS pin_exception TEXT;
