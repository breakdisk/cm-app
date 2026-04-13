-- Add customer booking fields for B2C self-service shipments.
-- customer_email: for payment receipt delivery
-- booked_by_customer: true when booking originates from customer app (drives PaymentReceipt vs invoice)

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS customer_email      TEXT,
    ADD COLUMN IF NOT EXISTS booked_by_customer  BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN order_intake.shipments.customer_email
    IS 'Email for the customer/recipient — used for payment receipt delivery.';
COMMENT ON COLUMN order_intake.shipments.booked_by_customer
    IS 'True when the shipment was self-booked via the customer app (B2C). Triggers PaymentReceipt at POD.';
