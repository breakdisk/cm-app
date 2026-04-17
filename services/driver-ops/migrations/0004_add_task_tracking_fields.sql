-- Migration: 0004 — Add tracking_number and customer_email to tasks
-- These fields are populated from TaskAssigned events and denormalized into
-- TaskCompleted/TaskFailed events so engagement can send receipts without
-- cross-service queries.

ALTER TABLE driver_ops.tasks ADD COLUMN IF NOT EXISTS customer_email  TEXT;
ALTER TABLE driver_ops.tasks ADD COLUMN IF NOT EXISTS tracking_number TEXT;
