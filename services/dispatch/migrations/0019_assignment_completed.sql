-- Assignments can end in `completed` (the driver is freed when the work ends).
-- Reader half: this ships and deploys before anything writes the value.
-- The partial unique indexes (uq_driver_active_assignment,
-- idx_one_active_assignment_per_driver) only cover ('pending','accepted'),
-- so a completed row never blocks a new assignment.
ALTER TABLE dispatch.driver_assignments
    DROP CONSTRAINT IF EXISTS driver_assignments_status_check;
ALTER TABLE dispatch.driver_assignments
    ADD CONSTRAINT driver_assignments_status_check
    CHECK (status IN ('pending', 'accepted', 'rejected', 'cancelled', 'completed'));
