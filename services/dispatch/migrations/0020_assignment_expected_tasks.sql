-- The number of TASK_ASSIGNED legs dispatch emitted for the assignment
-- (pickup + delivery = 2, delivery only = 1). The close predicate waits for
-- this many tasks to exist, so a completed pickup cannot close an assignment
-- whose delivery leg is late or lost. NULL = legacy row: the close falls back
-- to counting route stops.
ALTER TABLE dispatch.driver_assignments
    ADD COLUMN IF NOT EXISTS expected_tasks INT;
