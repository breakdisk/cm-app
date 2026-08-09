#!/usr/bin/env bash
# Seeds the hero flow: "Dinner for two from Kuya's, and we're out of milk and eggs."
#
# Idempotent — safe to re-run. Uses fixed UUIDs so a developer can reference the
# same vendor across restarts, and so a failed run can simply be repeated.
set -euo pipefail

DB="${DB:-svc_omnideliv}"
FIELD_OPS_DB="${FIELD_OPS_DB:-svc_field_ops}"
PSQL="docker exec -i logisticos-postgres psql -U logisticos -v ON_ERROR_STOP=1"

TENANT="00000000-0000-0000-0000-000000000001"   # the existing dev tenant
KUYAS="11111111-0000-0000-0000-000000000001"
PUREGOLD="11111111-0000-0000-0000-000000000002"
COURIER="33333333-0000-0000-0000-000000000001"
# The dev driver login, so the courier half of the flow is reachable. A
# gen_random_uuid() here produces a courier nobody can authenticate as, which
# leaves the claim -> Kafka -> order-advances leg permanently untestable — the
# seed looks complete and the only part it cannot exercise is the event pipeline.
DRIVER_USER="00000000-0000-0000-0000-000000000004"   # driver@demo.com
# The merchant portal login, so `GET /v1/omnideliv/vendors/me/earnings`
# has a store to resolve. Without it that endpoint 404s and the vendor
# payout cannot be checked over HTTP — only in psql.
VENDOR_USER="00000000-0000-0000-0000-000000000003"   # merchant@demo.com

echo "Seeding vendors…"
$PSQL -d "$DB" <<SQL
INSERT INTO omnideliv.vendors (id, tenant_id, vertical, name, address, lat, lng, prep_time_minutes, commission_bps, status)
VALUES
  ('$KUYAS',    '$TENANT', 'restaurant', 'Kuya''s Silog House', '12 Mabini St, Manila', 14.5995, 120.9842, 20, 1500, 'active'),
  ('$PUREGOLD', '$TENANT', 'grocery',    'Puregold Ermita',     '8 Padre Faura, Manila', 14.5820, 120.9830,  5, 1200, 'active')
ON CONFLICT (id) DO UPDATE SET status = 'active';

-- Link the restaurant to a real portal login so its earnings are readable.
UPDATE omnideliv.vendors SET user_id = '$VENDOR_USER' WHERE id = '$KUYAS';
SQL

echo "Seeding catalog…"
$PSQL -d "$DB" <<SQL
INSERT INTO omnideliv.catalog_items (id, tenant_id, vendor_id, sku, name, price_cents, allergens, dietary_tags)
VALUES
  ('22222222-0000-0000-0000-000000000001', '$TENANT', '$KUYAS',    'tapsilog',  'Tapsilog',        17000, '{}',      '{}'),
  ('22222222-0000-0000-0000-000000000002', '$TENANT', '$KUYAS',    'bangsilog', 'Bangsilog',       16000, '{fish}',  '{}'),
  ('22222222-0000-0000-0000-000000000003', '$TENANT', '$PUREGOLD', 'milk-1l',   'Fresh Milk 1L',    8500, '{dairy}', '{}'),
  ('22222222-0000-0000-0000-000000000004', '$TENANT', '$PUREGOLD', 'eggs-12',   'Eggs (dozen)',    12000, '{eggs}',  '{}'),
  ('22222222-0000-0000-0000-000000000005', '$TENANT', '$PUREGOLD', 'eggs-12-b', 'Eggs, Farm Fresh',10800, '{eggs}',  '{}')
ON CONFLICT (id) DO NOTHING;

-- Availability is inserted by save_item in production. Seeded here explicitly
-- so the freshness clock starts now rather than at whatever the default was.
-- Declare the allergen contents, as a real vendor would.
--
-- Without this every seeded item is *undeclared*, and reconcile correctly
-- refuses undeclared items to any customer who states an allergy (migration
-- 0014) — so the hero flow "no peanuts" would return an empty basket and look
-- broken when it is in fact working. Declaring here is what a vendor does in
-- the storefront console.
UPDATE omnideliv.catalog_items
   SET allergens_declared_at = NOW()
 WHERE tenant_id = '$TENANT';

-- confirmed_at as well as updated_at, and not by accident: since migration 0015
-- the freshness model reads only confirmed_at, and NULL there means "no human
-- has ever attested to this". Seeding without it would leave every demo item
-- permanently uncertain — the mesh would substitute the whole catalog and the
-- hero flow would look broken while behaving exactly as designed.
--
-- The seed stands in for the vendor tapping "confirm" in the storefront console,
-- so it writes the human clock. An *ingest* never may; that is the distinction
-- the two columns exist to hold.
INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at, confirmed_at)
SELECT id, tenant_id, 'available', NOW(), NOW()
  FROM omnideliv.catalog_items WHERE tenant_id = '$TENANT'
ON CONFLICT (item_id) DO UPDATE SET
    state = 'available', updated_at = NOW(), confirmed_at = NOW();

-- One item out of stock, so the substitution path has something to do. Without
-- this the hero flow never exercises Screen C, which is half the design.
UPDATE omnideliv.item_availability
   SET state = 'out_of_stock', updated_at = NOW()
 WHERE item_id = '22222222-0000-0000-0000-000000000004';
SQL

echo "Seeding a courier…"
$PSQL -d "$FIELD_OPS_DB" <<SQL
INSERT INTO field_ops.couriers (id, tenant_id, user_id, first_name, last_name, phone, status, last_lat, last_lng, last_seen_at)
VALUES ('$COURIER', '$TENANT', '$DRIVER_USER',
        'Rico', 'M', '+639170000001', 'available', 14.5900, 120.9800, NOW())
ON CONFLICT (id) DO UPDATE SET status = 'available', last_seen_at = NOW(), user_id = '$DRIVER_USER';

-- THE ROW THAT MAKES THE COURIER FINDABLE.
--
-- couriers.last_lat/last_lng are a render cache, not the search index: per
-- ADR-0015, proximity search is ST_DWithin against courier_latest_locations,
-- which is a view over courier_locations. A courier seeded with only the cached
-- columns has no row there, so find_available_near returns nothing and checkout
-- fails with "no courier available" — a seed that looks complete and silently
-- breaks the hero flow.
INSERT INTO field_ops.courier_locations
    (id, tenant_id, courier_id, lat, lng, device_timestamp, recorded_at)
VALUES (gen_random_uuid(), '$TENANT', '$COURIER', 14.5900, 120.9800, NOW(), NOW());

-- Trim older seeded fixes so re-running does not accumulate a fake breadcrumb
-- trail. Only the newest matters — the view is DISTINCT ON (courier_id).
DELETE FROM field_ops.courier_locations
 WHERE courier_id = '$COURIER'
   AND recorded_at < (SELECT MAX(recorded_at) FROM field_ops.courier_locations WHERE courier_id = '$COURIER');
SQL

echo
echo "Seeded. Try:"
echo "  Kuya's Silog House : $KUYAS  (owned by merchant@demo.com, so /vendors/me works)"
echo "  Puregold Ermita    : $PUREGOLD"
echo "  Courier Rico       : $COURIER  (fresh GPS fix, so proximity search finds him)"
echo "  Eggs (dozen) is OUT OF STOCK — the substitution path has something to propose."
echo
echo "NOTE: the GPS fix ages. find_available_near only considers fixes from the"
echo "      last 10 minutes, so re-run this before testing dispatch."
