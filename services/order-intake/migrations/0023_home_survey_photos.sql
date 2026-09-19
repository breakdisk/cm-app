-- Migration: 0023 — the survey's photos
--
-- The typed room list prices the move; photos are its context. Two kinds:
--   condition — pre-existing condition evidence (an antique's scratches, a
--               television's cracked corner), protecting customer, lead and
--               platform from a false damage claim later;
--   access    — the bottlenecks (a narrow lift, a tight stair turn, a long
--               carry) so no crew is blindsided on the day.
-- The kind is validated in the service, not by a CHECK: a new kind of photo
-- is data. The image lives in the pod service's media store under
-- survey/{tenant}/{shipment}/; this row is what the move shows.

CREATE TABLE IF NOT EXISTS order_intake.home_survey_photos (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    shipment_id      UUID        NOT NULL,
    tenant_id        UUID        NOT NULL,
    uploaded_by      UUID        NOT NULL,
    kind             TEXT        NOT NULL,
    room             TEXT,
    caption          TEXT        NOT NULL DEFAULT '',
    object_key       TEXT        NOT NULL,
    content_type     TEXT        NOT NULL,
    size_bytes       BIGINT      NOT NULL,
    -- The shutter, on the lead's phone; created_at is when it reached us.
    device_timestamp TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT home_survey_photos_once UNIQUE (object_key)
);

CREATE INDEX IF NOT EXISTS home_survey_photos_by_move
    ON order_intake.home_survey_photos (shipment_id, created_at);
