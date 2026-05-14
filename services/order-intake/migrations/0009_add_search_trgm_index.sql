-- Add pg_trgm GIN index for AWB + customer_name search.
--
-- CREATE INDEX CONCURRENTLY cannot run inside a transaction block.
-- If the migration runner wraps all migrations in BEGIN/COMMIT, run
-- this migration manually via psql on VPS:
--   docker exec -it logisticos-postgres psql -U logisticos -d svc_order_intake \
--     -c "CREATE EXTENSION IF NOT EXISTS pg_trgm;"
--   docker exec -it logisticos-postgres psql -U logisticos -d svc_order_intake \
--     -c "CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_shipments_search_trgm
--         ON order_intake.shipments USING GIN
--         ((awb || ' ' || customer_name) gin_trgm_ops);"

CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE INDEX IF NOT EXISTS idx_shipments_search_trgm
    ON order_intake.shipments
    USING GIN ((awb || ' ' || customer_name) gin_trgm_ops);
