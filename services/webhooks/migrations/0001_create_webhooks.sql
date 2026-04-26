-- Webhooks: tenant-scoped subscriptions to platform events.
-- Each row is one outbound URL the tenant wants notified when events fire.
-- The dispatcher (Kafka consumer in this service) signs each delivery with
-- HMAC-SHA256(secret, body) and includes the signature in `X-LogisticOS-Signature`.

CREATE SCHEMA IF NOT EXISTS webhooks;

CREATE TABLE IF NOT EXISTS webhooks.webhooks (
    id              UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID         NOT NULL,
    url             TEXT         NOT NULL,
    -- Array of event types this webhook subscribes to. Wildcard "*" matches everything.
    -- Stored as TEXT[] rather than JSONB so a GIN index gives us efficient `event_type = ANY(events)` lookups.
    events          TEXT[]       NOT NULL DEFAULT ARRAY[]::TEXT[],
    -- HMAC signing secret. Generated on create + returned exactly once; never re-readable.
    secret          TEXT         NOT NULL,
    -- 'active' | 'disabled'. Disabled webhooks are skipped by the dispatcher
    -- but kept around so the admin can re-enable without losing history.
    status          TEXT         NOT NULL DEFAULT 'active',
    description     TEXT,
    -- Cumulative counters maintained by the dispatcher. Reset only via migration.
    success_count   BIGINT       NOT NULL DEFAULT 0,
    fail_count      BIGINT       NOT NULL DEFAULT 0,
    last_delivery_at  TIMESTAMPTZ,
    last_status_code  INTEGER,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhooks_tenant
    ON webhooks.webhooks (tenant_id, status);

-- GIN index supports `WHERE events && ARRAY['shipment.created']` (overlap)
-- which is the dispatcher's hot path: given an event_type, find all webhooks
-- in the tenant that subscribe to it.
CREATE INDEX IF NOT EXISTS idx_webhooks_events_gin
    ON webhooks.webhooks USING GIN (events);

-- ─── Delivery History ─────────────────────────────────────────────────────────
-- Append-heavy, retained 30 days for debugging. Per-attempt row so the
-- dispatcher can record retries with their individual outcomes.

CREATE TABLE IF NOT EXISTS webhooks.deliveries (
    id              UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_id      UUID         NOT NULL REFERENCES webhooks.webhooks(id) ON DELETE CASCADE,
    tenant_id       UUID         NOT NULL,    -- denormalized for RLS scoping
    event_type      TEXT         NOT NULL,
    -- Full event payload sent to the URL (post-signing). Trimmed to 64KB by
    -- the dispatcher before insert; oversized events get a placeholder.
    payload         JSONB        NOT NULL,
    -- 0 = first attempt; bumped on retry. Capped at MAX_RETRY_ATTEMPTS in code.
    attempt         INTEGER      NOT NULL DEFAULT 0,
    -- HTTP response from the receiver. status_code = 0 means request never
    -- left this side (timeout / DNS / TLS failure); the error text goes to
    -- response_body so the admin sees the same struct.
    status_code     INTEGER      NOT NULL DEFAULT 0,
    response_body   TEXT,
    duration_ms     INTEGER      NOT NULL DEFAULT 0,
    delivered_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_deliveries_webhook
    ON webhooks.deliveries (webhook_id, delivered_at DESC);

CREATE INDEX IF NOT EXISTS idx_deliveries_tenant
    ON webhooks.deliveries (tenant_id, delivered_at DESC);

-- ─── Row-Level Security ───────────────────────────────────────────────────────
-- ADR-0003 compliance — tenant_id from JWT is set on every connection via SET app.tenant_id.

ALTER TABLE webhooks.webhooks   ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhooks.deliveries ENABLE ROW LEVEL SECURITY;

CREATE POLICY webhooks_tenant_isolation ON webhooks.webhooks
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

CREATE POLICY deliveries_tenant_isolation ON webhooks.deliveries
    USING (tenant_id = current_setting('app.tenant_id', true)::UUID);

-- Service-role bypass: the dispatcher service runs without app.tenant_id
-- set (it processes all tenants). Connection-level FORCE ROW LEVEL is
-- intentionally NOT applied so the service role's superuser-equivalent
-- access still works for cross-tenant aggregations.
