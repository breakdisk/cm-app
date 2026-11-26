-- Collapse `couriers.id` onto `couriers.user_id` (ADR-0015), mirroring
-- driver-ops migration 0014 for `driver_ops.drivers`.
--
-- Registration already sets `id = user_id` (dispatch_service register path),
-- but the column kept `DEFAULT gen_random_uuid()` and nothing enforced it, so
-- any direct INSERT could mint a courier whose id differs from the identity
-- user UUID — and `courier_id` would then mean different things in
-- assignments, ledger and locations. This makes the convention an invariant.

-- Refuse rather than corrupt. If any row diverges, fail loudly and reconcile
-- by hand: silently rewriting ids would repoint assignments, ledger entries
-- and GPS history at the wrong courier.
DO $$
DECLARE divergent BIGINT;
BEGIN
    SELECT count(*) INTO divergent FROM field_ops.couriers WHERE id <> user_id;
    IF divergent > 0 THEN
        RAISE EXCEPTION
            'Cannot collapse courier ids: % row(s) have id <> user_id. '
            'Reconcile field_ops.assignments.courier_id, field_ops.courier_ledgers.courier_id '
            'and field_ops.courier_locations.courier_id first — see ADR-0015.', divergent;
    END IF;
END $$;

ALTER TABLE field_ops.couriers ALTER COLUMN id DROP DEFAULT;

ALTER TABLE field_ops.couriers
    DROP CONSTRAINT IF EXISTS couriers_id_is_user_id;
ALTER TABLE field_ops.couriers
    ADD CONSTRAINT couriers_id_is_user_id CHECK (id = user_id);

COMMENT ON COLUMN field_ops.couriers.id IS
  'Always equal to user_id (identity.users.id), enforced by couriers_id_is_user_id. '
  'One identity for a field worker across field_ops, driver_ops and dispatch.';
