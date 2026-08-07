#!/bin/bash
set -e

echo "=== Step 1: Set search_path defaults on all databases ==="
docker exec -i logisticos-postgres psql -U logisticos << 'SQL'
ALTER DATABASE svc_identity SET search_path TO identity, public;
ALTER DATABASE svc_cdp SET search_path TO cdp, public;
ALTER DATABASE svc_engagement SET search_path TO engagement, public;
ALTER DATABASE svc_carrier SET search_path TO carrier, public;
ALTER DATABASE svc_business_logic SET search_path TO business_logic, public;
ALTER DATABASE svc_fleet SET search_path TO fleet, public;
ALTER DATABASE svc_marketing SET search_path TO marketing, public;
ALTER DATABASE svc_payments SET search_path TO payments, public;
ALTER DATABASE svc_hub_ops SET search_path TO hub_ops, public;
ALTER DATABASE svc_pod SET search_path TO pod, public;
ALTER DATABASE svc_compliance SET search_path TO compliance, public;
ALTER DATABASE svc_delivery_experience SET search_path TO delivery_experience, public;
ALTER DATABASE svc_ai_layer SET search_path TO ai, public;
ALTER DATABASE svc_analytics SET search_path TO analytics, public;
ALTER DATABASE svc_dispatch SET search_path TO dispatch, public;
ALTER DATABASE svc_driver_ops SET search_path TO driver_ops, public;
ALTER DATABASE svc_order_intake SET search_path TO order_intake, public;
SQL
echo "Done: search_path defaults set"

echo ""
echo "=== Step 2: Drop and recreate svc_payments ==="
docker stop logisticos-payments 2>/dev/null || true
docker exec -i logisticos-postgres psql -U logisticos -c "DROP DATABASE IF EXISTS svc_payments WITH (FORCE);"
docker exec -i logisticos-postgres psql -U logisticos -c "CREATE DATABASE svc_payments;"
docker exec -i logisticos-postgres psql -U logisticos -c "ALTER DATABASE svc_payments SET search_path TO payments, public;"
docker exec -i logisticos-postgres psql -U logisticos -d svc_payments -c "
  CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";
  CREATE EXTENSION IF NOT EXISTS pgcrypto;
  CREATE EXTENSION IF NOT EXISTS pg_trgm;
  CREATE SCHEMA IF NOT EXISTS payments;
"
echo "Done: svc_payments recreated with extensions and schema"

echo ""
echo "=== Step 3: Apply payments migrations manually (with DEFERRABLE fix) ==="
# Run the migration SQL from the repo, piped through docker exec
# Migration files are in the Dokploy checkout
REPO_DIR="/etc/dokploy/compose/oscargomarketnet-logisticosbackend-rldhbg/code"

# Apply migration 1
docker exec -i logisticos-postgres psql -U logisticos -d svc_payments < "${REPO_DIR}/services/payments/migrations/0001_create_payments_tables.sql"
echo "Applied migration 1"

# Apply migration 2 with DEFERRABLE line removed
cat "${REPO_DIR}/services/payments/migrations/0002_create_cod_tables.sql" | sed 's/    DEFERRABLE INITIALLY DEFERRED;/;/' | docker exec -i logisticos-postgres psql -U logisticos -d svc_payments
echo "Applied migration 2 (DEFERRABLE removed)"

# Apply migration 3
docker exec -i logisticos-postgres psql -U logisticos -d svc_payments < "${REPO_DIR}/services/payments/migrations/0003_restructure_invoices.sql"
echo "Applied migration 3"

echo ""
echo "=== Step 4: Insert fake _sqlx_migrations records for payments ==="
docker exec -i logisticos-postgres psql -U logisticos -d svc_payments << 'SQL'
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    success BOOLEAN NOT NULL DEFAULT true,
    checksum BYTEA NOT NULL,
    execution_time BIGINT NOT NULL DEFAULT 0
);

INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time) VALUES
(1, 'create payments tables', NOW(), true, E'\\x40a901dec8dd3f7bf085df7325950a88ca484478b3c0e596178584ece3f80c092162c9e24e212815361d25be8f8a3750', 0),
(2, 'create cod tables', NOW(), true, E'\\x2e5e6789f72ec7c9234e3896b11fd84d5e0f18c1103fdc34065102d7138974f7cac9cf99a9cd0b8459f718edefa8610f', 0),
(3, 'restructure invoices', NOW(), true, E'\\x5b8d71f3b62803dd314ea81996125b04f4f3a0af721784858deea4b41253f34c3cf36d0b34c125a7e29ed280ee2e2106', 0);
SQL
echo "Done: _sqlx_migrations records inserted for payments"

