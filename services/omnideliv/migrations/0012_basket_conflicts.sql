-- What reconcile found while verifying a mesh run's proposed lines.
--
-- Persisted rather than left to the SSE stream alone. `ConstraintDetected`
-- events reach Screen B while the run is in flight, but a customer who taps
-- through to review before the stream finishes never reads them — and a
-- blocking conflict is precisely the thing they must see, because their basket
-- is missing a line they asked for.
--
-- On the basket, not on a table of its own: conflicts belong to one run's
-- verification of one basket, are read only with that basket, and are replaced
-- wholesale when a run re-verifies. A child table would buy history nobody
-- queries and cost a join on the checkout read path.
ALTER TABLE omnideliv.baskets
    ADD COLUMN IF NOT EXISTS conflicts JSONB NOT NULL DEFAULT '[]'::JSONB;

COMMENT ON COLUMN omnideliv.baskets.conflicts IS
  'Array of {kind, blocking, description} from mesh reconcile. Blocking entries '
  'have already had their line removed from the basket; advisory entries are '
  'shown to the customer to weigh. Written by the mesh, read by Screen C.';
