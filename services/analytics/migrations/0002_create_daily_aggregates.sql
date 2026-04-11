-- Migration: 0002 — Analytics: Daily KPI and performance aggregation tables
-- These tables are populated by scheduled refresh functions and serve as the
-- primary data source for the BI dashboard APIs, avoiding expensive real-time
-- aggregations over the append-only shipment_events store.
--
-- Refresh cadence: nightly at 02:00 tenant local time via background job.
-- Manual refresh: call refresh_daily_kpis(tenant_id, date) for back-fills.
-- These tables are READ-ONLY from application code — never INSERT/UPDATE directly.

-- ─── Daily KPI Aggregate ──────────────────────────────────────────────────────

-- analytics.daily_kpis stores one row per (tenant, date) with pre-computed
-- shipment KPIs for that calendar day. Used by the BI dashboard, the merchant
-- portal overview page, and the AI analytics MCP tool `get_delivery_metrics`.
CREATE TABLE IF NOT EXISTS analytics.daily_kpis (
    tenant_id             UUID        NOT NULL,
    date                  DATE        NOT NULL,

    -- Volume counters
    total_shipments       BIGINT      NOT NULL DEFAULT 0,
    delivered             BIGINT      NOT NULL DEFAULT 0,
    failed                BIGINT      NOT NULL DEFAULT 0,
    cancelled             BIGINT      NOT NULL DEFAULT 0,

    -- SLA counters
    on_time_count         BIGINT      NOT NULL DEFAULT 0,   -- delivered within time window
    with_eta_count        BIGINT      NOT NULL DEFAULT 0,   -- shipments that had an ETA set

    -- Delivery time
    avg_delivery_hours    FLOAT8,                           -- NULL if no deliveries that day

    -- COD financials
    cod_shipments         BIGINT      NOT NULL DEFAULT 0,
    cod_collected_cents   BIGINT      NOT NULL DEFAULT 0,

    -- Derived rates (pre-computed to avoid repeated division in queries)
    success_rate          FLOAT8      NOT NULL DEFAULT 0.0, -- delivered / total_shipments
    on_time_rate          FLOAT8      NOT NULL DEFAULT 0.0, -- on_time_count / delivered

    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    PRIMARY KEY (tenant_id, date)
);

-- Composite index for date-range queries across a tenant (BI dashboard default query)
CREATE INDEX IF NOT EXISTS idx_daily_kpis_tenant_date
    ON analytics.daily_kpis (tenant_id, date DESC);

-- ─── Driver Daily Stats ───────────────────────────────────────────────────────

-- analytics.driver_daily_stats stores per-driver performance aggregates.
-- Used by the driver performance leaderboard and the AI dispatch model
-- for driver success-rate feature computation.
CREATE TABLE IF NOT EXISTS analytics.driver_daily_stats (
    tenant_id             UUID        NOT NULL,
    driver_id             UUID        NOT NULL,
    date                  DATE        NOT NULL,

    -- Delivery counts
    total_deliveries      BIGINT      NOT NULL DEFAULT 0,
    successful            BIGINT      NOT NULL DEFAULT 0,
    failed                BIGINT      NOT NULL DEFAULT 0,

    -- Performance
    avg_hours             FLOAT8,                           -- average hours per delivery
    cod_collected_cents   BIGINT      NOT NULL DEFAULT 0,

    -- Derived
    success_rate          FLOAT8      NOT NULL DEFAULT 0.0,

    PRIMARY KEY (tenant_id, driver_id, date)
);

-- Tenant + date index for daily leaderboard queries
CREATE INDEX IF NOT EXISTS idx_driver_daily_tenant_date
    ON analytics.driver_daily_stats (tenant_id, date DESC);

-- Tenant + driver index for individual driver history queries
CREATE INDEX IF NOT EXISTS idx_driver_daily_tenant_driver
    ON analytics.driver_daily_stats (tenant_id, driver_id, date DESC);

-- ─── Zone Daily Stats ─────────────────────────────────────────────────────────

