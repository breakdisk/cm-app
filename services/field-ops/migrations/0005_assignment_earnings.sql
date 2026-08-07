-- What the courier earns for this job.
--
-- The product declares the earning when it asks for the work, rather than
-- field-ops deriving it: pricing is a product decision (OmniDeliv's flat fee
-- and tip are not LogisticOS's per-parcel rate), and a platform tier that
-- computed pay would need to know every product's tariff.
--
-- field-ops treats these as opaque amounts — it credits them on delivery and
-- never interprets how they were arrived at, the same opacity it applies to
-- external_ref.
ALTER TABLE field_ops.courier_assignments
    ADD COLUMN IF NOT EXISTS trip_cents BIGINT NOT NULL DEFAULT 0 CHECK (trip_cents >= 0),
    ADD COLUMN IF NOT EXISTS tip_cents  BIGINT NOT NULL DEFAULT 0 CHECK (tip_cents  >= 0);
