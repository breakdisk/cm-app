-- Migration: 0025 — how precisely each destination was geocoded
--
-- The neighborhood store resolver allocates a parcel to a store only when the
-- destination is precise enough, so it needs to know how precise each geocode
-- was. Every existing shipment reads as UNKNOWN. That is the safe reading and
-- it blocks nothing: this gate applies only to the store resolver, never to
-- dispatch.
--
-- No CHECK on quality: values cross service boundaries and a new level must be
-- data, not DDL. A value this build does not recognise reads as UNKNOWN.
--
-- source:     provider | customer_pin | ops_corrected
-- place_id:   the provider's id (Mapbox mapbox_id). Only stored when Mapbox
--             permanent geocoding is on (MAPBOX__PERMANENT_GEOCODING).
-- confidence: the provider's own confidence word, same condition.

ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS dest_geocoding_quality    TEXT NOT NULL DEFAULT 'UNKNOWN',
    ADD COLUMN IF NOT EXISTS dest_geocoding_source     TEXT,
    ADD COLUMN IF NOT EXISTS dest_geocoding_provider   TEXT,
    ADD COLUMN IF NOT EXISTS dest_geocoding_place_id   TEXT,
    ADD COLUMN IF NOT EXISTS dest_geocoding_confidence TEXT,
    ADD COLUMN IF NOT EXISTS dest_geocoded_at          TIMESTAMPTZ;
