-- Courier compliance status, denormalised from the compliance service.
--
-- WHY A COLUMN AND NOT A REDIS CACHE.
-- The sibling tier (dispatch, for driver-ops drivers) keeps this in Redis with
-- a 5-minute TTL and fails open on a miss. That is fine there because a driver
-- who is wrongly offered work is corrected within the TTL. It is the wrong
-- shape here for two reasons:
--
--   1. field-ops has no Redis dependency at all, and a cache that is empty on
--      every boot fails open until the next status change publishes — which for
--      a compliant-and-stable courier may be never.
--   2. "Why is this courier not getting jobs?" already has three independent
--      answers that look identical from outside (suspended by ops / off duty /
--      GPS fix older than 10 minutes). Compliance is a fourth. Putting it in a
--      cache the ops roster cannot read would make it the one nobody can see,
--      which is exactly the problem the roster's Dispatchable column exists to
--      solve. A column is on the row the roster already selects.
--
-- NULL status means compliance has never spoken about this courier, which is
-- the state every courier alive today is in: nothing has ever published
-- `driver.registered`, so no courier has a compliance profile. Unknown must
-- therefore fail OPEN — `compliance_assignable` defaults to true — or turning
-- this on would stop the entire live fleet. The distinction between "unknown"
-- (status NULL) and "known good" (status 'compliant') is visible on the roster
-- so ops can tell an un-onboarded courier from a cleared one.
ALTER TABLE field_ops.couriers
    ADD COLUMN IF NOT EXISTS compliance_status     TEXT,
    ADD COLUMN IF NOT EXISTS compliance_assignable BOOLEAN     NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS compliance_updated_at TIMESTAMPTZ;

-- DELIBERATELY NO INDEX CHANGE.
-- The obvious move is to narrow idx_courier_tenant_status to
-- `WHERE is_active AND compliance_assignable`. That is wrong here: PostgreSQL
-- only uses a partial index when the query's predicate *implies* the index's,
-- and this ships enforcing = false by default, so find_available_near carries
-- no compliance predicate at all on day one. The narrowed index would simply
-- stop being used, turning every supply lookup into a scan — a performance
-- regression bought by an optimisation that cannot apply until a flag flips.
-- The compliance test is a boolean on rows already narrowed by tenant, status
-- and a GiST-indexed proximity join; it does not need its own index.
