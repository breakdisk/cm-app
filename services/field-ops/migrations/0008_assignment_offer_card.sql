-- What a courier sees before deciding to take a job.
--
-- Until now an offer carried an id, a product string and a pay figure. A
-- courier could not tell three stops from one, or a chilled pharmacy run from a
-- coffee, so "accept" was a decision made blind.
--
-- A blob, not columns. Columns named `vertical` or `temperature_class` would be
-- this tier naming a product's concepts in its own schema -- the interpretation
-- ADR-0015 says a platform tier must not do -- and would foreclose a third
-- product whose concepts differ. field-ops stores this and returns it verbatim;
-- it never reads a key of it, exactly as it never resolves `external_ref`.
--
-- `offer_to_nearest` fans out, so everything in here is disclosed to every
-- courier merely *considered* for the job, most of whom will not get it. It
-- therefore carries no customer identity and no street addresses at all --
-- those arrive with the product's own manifest, after the claim.
--
-- No index, deliberately. An index would imply something here is queried, and
-- nothing in this service may query into it. `scripts/check-offer-card-opacity.sh`
-- fails the build if anything ever does.
ALTER TABLE field_ops.courier_assignments
    ADD COLUMN IF NOT EXISTS offer_card JSONB;

COMMENT ON COLUMN field_ops.courier_assignments.offer_card IS
  'Opaque product-supplied summary, stored and returned verbatim, never read by '
  'field-ops. Disclosed to every courier in the fanout, so it carries no '
  'customer identity and no street addresses.';
