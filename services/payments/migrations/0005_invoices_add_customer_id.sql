-- Migration 0005: Add customer_id to invoices for B2C payment receipt lookup.
--
-- PaymentReceipt invoices are addressed to a customer (not a merchant).
-- This column allows GET /v1/customers/:id/invoices to list a customer's receipts.
-- Merchant invoices leave this NULL.

ALTER TABLE payments.invoices
    ADD COLUMN IF NOT EXISTS customer_id UUID;

-- Index for the customer-facing list endpoint
CREATE INDEX IF NOT EXISTS idx_invoices_customer_id
    ON payments.invoices (customer_id)
    WHERE customer_id IS NOT NULL;

COMMENT ON COLUMN payments.invoices.customer_id IS
    'Set on PaymentReceipt invoices (B2C flow). NULL for merchant/B2B invoices.';
