-- A delivery PIN now lives until delivery (days, not 15 minutes) and is
-- required to submit a POD. A long-lived 6-digit code with unlimited guesses
-- can be brute-forced through POST /v1/otps/verify, so wrong attempts are
-- counted and the PIN locks at DELIVERY_PIN__MAX_ATTEMPTS.
--
-- Additive with a default, so the running image keeps working until the new
-- one is rolled out.

ALTER TABLE pod.otp_codes
    ADD COLUMN IF NOT EXISTS failed_attempts INTEGER NOT NULL DEFAULT 0;
