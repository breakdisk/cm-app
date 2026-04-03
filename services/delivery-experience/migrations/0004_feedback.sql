-- Delivery feedback submitted by customers after a completed delivery.
CREATE TABLE IF NOT EXISTS delivery_experience.feedback (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tracking_number TEXT        NOT NULL,
    tenant_id       UUID,
    rating          SMALLINT    NOT NULL CHECK (rating BETWEEN 1 AND 5),
    tags            TEXT[]      NOT NULL DEFAULT '{}',
    comments        TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS feedback_tracking_number_idx
    ON delivery_experience.feedback (tracking_number);