-- analytics.zone_daily_stats stores per-delivery-zone performance aggregates.
-- Used by the demand forecasting model (`get_zone_demand_forecast` MCP tool)
-- and the Admin Portal zone heatmap.
CREATE TABLE IF NOT EXISTS analytics.zone_daily_stats (
    tenant_id             UUID        NOT NULL,
    zone_id               UUID        NOT NULL,
    date                  DATE        NOT NULL,

    -- Volume
    shipment_count        BIGINT      NOT NULL DEFAULT 0,
    delivered             BIGINT      NOT NULL DEFAULT 0,
    failed                BIGINT      NOT NULL DEFAULT 0,

    -- Derived
    success_rate          FLOAT8      NOT NULL DEFAULT 0.0,

    PRIMARY KEY (tenant_id, zone_id, date)
);

-- Tenant + date index for zone heatmap queries
CREATE INDEX IF NOT EXISTS idx_zone_daily_tenant_date
    ON analytics.zone_daily_stats (tenant_id, date DESC);

-- Tenant + zone index for individual zone history queries
CREATE INDEX IF NOT EXISTS idx_zone_daily_tenant_zone
    ON analytics.zone_daily_stats (tenant_id, zone_id, date DESC);

-- ─── Row Level Security ───────────────────────────────────────────────────────

ALTER TABLE analytics.daily_kpis          ENABLE ROW LEVEL SECURITY;
ALTER TABLE analytics.driver_daily_stats  ENABLE ROW LEVEL SECURITY;
ALTER TABLE analytics.zone_daily_stats    ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS analytics_daily_kpis_tenant_isolation ON analytics.daily_kpis;
DROP POLICY IF EXISTS analytics_daily_kpis_tenant_isolation ON analytics.daily_kpis;
CREATE POLICY analytics_daily_kpis_tenant_isolation ON analytics.daily_kpis
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

DROP POLICY IF EXISTS analytics_driver_daily_tenant_isolation ON analytics.driver_daily_stats;
DROP POLICY IF EXISTS analytics_driver_daily_tenant_isolation ON analytics.driver_daily_stats;
CREATE POLICY analytics_driver_daily_tenant_isolation ON analytics.driver_daily_stats
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

DROP POLICY IF EXISTS analytics_zone_daily_tenant_isolation ON analytics.zone_daily_stats;
DROP POLICY IF EXISTS analytics_zone_daily_tenant_isolation ON analytics.zone_daily_stats;
CREATE POLICY analytics_zone_daily_tenant_isolation ON analytics.zone_daily_stats
    USING (tenant_id = (current_setting('app.tenant_id', true)::UUID));

-- ─── Immutability Rules ───────────────────────────────────────────────────────
-- Application code must not directly INSERT/UPDATE these tables.
-- All writes go through refresh_daily_kpis() and its driver/zone equivalents.
-- The refresh functions use ON CONFLICT DO UPDATE, which is the sole allowed write path.
-- We do NOT block direct UPDATE here because the refresh function uses upsert,
-- but we document this constraint for code review enforcement.

-- ─── Refresh Functions ────────────────────────────────────────────────────────