echo ""
echo "=== Step 5: Drop and recreate svc_business_logic ==="
docker stop logisticos-business-logic 2>/dev/null || true
docker exec -i logisticos-postgres psql -U logisticos -c "DROP DATABASE IF EXISTS svc_business_logic WITH (FORCE);"
docker exec -i logisticos-postgres psql -U logisticos -c "CREATE DATABASE svc_business_logic;"
docker exec -i logisticos-postgres psql -U logisticos -c "ALTER DATABASE svc_business_logic SET search_path TO business_logic, public;"
docker exec -i logisticos-postgres psql -U logisticos -d svc_business_logic -c "
  CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";
  CREATE EXTENSION IF NOT EXISTS pgcrypto;
  CREATE EXTENSION IF NOT EXISTS pg_trgm;
  CREATE SCHEMA IF NOT EXISTS business_logic;
"
echo "Done: svc_business_logic recreated"

echo ""
echo "=== Step 6: Recreate POD container with TWILIO env vars ==="
docker stop logisticos-pod 2>/dev/null || true
docker rm logisticos-pod 2>/dev/null || true

# Get the network name
NETWORK="oscargomarketnet-logisticosbackend-rldhbg_logisticos"

docker run -d \
  --name logisticos-pod \
  --restart unless-stopped \
  --network "$NETWORK" \
  -p 8011:8011 \
  -e APP__HOST=0.0.0.0 \
  -e APP__PORT=8011 \
  -e APP__ENV=development \
  -e "DATABASE__URL=postgres://logisticos:password@postgres:5432/svc_pod" \
  -e DATABASE__MAX_CONNECTIONS=5 \
  -e KAFKA__BROKERS=kafka:29092 \
  -e KAFKA__GROUP_ID=pod-dev \
  -e "AUTH__JWT_SECRET=dev-jwt-secret-REPLACE-WITH-32CHAR-RANDOM-VALUE-123" \
  -e AUTH__JWT_EXPIRY_SECONDS=3600 \
  -e AUTH__REFRESH_TOKEN_EXPIRY_SECONDS=86400 \
  -e REDIS__URL=redis://redis:6379 \
  -e SERVICES__IDENTITY_URL=http://identity:8001 \
  -e SERVICES__CDP_URL=http://cdp:8002 \
  -e SERVICES__ENGAGEMENT_URL=http://engagement:8003 \
  -e SERVICES__ORDER_INTAKE_URL=http://order-intake:8004 \
  -e SERVICES__DISPATCH_URL=http://dispatch:8005 \
  -e SERVICES__DRIVER_OPS_URL=http://driver-ops:8006 \
  -e SERVICES__DELIVERY_EXPERIENCE_URL=http://delivery-experience:8007 \
  -e SERVICES__FLEET_URL=http://fleet:8008 \
  -e SERVICES__HUB_OPS_URL=http://hub-ops:8009 \
  -e SERVICES__CARRIER_URL=http://carrier:8010 \
  -e SERVICES__POD_URL=http://pod:8011 \
  -e SERVICES__PAYMENTS_URL=http://payments:8012 \
  -e SERVICES__ANALYTICS_URL=http://analytics:8013 \
  -e SERVICES__MARKETING_URL=http://marketing:8014 \
  -e SERVICES__BUSINESS_LOGIC_URL=http://business-logic:8015 \
  -e SERVICES__AI_LAYER_URL=http://ai-layer:8016 \
  -e S3_ENDPOINT=http://minio:9001 \
  -e S3_BUCKET=pod-evidence \
  -e S3_ACCESS_KEY=minioadmin \
  -e S3_SECRET_KEY=minioadmin \
  -e TWILIO_ACCOUNT_SID=dev-placeholder \
  -e TWILIO_AUTH_TOKEN=dev-placeholder \
  -e TWILIO_FROM_NUMBER=+15005550006 \
  -e RUST_LOG=info \
  ghcr.io/breakdisk/logisticos-service-pod:latest
echo "Done: POD container recreated with TWILIO env vars"

echo ""
echo "=== Step 7: Start payments and business-logic ==="
docker start logisticos-payments logisticos-business-logic

echo ""
echo "=== Waiting 30 seconds for services to stabilize ==="
sleep 30

echo ""
echo "=== Final status ==="
docker ps --format "table {{.Names}}\t{{.Status}}" | grep logisticos | grep -v "Up "
echo ""
echo "If empty above, all services are UP."
