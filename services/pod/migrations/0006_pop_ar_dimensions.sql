-- Migration 0006: Persist AR-measured box dimensioning on the Proof of Pickup.
--
-- The driver app's VERIFY-mode AR measurement (ARCore) captures the parcel's
-- L×W×H at pickup. These are folded into the POP so it serves its three audit
-- duties at the chain-of-custody open:
--   * anti-fraud    — dimension_integrity (VERIFIED|REVIEW|FLAGGED) + raw dimensions
--   * size audit    — verified_cbm / volumetric_weight_kg vs. the booked size
--   * box count     — box_quantity (number of identical boxes in the pickup)
--
-- All columns are nullable: pickups without an AR measurement (manual flow, older
-- clients) simply leave them NULL.

ALTER TABLE pod.proofs_of_pickup
    ADD COLUMN IF NOT EXISTS verified_length_cm   DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS verified_width_cm    DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS verified_height_cm   DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS verified_cbm         DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS volumetric_weight_kg DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS box_quantity         INTEGER,
    ADD COLUMN IF NOT EXISTS dimension_integrity  TEXT;

COMMENT ON COLUMN pod.proofs_of_pickup.verified_cbm IS
    'AR-measured cubic metres (L×W×H) at pickup. Audited against the booked box size.';
COMMENT ON COLUMN pod.proofs_of_pickup.volumetric_weight_kg IS
    'AR-derived dimensional weight (L×W×H ÷ 5000). Drives air-freight chargeable weight audit.';
COMMENT ON COLUMN pod.proofs_of_pickup.box_quantity IS
    'Number of identical boxes collected in this pickup (POP quantity audit).';
COMMENT ON COLUMN pod.proofs_of_pickup.dimension_integrity IS
    'Anti-fraud score of the AR measurement: VERIFIED | REVIEW | FLAGGED | PENDING.';
