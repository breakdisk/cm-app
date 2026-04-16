-- Migration 0007: receipt_email_requests table
-- Customer-initiated "email me my receipt" requests.
-- The engagement engine polls this table (or consumes a Kafka event) and
-- sends the formatted receipt email to the customer.

CREATE TABLE IF NOT EXISTS tracking.receipt_email_requests (
    tracking_number  TEXT        NOT NULL PRIMARY KEY,
    email            TEXT        NOT NULL,
    requested_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at          TIMESTAMPTZ,          -- set by engagement engine on successful send
    failed_reason    TEXT                  -- set on send failure
);

CREATE INDEX IF NOT EXISTS idx_receipt_email_requests_pending
    ON tracking.receipt_email_requests (requested_at)
    WHERE sent_at IS NULL;
