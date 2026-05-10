-- Migration: 0008 — Add customer_id to tasks
-- Populated from TaskAssigned events (dispatch forwards customer_id from the
-- dispatch_queue row). Used by TaskCompleted events so engagement can route
-- delivery receipt notifications to the correct customer profile.

ALTER TABLE driver_ops.tasks ADD COLUMN IF NOT EXISTS customer_id UUID;
