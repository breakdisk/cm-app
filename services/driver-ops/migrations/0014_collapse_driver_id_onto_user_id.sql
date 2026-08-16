-- Collapse `drivers.id` onto `drivers.user_id` (ADR-0015).
--
-- The table carried two candidate keys and the codebase quietly disagreed about
-- which one `driver_id` meant. `driver_locations.driver_id` holds a *user_id*
-- despite its name; `tasks.driver_id` holds a `drivers.id`; and
-- `task_repo::list_by_driver` carried a defensive join whose comment admitted
-- the two "may differ for API-registered drivers". A column that means one
-- thing in the GPS path and another in the task path is a silent-wrong-answer
-- waiting for the first row that takes the other branch: proximity search
-- returns a courier who is not there, or drops one who is, with no error.
--
-- Both creation paths already set `id = user_id` (`driver_service.rs` and
-- `location_service::find_or_create_driver`). What kept the ambiguity alive was
-- the `gen_random_uuid()` default, which let any direct INSERT — or any future
-- code path that forgot — mint a divergent row. This makes the convention an
-- invariant the database enforces.
--
-- Doing it now is the whole point: production has 6 drivers and 0 divergent, so
-- this is a constraint addition. After the first divergent row it becomes a
-- data migration with FK reconciliation across tasks, locations and routes.

-- Refuse rather than corrupt. If any row diverges, this migration must fail
-- loudly and be replaced by a reconciliation plan — silently rewriting ids
-- would repoint tasks and GPS history at the wrong driver.
DO $$
DECLARE divergent BIGINT;
BEGIN
    SELECT count(*) INTO divergent FROM driver_ops.drivers WHERE id <> user_id;
    IF divergent > 0 THEN
        RAISE EXCEPTION
            'Cannot collapse driver ids: % row(s) have id <> user_id. '
            'Reconcile driver_ops.tasks.driver_id, driver_ops.driver_locations.driver_id '
            'and any route references first — see ADR-0015.', divergent;
    END IF;
END $$;

-- A new row can no longer invent an id. Callers must supply it, and the check
-- below means the only value they can supply is the user_id.
ALTER TABLE driver_ops.drivers ALTER COLUMN id DROP DEFAULT;

ALTER TABLE driver_ops.drivers
    DROP CONSTRAINT IF EXISTS drivers_id_is_user_id;
ALTER TABLE driver_ops.drivers
    ADD CONSTRAINT drivers_id_is_user_id CHECK (id = user_id);

COMMENT ON COLUMN driver_ops.drivers.id IS
  'Always equal to user_id (identity.users.id), enforced by drivers_id_is_user_id. '
  'One identity for a field worker across driver_ops and dispatch, so `driver_id` '
  'means the same thing in every table that carries it.';
