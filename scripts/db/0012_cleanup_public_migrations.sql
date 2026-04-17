-- One-time cleanup for ADR-0012: schema-isolated sqlx migration tracking.
--
-- Problem: historical deploys caused `public._sqlx_migrations` to be created,
-- and sqlx's unqualified DDL resolved every service's migration tracking
-- through that table via search_path. See
-- docs/adr/0012-schema-isolated-sqlx-migrations.md.
--
-- sqlx can only hold ONE row per `version` PRIMARY KEY in `public._sqlx_migrations`,
-- so at most one service's migration state is present there. If a second service
-- tried to run migrations with the same version numbers and different checksums,
-- sqlx would have aborted. Therefore the rows in `public._sqlx_migrations` all
-- belong to a single "poisoner" service — we just don't know which one by SQL
-- alone.
--
-- Safe strategy:
--   1. Back up public._sqlx_migrations to public._sqlx_migrations_backup_0012.
--   2. Drop public._sqlx_migrations.
--   3. For each service schema, ensure <schema>._sqlx_migrations exists with
--      the correct DDL.
--
-- After running this, restart each service. What happens next:
--   • Service whose schema is EMPTY: new tracking table is empty, migrator
--     runs all migrations, service comes up clean. (Correct outcome.)
--   • Service whose schema already has tables: migrator sees empty tracking
--     table, tries to re-create existing tables, and fails with "relation
--     already exists". Operator must then MANUALLY seed that service's
--     <schema>._sqlx_migrations with the rows copied from the backup table,
--     matching the version numbers that match the service's migrations/ dir.
--     See README at the bottom of this file for the exact command.
--
-- Run once against the production database:
--   docker exec -i logisticos-postgres psql -U logisticos -d logisticos \
--     < scripts/db/0012_cleanup_public_migrations.sql

\set ON_ERROR_STOP on

BEGIN;

-- ── 1. Audit ──────────────────────────────────────────────────────────────
DO $$
DECLARE
    public_exists BOOLEAN;
    row_count INTEGER;
    ver_list TEXT;
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '_sqlx_migrations'
    ) INTO public_exists;

    IF NOT public_exists THEN
        RAISE NOTICE 'public._sqlx_migrations does not exist — nothing to clean up.';
        RETURN;
    END IF;

    EXECUTE 'SELECT COUNT(*) FROM public._sqlx_migrations' INTO row_count;
    EXECUTE 'SELECT string_agg(version::text || '':'' || description, '', '' ORDER BY version)
             FROM public._sqlx_migrations' INTO ver_list;

    RAISE NOTICE '───── public._sqlx_migrations audit ─────';
    RAISE NOTICE 'Row count: %', row_count;
    RAISE NOTICE 'Versions:  %', COALESCE(ver_list, '(none)');
    RAISE NOTICE '─────────────────────────────────────────';
END $$;

-- ── 2. Back up public._sqlx_migrations ────────────────────────────────────
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = '_sqlx_migrations'
    ) THEN
        DROP TABLE IF EXISTS public._sqlx_migrations_backup_0012;
        CREATE TABLE public._sqlx_migrations_backup_0012 AS
            SELECT * FROM public._sqlx_migrations;
        RAISE NOTICE 'Backed up to public._sqlx_migrations_backup_0012 (keep for manual reconciliation).';
    END IF;
END $$;

-- ── 3. Ensure per-service _sqlx_migrations tables exist ───────────────────
-- Keep this list in sync with services/* that call logisticos_common::migrations::run.
CREATE TEMP TABLE service_schemas (name TEXT) ON COMMIT DROP;
INSERT INTO service_schemas(name) VALUES
    ('ai_layer'),
    ('analytics'),
    ('business_logic'),
    ('carrier'),
    ('cdp'),
    ('compliance'),
    ('delivery_experience'),
    ('dispatch'),
    ('driver_ops'),
    ('fleet'),
    ('hub_ops'),
    ('identity'),
    ('marketing'),
    ('order_intake'),
    ('payments'),
    ('pod');

DO $$
DECLARE
    svc RECORD;
    svc_table_count INTEGER;
BEGIN
    FOR svc IN SELECT name FROM service_schemas LOOP
        -- Schema must already exist (created by infra bootstrap).
        IF NOT EXISTS (SELECT 1 FROM information_schema.schemata WHERE schema_name = svc.name) THEN
            RAISE NOTICE 'schema %: does not exist, skipping', svc.name;
            CONTINUE;
        END IF;

        -- Create the per-service tracking table with sqlx 0.7 DDL.
        EXECUTE format(
            'CREATE TABLE IF NOT EXISTS %I._sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
                success BOOLEAN NOT NULL,
                checksum BYTEA NOT NULL,
                execution_time BIGINT NOT NULL
            )',
            svc.name
        );

        -- Report whether the schema has application tables (i.e. whether
        -- migrations have already been applied and need manual reconciliation).
        EXECUTE format(
            'SELECT COUNT(*) FROM information_schema.tables
             WHERE table_schema = %L
               AND table_name <> ''_sqlx_migrations''
               AND table_type = ''BASE TABLE''',
            svc.name
        ) INTO svc_table_count;

        IF svc_table_count > 0 THEN
            RAISE NOTICE 'schema %: has % application tables — MANUAL RECONCILIATION REQUIRED (see script footer)', svc.name, svc_table_count;
        ELSE
            RAISE NOTICE 'schema %: empty — service will re-run migrations on next start', svc.name;
        END IF;
    END LOOP;
END $$;

-- ── 4. Drop public._sqlx_migrations ───────────────────────────────────────
DROP TABLE IF EXISTS public._sqlx_migrations;

COMMIT;

\echo ''
\echo '════════════════════════════════════════════════════════════════════════'
\echo 'Cleanup complete.'
\echo ''
\echo 'Next steps:'
\echo '  1. Restart each service. Services with empty schemas will migrate cleanly.'
\echo '  2. Services whose schemas already have application tables will fail'
\echo '     with "relation already exists" on the first migration. For each such'
\echo '     service, seed its per-schema tracking table from the backup:'
\echo ''
\echo '     INSERT INTO <schema>._sqlx_migrations'
\echo '       SELECT * FROM public._sqlx_migrations_backup_0012'
\echo '       WHERE version IN (<versions that belong to this service>);'
\echo ''
\echo '     Check services/<svc>/migrations/ to identify which version numbers'
\echo '     belong to that service. Verify checksums match by restarting; sqlx'
\echo '     will error on mismatch rather than silently diverging.'
\echo ''
\echo '  3. Once all services are healthy, drop the backup:'
\echo '     DROP TABLE public._sqlx_migrations_backup_0012;'
\echo '════════════════════════════════════════════════════════════════════════'
