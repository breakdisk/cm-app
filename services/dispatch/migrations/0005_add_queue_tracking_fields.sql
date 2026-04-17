-- Migration: 0005 — Add tracking_number and customer_email to dispatch_queue
-- These fields flow from ShipmentCreated → dispatch_queue → TaskAssigned → driver_ops.tasks
-- so engagement can send receipts at each lifecycle stage without cross-service queries.

ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS customer_email  TEXT;
ALTER TABLE dispatch.dispatch_queue ADD COLUMN IF NOT EXISTS tracking_number TEXT;
