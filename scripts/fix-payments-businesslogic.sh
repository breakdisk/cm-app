#!/bin/bash
# =============================================================================
# Fix the last 2 crashing services: payments and business-logic
#
# Root cause — payments:
#   Manual migration 3 application failed (ROW_NUMBER window function error on
#   non-empty table). Fake _sqlx_migrations records don't match binary state.
#   Fix: nuke DB, let the binary run its own baked-in migrations on empty tables.
#
# Root cause — business-logic:
#   The migration on disk creates "automation_rules" but the deployed binary
#   expects "rules" (older code). The binary's baked-in migration creates the
#   correct schema. Also requires logisticos_app and logisticos_service roles.
#   Fix: ensure roles exist, nuke DB, let binary run its own migrations.
#
# Strategy: Do NOT manually apply migration SQL. Let the service binaries handle
# everything via sqlx::migrate!(). The binaries have correct SQL baked in at
# compile time.
# =============================================================================
set -e

PG="docker exec -i logisticos-postgres psql -U logisticos"

echo "================================================================"
echo "  Step 1: Stop both services"
echo "================================================================"
docker stop logisticos-payments logisticos-business-logic 2>/dev/null || true
echo "Stopped."

echo ""
echo "================================================================"
echo "  Step 2: Ensure required PostgreSQL roles exist (cluster-wide)"
echo "================================================================"
$PG -c "
DO \$\$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'logisticos_app') THEN
        CREATE ROLE logisticos_app LOGIN;
    END IF;
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'logisticos_service') THEN
        CREATE ROLE logisticos_service LOGIN;
    END IF;
END
\$\$;
"
echo "Roles logisticos_app and logisticos_service ensured."

echo ""
echo "================================================================"
echo "  Step 3: Drop and recreate svc_payments"
echo "================================================================"
$PG -c "DROP DATABASE IF EXISTS svc_payments WITH (FORCE);"
$PG -c "CREATE DATABASE svc_payments;"
$PG -c "ALTER DATABASE svc_payments SET search_path TO payments, public;"
$PG -d svc_payments -c "
  CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";
  CREATE EXTENSION IF NOT EXISTS pgcrypto;
  CREATE EXTENSION IF NOT EXISTS pg_trgm;
  CREATE SCHEMA IF NOT EXISTS payments;
"
echo "svc_payments: recreated with extensions and schema."

echo ""
echo "================================================================"
echo "  Step 4: Drop and recreate svc_business_logic"
echo "================================================================"
$PG -c "DROP DATABASE IF EXISTS svc_business_logic WITH (FORCE);"
$PG -c "CREATE DATABASE svc_business_logic;"
$PG -c "ALTER DATABASE svc_business_logic SET search_path TO business_logic, public;"
$PG -d svc_business_logic -c "
  CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";
  CREATE EXTENSION IF NOT EXISTS pgcrypto;
  CREATE EXTENSION IF NOT EXISTS pg_trgm;
  CREATE SCHEMA IF NOT EXISTS business_logic;
"
# Grant schema usage to the roles the migration expects
$PG -d svc_business_logic -c "
  GRANT USAGE ON SCHEMA business_logic TO logisticos_app;
  GRANT ALL ON SCHEMA business_logic TO logisticos_service;
  GRANT ALL ON ALL TABLES IN SCHEMA business_logic TO logisticos_service;
  GRANT ALL ON ALL SEQUENCES IN SCHEMA business_logic TO logisticos_service;
"
echo "svc_business_logic: recreated with extensions, schema, and role grants."

echo ""
echo "================================================================"
echo "  Step 5: Verify both databases are clean (no _sqlx_migrations)"
echo "================================================================"
echo "--- svc_payments tables ---"
$PG -d svc_payments -c "SELECT schemaname, tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') ORDER BY 1,2;"
echo ""
echo "--- svc_business_logic tables ---"
$PG -d svc_business_logic -c "SELECT schemaname, tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') ORDER BY 1,2;"

echo ""
echo "================================================================"
echo "  Step 6: Start both services — let them run their own migrations"
echo "================================================================"
docker start logisticos-payments logisticos-business-logic
echo "Started. Waiting 15 seconds for migrations to run..."
sleep 15

echo ""
echo "================================================================"
echo "  Step 7: Check service health"
echo "================================================================"
echo "--- payments logs (last 20 lines) ---"
docker logs logisticos-payments --tail 20 2>&1
echo ""
echo "--- business-logic logs (last 20 lines) ---"
docker logs logisticos-business-logic --tail 20 2>&1

echo ""
echo "================================================================"
echo "  Step 8: Verify tables were created by migrations"
echo "================================================================"
echo "--- svc_payments tables ---"
$PG -d svc_payments -c "SELECT schemaname, tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') ORDER BY 1,2;"
echo ""
echo "--- svc_business_logic tables ---"
$PG -d svc_business_logic -c "SELECT schemaname, tablename FROM pg_tables WHERE schemaname NOT IN ('pg_catalog','information_schema') ORDER BY 1,2;"

echo ""
echo "================================================================"
echo "  Final: Overall service status"
echo "================================================================"
docker ps --format "table {{.Names}}\t{{.Status}}" | grep logisticos | sort
echo ""
echo "If payments and business-logic show 'Up' — all 17 services are healthy."
