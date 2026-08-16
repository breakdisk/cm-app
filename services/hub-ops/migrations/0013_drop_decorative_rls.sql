-- Drop decorative Row-Level Security. See ADR-0016.
--
-- These policies never enforced anything: no service sets
-- current_setting('app.tenant_id'), and services connect as the schema
-- owner, which PostgreSQL exempts from RLS unless FORCE is set. Where FORCE
-- was tried it broke reads and was reverted (order-intake/0007,
-- driver-ops/0005).
--
-- Tenant isolation is application-layer: every repository method takes
-- tenant_id and every statement filters on it. Removing these stops the
-- schema asserting a database guarantee that does not exist.
--
-- Observably a no-op at runtime, which is why it is safe in one pass.

DROP POLICY IF EXISTS hub_tenant ON hub_ops.hubs;
DROP POLICY IF EXISTS induction_tenant ON hub_ops.parcel_inductions;
DROP POLICY IF EXISTS tenant_rls ON hub_ops.dock_slots;
DROP POLICY IF EXISTS tenant_rls ON hub_ops.sort_scans;
DROP POLICY IF EXISTS pallet_tenant ON hub_ops.pallets;
DROP POLICY IF EXISTS pallet_piece_tenant ON hub_ops.pallet_pieces;
DROP POLICY IF EXISTS container_tenant ON hub_ops.containers;
DROP POLICY IF EXISTS container_pallet_tenant ON hub_ops.container_pallets;
DROP POLICY IF EXISTS container_loose_tenant ON hub_ops.container_loose_pieces;
DROP POLICY IF EXISTS truck_spec_tenant ON hub_ops.truck_specs;
DROP POLICY IF EXISTS consolidation_plan_tenant ON hub_ops.consolidation_plans;
DROP POLICY IF EXISTS hub_scan_tenant ON hub_ops.hub_scans;
DROP POLICY IF EXISTS hub_location_tenant ON hub_ops.hub_locations;
DROP POLICY IF EXISTS hub_inventory_tenant ON hub_ops.hub_inventory;
DROP POLICY IF EXISTS manifest_tenant ON hub_ops.hub_transfer_manifests;
DROP POLICY IF EXISTS routing_config_tenant ON hub_ops.hub_routing_configs;

ALTER TABLE IF EXISTS hub_ops.hubs DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.parcel_inductions DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.dock_slots DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.sort_scans DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.pallets DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.pallet_pieces DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.containers DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.container_pallets DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.container_loose_pieces DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.truck_specs DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.consolidation_plans DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.hub_scans DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.hub_locations DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.hub_inventory DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.hub_transfer_manifests DISABLE ROW LEVEL SECURITY;
ALTER TABLE IF EXISTS hub_ops.hub_routing_configs DISABLE ROW LEVEL SECURITY;
