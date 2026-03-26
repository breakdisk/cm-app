CREATE TABLE compliance.document_types (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    code              TEXT        NOT NULL UNIQUE,
    jurisdiction      TEXT        NOT NULL,
    applicable_to     TEXT[]      NOT NULL DEFAULT '{}',
    name              TEXT        NOT NULL,
    description       TEXT,
    is_required       BOOLEAN     NOT NULL DEFAULT true,
    has_expiry        BOOLEAN     NOT NULL DEFAULT true,
    warn_days_before  INT         NOT NULL DEFAULT 30,
    grace_period_days INT         NOT NULL DEFAULT 7,
    vehicle_classes   TEXT[],
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
