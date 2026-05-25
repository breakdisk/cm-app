-- Migration: 0005 — Link containers to a truck spec for capacity tracking.
--
-- This is a soft link — existing containers without a spec are still valid.
-- The spec is used by the consolidation engine and 3D viewer; it is not
-- enforced at the DB level for backward compatibility.

ALTER TABLE hub_ops.containers
    ADD COLUMN IF NOT EXISTS truck_spec_id UUID REFERENCES hub_ops.truck_specs(id);