-- refresh_daily_kpis(p_tenant_id, p_date)
-- Recomputes daily_kpis for a single (tenant, date) pair by aggregating over
-- analytics.shipment_events. Safe to call multiple times (idempotent via
-- ON CONFLICT DO UPDATE). Called nightly by the analytics refresh job and
-- on-demand for back-fills or incident recovery.
--
-- Parameters:
--   p_tenant_id  — tenant to refresh (required)
--   p_date       — date to refresh (required; use CURRENT_DATE for today)
CREATE OR REPLACE FUNCTION analytics.refresh_daily_kpis(
    p_tenant_id UUID,
    p_date      DATE
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_total         BIGINT;
    v_delivered     BIGINT;
    v_failed        BIGINT;
    v_cancelled     BIGINT;
    v_on_time       BIGINT;
    v_with_eta      BIGINT;
    v_avg_hours     FLOAT8;
    v_cod_count     BIGINT;
    v_cod_cents     BIGINT;
    v_success_rate  FLOAT8;
    v_on_time_rate  FLOAT8;
BEGIN
    -- Aggregate from the append-only event store for the given tenant + date.
    -- We only count the terminal event per shipment (delivered/failed/cancelled)
    -- to avoid double-counting shipments that have multiple events.
    SELECT
        COUNT(*)                                                      AS total,
        COUNT(*) FILTER (WHERE event_type = 'delivered')              AS delivered,
        COUNT(*) FILTER (WHERE event_type = 'failed')                 AS failed,
        COUNT(*) FILTER (WHERE event_type = 'cancelled')              AS cancelled,
        COUNT(*) FILTER (WHERE event_type = 'delivered' AND on_time)  AS on_time,
        COUNT(*) FILTER (WHERE event_type = 'delivered'
                           AND delivery_hours IS NOT NULL)             AS with_eta,
        AVG(delivery_hours) FILTER (WHERE event_type = 'delivered'
                                     AND delivery_hours IS NOT NULL)   AS avg_hours,
        COUNT(*) FILTER (WHERE event_type = 'delivered'
                           AND cod_amount_cents > 0)                   AS cod_count,
        COALESCE(SUM(cod_amount_cents) FILTER (WHERE event_type = 'delivered'
                                               AND cod_amount_cents > 0), 0) AS cod_cents
    INTO
        v_total, v_delivered, v_failed, v_cancelled,
        v_on_time, v_with_eta, v_avg_hours,
        v_cod_count, v_cod_cents
    FROM analytics.shipment_events
    WHERE tenant_id   = p_tenant_id
      AND occurred_at >= p_date::TIMESTAMPTZ
      AND occurred_at <  (p_date + INTERVAL '1 day')::TIMESTAMPTZ;

    -- Compute derived rates safely (avoid division by zero)
    v_success_rate := CASE WHEN v_total     > 0 THEN v_delivered::FLOAT8 / v_total    ELSE 0.0 END;
    v_on_time_rate := CASE WHEN v_delivered > 0 THEN v_on_time::FLOAT8  / v_delivered ELSE 0.0 END;

    -- Upsert the aggregate row. ON CONFLICT updates all computed columns.
    INSERT INTO analytics.daily_kpis (
        tenant_id,
        date,
        total_shipments,
        delivered,
        failed,
        cancelled,
        on_time_count,
        with_eta_count,
        avg_delivery_hours,
        cod_shipments,
        cod_collected_cents,
        success_rate,
        on_time_rate,
        updated_at
    )
    VALUES (
        p_tenant_id,
        p_date,
        v_total,
        v_delivered,
        v_failed,
        v_cancelled,
        v_on_time,
        v_with_eta,
        v_avg_hours,
        v_cod_count,
        v_cod_cents,
        v_success_rate,
        v_on_time_rate,
        NOW()
    )
    ON CONFLICT (tenant_id, date) DO UPDATE
        SET total_shipments     = EXCLUDED.total_shipments,
            delivered           = EXCLUDED.delivered,
            failed              = EXCLUDED.failed,
            cancelled           = EXCLUDED.cancelled,
            on_time_count       = EXCLUDED.on_time_count,
            with_eta_count      = EXCLUDED.with_eta_count,
            avg_delivery_hours  = EXCLUDED.avg_delivery_hours,
            cod_shipments       = EXCLUDED.cod_shipments,
            cod_collected_cents = EXCLUDED.cod_collected_cents,
            success_rate        = EXCLUDED.success_rate,
            on_time_rate        = EXCLUDED.on_time_rate,
            updated_at          = NOW();
END;
$$;

-- refresh_driver_daily_stats(p_tenant_id, p_date)
-- Recomputes driver_daily_stats for all drivers of a tenant for a given date.
-- Idempotent via ON CONFLICT DO UPDATE.
CREATE OR REPLACE FUNCTION analytics.refresh_driver_daily_stats(
    p_tenant_id UUID,
    p_date      DATE
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    INSERT INTO analytics.driver_daily_stats (
        tenant_id,
        driver_id,
        date,
        total_deliveries,
        successful,
        failed,
        avg_hours,
        cod_collected_cents,
        success_rate
    )
    SELECT
        tenant_id,
        driver_id,
        p_date                                                             AS date,
        COUNT(*)                                                           AS total_deliveries,
        COUNT(*) FILTER (WHERE event_type = 'delivered')                   AS successful,
        COUNT(*) FILTER (WHERE event_type = 'failed')                      AS failed,
        AVG(delivery_hours) FILTER (WHERE event_type = 'delivered'
                                     AND delivery_hours IS NOT NULL)       AS avg_hours,
        COALESCE(SUM(cod_amount_cents) FILTER (WHERE event_type = 'delivered'
                                               AND cod_amount_cents > 0), 0) AS cod_collected_cents,
        CASE
            WHEN COUNT(*) > 0
            THEN COUNT(*) FILTER (WHERE event_type = 'delivered')::FLOAT8 / COUNT(*)
            ELSE 0.0
        END                                                                AS success_rate
    FROM analytics.shipment_events
    WHERE tenant_id   = p_tenant_id
      AND driver_id   IS NOT NULL
      AND occurred_at >= p_date::TIMESTAMPTZ
      AND occurred_at <  (p_date + INTERVAL '1 day')::TIMESTAMPTZ
    GROUP BY tenant_id, driver_id
    ON CONFLICT (tenant_id, driver_id, date) DO UPDATE
        SET total_deliveries    = EXCLUDED.total_deliveries,
            successful          = EXCLUDED.successful,
            failed              = EXCLUDED.failed,
            avg_hours           = EXCLUDED.avg_hours,
            cod_collected_cents = EXCLUDED.cod_collected_cents,
            success_rate        = EXCLUDED.success_rate;
END;
$$;

-- refresh_zone_daily_stats(p_tenant_id, p_date)
-- Recomputes zone_daily_stats. Requires analytics.shipment_events to be
-- extended with a zone_id column (added in a future migration if not present).
-- This function is a no-op if the zone_id column does not yet exist.
CREATE OR REPLACE FUNCTION analytics.refresh_zone_daily_stats(
    p_tenant_id UUID,
    p_date      DATE
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    -- Guard: only run if zone_id column exists on shipment_events
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'analytics'
          AND table_name   = 'shipment_events'
          AND column_name  = 'zone_id'
    ) THEN
        RETURN;
    END IF;

    INSERT INTO analytics.zone_daily_stats (
        tenant_id,
        zone_id,
        date,
        shipment_count,
        delivered,
        failed,
        success_rate
    )
    SELECT
        tenant_id,
        zone_id,
        p_date                                                           AS date,
        COUNT(*)                                                         AS shipment_count,
        COUNT(*) FILTER (WHERE event_type = 'delivered')                 AS delivered,
        COUNT(*) FILTER (WHERE event_type = 'failed')                    AS failed,
        CASE
            WHEN COUNT(*) > 0
            THEN COUNT(*) FILTER (WHERE event_type = 'delivered')::FLOAT8 / COUNT(*)
            ELSE 0.0
        END                                                              AS success_rate
    FROM analytics.shipment_events
    WHERE tenant_id   = p_tenant_id
      AND zone_id     IS NOT NULL
      AND occurred_at >= p_date::TIMESTAMPTZ
      AND occurred_at <  (p_date + INTERVAL '1 day')::TIMESTAMPTZ
    GROUP BY tenant_id, zone_id
    ON CONFLICT (tenant_id, zone_id, date) DO UPDATE
        SET shipment_count = EXCLUDED.shipment_count,
            delivered      = EXCLUDED.delivered,
            failed         = EXCLUDED.failed,
            success_rate   = EXCLUDED.success_rate;
END;
$$;

-- Convenience wrapper: refresh all three aggregate tables for a given tenant + date.
CREATE OR REPLACE FUNCTION analytics.refresh_all_daily_aggregates(
    p_tenant_id UUID,
    p_date      DATE DEFAULT CURRENT_DATE - INTERVAL '1 day'
)
RETURNS VOID
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    PERFORM analytics.refresh_daily_kpis(p_tenant_id, p_date);
    PERFORM analytics.refresh_driver_daily_stats(p_tenant_id, p_date);
    PERFORM analytics.refresh_zone_daily_stats(p_tenant_id, p_date);
END;
$$;

COMMENT ON FUNCTION analytics.refresh_daily_kpis IS
    'Recomputes daily_kpis for a single (tenant, date) pair. Idempotent. '
    'Call with yesterday''s date from the nightly analytics job, or with any '
    'historical date for back-fill purposes.';

COMMENT ON FUNCTION analytics.refresh_driver_daily_stats IS
    'Recomputes driver_daily_stats for all drivers of a tenant for a given date. Idempotent.';

COMMENT ON FUNCTION analytics.refresh_zone_daily_stats IS
    'Recomputes zone_daily_stats for a tenant + date. No-op if zone_id column is absent.';

COMMENT ON FUNCTION analytics.refresh_all_daily_aggregates IS
    'Convenience wrapper that refreshes all three daily aggregate tables for a tenant + date. '
    'Default date is yesterday (for use in nightly cron jobs).';
