-- Which identity user is carrying this order.
--
-- `courier_task_id` is a field-ops *assignment* id, and field-ops' own
-- `courier_id` is its key for the person -- neither can be compared against the
-- `user_id` in a courier's JWT. The driver manifest authorizes on exactly that
-- comparison, and resolving it per request would put a polled endpoint on
-- another service's availability.
--
-- Populated from CourierEvent::Assigned, which already carried courier_id and
-- now carries the user id alongside it.
--
-- Nullable: orders claimed before this column existed have none. The manifest
-- refuses those rather than falling open -- "we do not know who is carrying
-- this" must never read as "anyone may look".
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS courier_user_id UUID;

CREATE INDEX IF NOT EXISTS idx_orders_courier_user
    ON omnideliv.orders (tenant_id, courier_user_id)
    WHERE courier_user_id IS NOT NULL;
