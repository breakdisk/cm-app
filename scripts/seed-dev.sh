#!/usr/bin/env bash
# ============================================================
# LogisticOS — Development Data Seeder
# ============================================================
# Seeds a predictable, idempotent dataset for local development.
# Safe to re-run multiple times — uses INSERT ... ON CONFLICT.
#
# Seeds:
#   • 2 test tenants
#   • 10 merchants per tenant (20 total)
#   • 5 drivers (AVAILABLE status)
#   • 3 hubs (Manila, Cebu, Davao)
#   • 20 sample shipments (various statuses)
#   • 3 carriers (LBC, J&T Express, Ninja Van)
#
# Usage:
#   ./scripts/seed-dev.sh
#   DATABASE_URL=postgres://... ./scripts/seed-dev.sh
# ============================================================

set -euo pipefail
IFS=$'\n\t'

# ── Colors ──────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
RESET='\033[0m'

info()    { echo -e "${CYAN}[SEED]${RESET}   $*"; }
success() { echo -e "${GREEN}[OK]${RESET}     $*"; }
warn()    { echo -e "${YELLOW}[WARN]${RESET}   $*"; }
error()   { echo -e "${RED}[ERROR]${RESET}  $*" >&2; }
section() { echo -e "\n${BOLD}${CYAN}▶ $*${RESET}"; }

# ── Environment ───────────────────────────────────────────────
DATABASE_URL="${DATABASE_URL:-postgres://logisticos:password@localhost:5432/logisticos}"
export DATABASE_URL

# ── Check DB connectivity ─────────────────────────────────────
if ! psql "$DATABASE_URL" -c "SELECT 1" &>/dev/null 2>&1; then
  error "Cannot connect to database: $DATABASE_URL"
  error "Ensure PostgreSQL is running: docker compose up postgres"
  exit 1
fi

# ── Seed tracking ─────────────────────────────────────────────
INSERTED=0
SKIPPED=0

psql_exec() {
  psql "$DATABASE_URL" --no-psqlrc -v ON_ERROR_STOP=1 -q "$@"
}

psql_count() {
  psql "$DATABASE_URL" --no-psqlrc -t -A -c "$1"
}

echo -e "${BOLD}${CYAN}╔══════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}${CYAN}║  LogisticOS Development Data Seeder      ║${RESET}"
echo -e "${BOLD}${CYAN}╚══════════════════════════════════════════╝${RESET}"
echo ""
info "Database: ${DATABASE_URL%%@*}@…"
info "Date:     $(date)"
echo ""

# ============================================================
# TENANTS
# ============================================================
section "Seeding Tenants"

psql_exec <<'SQL'
-- Ensure tenants table has the test tenants
INSERT INTO tenants (
  id,
  name,
  slug,
  plan,
  status,
  region,
  contact_email,
  contact_phone,
  address,
  settings,
  created_at,
  updated_at
) VALUES
(
  'tenant_test_001',
  'FastShip Philippines',
  'fastship-ph',
  'enterprise',
  'active',
  'ap-southeast-1',
  'ops@fastship.ph',
  '+639171234567',
  '123 Bonifacio Global City, Taguig, Metro Manila, PH',
  '{"default_currency": "PHP", "timezone": "Asia/Manila", "sla_standard_hours": 48, "cod_enabled": true, "enable_ai_dispatch": true}',
  NOW() - INTERVAL '90 days',
  NOW()
),
(
  'tenant_test_002',
  'QuickDeliver Cebu',
  'quickdeliver-cebu',
  'growth',
  'active',
  'ap-southeast-1',
  'ops@quickdeliver.cebu',
  '+639281234567',
  '456 Cebu IT Park, Lahug, Cebu City, PH',
  '{"default_currency": "PHP", "timezone": "Asia/Manila", "sla_standard_hours": 72, "cod_enabled": true, "enable_ai_dispatch": false}',
  NOW() - INTERVAL '45 days',
  NOW()
)
ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM tenants WHERE id IN ('tenant_test_001','tenant_test_002')")
success "Tenants: $COUNT records (target: 2)"

# ============================================================
# MERCHANTS
# ============================================================
section "Seeding Merchants (10 per tenant)"

psql_exec <<'SQL'
-- Tenant 001 merchants
INSERT INTO merchants (
  id, tenant_id, name, email, phone,
  address, city, province, postal_code, country,
  business_type, monthly_volume_estimate, status,
  api_key, webhook_url,
  created_at, updated_at
) VALUES
('merchant_001_01','tenant_test_001','Lazada Store PH','seller1@lazada.ph','+639171000001','Unit 1, Makati Ave, Makati','Makati','Metro Manila','1200','PH','ecommerce',500,'active','mk_live_001_01_aaaaaaaaaaaa','https://hooks.lazada.ph/logisticos',NOW()-INTERVAL '80 days',NOW()),
('merchant_001_02','tenant_test_001','Shopee Flash Deals','flash@shopee.ph','+639171000002','2F Robinsons Galleria, Ortigas','Pasig','Metro Manila','1600','PH','ecommerce',1200,'active','mk_live_001_02_bbbbbbbbbbbb','https://api.shopee.ph/webhooks/logisticos',NOW()-INTERVAL '75 days',NOW()),
('merchant_001_03','tenant_test_001','Balikbayan Padala Hub','info@bbpadala.ph','+639171000003','123 España Blvd, Sampaloc','Manila','Metro Manila','1008','PH','freight',200,'active','mk_live_001_03_cccccccccccc',NULL,NOW()-INTERVAL '70 days',NOW()),
('merchant_001_04','tenant_test_001','GroceryDash PH','orders@grocerydash.ph','+639171000004','456 Quezon Ave, Quezon City','Quezon City','Metro Manila','1103','PH','grocery',800,'active','mk_live_001_04_dddddddddddd','https://grocerydash.ph/webhooks/delivery',NOW()-INTERVAL '65 days',NOW()),
('merchant_001_05','tenant_test_001','MedExpress Pharmacy','meds@medexpress.ph','+639171000005','789 UN Ave, Paco','Manila','Metro Manila','1007','PH','pharmacy',300,'active','mk_live_001_05_eeeeeeeeeeee',NULL,NOW()-INTERVAL '60 days',NOW()),
('merchant_001_06','tenant_test_001','TechGadgets Online','sales@techgadgets.ph','+639171000006','SM Megamall B2, Mandaluyong','Mandaluyong','Metro Manila','1550','PH','electronics',450,'active','mk_live_001_06_ffffffffffff','https://techgadgets.ph/delivery-hook',NOW()-INTERVAL '55 days',NOW()),
('merchant_001_07','tenant_test_001','FashionForward PH','orders@fashionforward.ph','+639171000007','Greenbelt 5, Makati','Makati','Metro Manila','1200','PH','fashion',600,'active','mk_live_001_07_gggggggggggg',NULL,NOW()-INTERVAL '50 days',NOW()),
('merchant_001_08','tenant_test_001','HomeDecor Direct','hello@homedecordirect.ph','+639171000008','1010 EDSA, Quezon City','Quezon City','Metro Manila','1103','PH','home_goods',250,'active','mk_live_001_08_hhhhhhhhhhhh',NULL,NOW()-INTERVAL '45 days',NOW()),
('merchant_001_09','tenant_test_001','OfficeSupply Central','bulk@officesupply.ph','+639171000009','Fort Bonifacio, Taguig','Taguig','Metro Manila','1634','PH','b2b',150,'active','mk_live_001_09_iiiiiiiiiiii',NULL,NOW()-INTERVAL '40 days',NOW()),
('merchant_001_10','tenant_test_001','AutoParts Express PH','orders@autopartsph.com','+639171000010','PEZA Laguna Technopark','Binan','Laguna','4024','PH','automotive',90,'active','mk_live_001_10_jjjjjjjjjjjj',NULL,NOW()-INTERVAL '35 days',NOW()),
-- Tenant 002 merchants
('merchant_002_01','tenant_test_002','Sugbo Ukay-Ukay','ukay@sugbo.com','+639281000001','Carbon Market, Cebu City','Cebu City','Cebu','6000','PH','fashion',300,'active','mk_live_002_01_kkkkkkkkkkkk',NULL,NOW()-INTERVAL '40 days',NOW()),
('merchant_002_02','tenant_test_002','Cebu Lechon Express','orders@cebulachon.com','+639281000002','Mactan Island, Lapu-Lapu','Lapu-Lapu','Cebu','6015','PH','food',400,'active','mk_live_002_02_llllllllllll','https://cebulachon.com/hooks/delivery',NOW()-INTERVAL '38 days',NOW()),
('merchant_002_03','tenant_test_002','Visayas Fresh Produce','fresh@visayasfarm.com','+639281000003','Talisay City, Cebu','Talisay','Cebu','6045','PH','grocery',600,'active','mk_live_002_03_mmmmmmmmmmmm',NULL,NOW()-INTERVAL '35 days',NOW()),
('merchant_002_04','tenant_test_002','IT Park Electronics','gadgets@itparkcebu.com','+639281000004','Cebu IT Park, Lahug','Cebu City','Cebu','6000','PH','electronics',200,'active','mk_live_002_04_nnnnnnnnnnnn',NULL,NOW()-INTERVAL '32 days',NOW()),
('merchant_002_05','tenant_test_002','Cebu Handicrafts','crafts@cebucrafts.com','+639281000005','Colon Street, Cebu City','Cebu City','Cebu','6000','PH','crafts',150,'active','mk_live_002_05_oooooooooooo',NULL,NOW()-INTERVAL '30 days',NOW()),
('merchant_002_06','tenant_test_002','Bohol Honey Farm','honey@bholhoney.com','+639281000006','Tagbilaran City, Bohol','Tagbilaran','Bohol','6300','PH','food',80,'active','mk_live_002_06_pppppppppppp',NULL,NOW()-INTERVAL '28 days',NOW()),
('merchant_002_07','tenant_test_002','Iloilo Style House','style@ilostyle.com','+639281000007','SM City Iloilo, Mandurriao','Iloilo City','Iloilo','5000','PH','fashion',220,'active','mk_live_002_07_qqqqqqqqqqqq',NULL,NOW()-INTERVAL '25 days',NOW()),
('merchant_002_08','tenant_test_002','Dumaguete Books','books@dumbooks.com','+639281000008','Silliman Ave, Dumaguete','Dumaguete','Negros Oriental','6200','PH','books',60,'active','mk_live_002_08_rrrrrrrrrrrr',NULL,NOW()-INTERVAL '22 days',NOW()),
('merchant_002_09','tenant_test_002','Bacolod Sweet Delights','sweets@bacsweet.com','+639281000009','Lacson Street, Bacolod','Bacolod City','Negros Occidental','6100','PH','food',180,'active','mk_live_002_09_ssssssssssss',NULL,NOW()-INTERVAL '20 days',NOW()),
('merchant_002_10','tenant_test_002','Leyte Seafoods Direct','seafood@leytesea.com','+639281000010','Real Street, Tacloban','Tacloban','Leyte','6500','PH','food',120,'active','mk_live_002_10_tttttttttttt',NULL,NOW()-INTERVAL '18 days',NOW())
ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM merchants WHERE id LIKE 'merchant_%'")
success "Merchants: $COUNT records (target: 20)"

# ============================================================
# HUBS
# ============================================================
section "Seeding Hubs"

psql_exec <<'SQL'
INSERT INTO hubs (
  id, tenant_id, name, code,
  address, city, province, postal_code, country,
  latitude, longitude,
  capacity_parcels_per_day, current_load,
  status, hub_type,
  contact_name, contact_phone, contact_email,
  operating_hours_start, operating_hours_end,
  created_at, updated_at
) VALUES
(
  'hub_manila_001',
  'tenant_test_001',
  'Manila Central Hub',
  'MNL-HUB-001',
  '100 Logistics Drive, Valenzuela City, Metro Manila',
  'Valenzuela',
  'Metro Manila',
  '1440',
  'PH',
  14.7000,
  120.9700,
  5000,
  0,
  'active',
  'main_hub',
  'Roberto Santos',
  '+639171999001',
  'hub.manila@fastship.ph',
  '06:00:00',
  '22:00:00',
  NOW() - INTERVAL '90 days',
  NOW()
),
(
  'hub_cebu_001',
  'tenant_test_001',
  'Cebu Hub',
  'CEB-HUB-001',
  '50 Mactan Economic Zone, Lapu-Lapu City',
  'Lapu-Lapu',
  'Cebu',
  '6015',
  'PH',
  10.3333,
  123.9333,
  2000,
  0,
  'active',
  'regional_hub',
  'Maria Santos',
  '+639171999002',
  'hub.cebu@fastship.ph',
  '07:00:00',
  '21:00:00',
  NOW() - INTERVAL '85 days',
  NOW()
),
(
  'hub_davao_001',
  'tenant_test_001',
  'Davao Hub',
  'DAV-HUB-001',
  '25 Km. 7, Bangkal, Davao City',
  'Davao City',
  'Davao del Sur',
  '8000',
  'PH',
  7.0730,
  125.6120,
  1500,
  0,
  'active',
  'regional_hub',
  'Jose Reyes',
  '+639171999003',
  'hub.davao@fastship.ph',
  '07:00:00',
  '21:00:00',
  NOW() - INTERVAL '80 days',
  NOW()
)
ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM hubs WHERE id LIKE 'hub_%'")
success "Hubs: $COUNT records (target: 3)"

# ============================================================
# DRIVERS
# ============================================================
section "Seeding Drivers"

psql_exec <<'SQL'
INSERT INTO drivers (
  id, tenant_id,
  first_name, last_name,
  email, phone,
  license_number, license_expiry,
  vehicle_type, vehicle_plate, vehicle_model,
  status,
  current_latitude, current_longitude,
  hub_id,
  rating, total_deliveries, total_distance_km,
  is_verified, is_online,
  created_at, updated_at
) VALUES
(
  'driver_001',
  'tenant_test_001',
  'Juan', 'Dela Cruz',
  'juan.delacruz@fastship.ph',
  '+639171500001',
  'NCR-2019-123456',
  '2027-12-31',
  'motorcycle',
  'ABC 1234',
  'Honda Click 125i',
  'AVAILABLE',
  14.6760, 121.0437,
  'hub_manila_001',
  4.82, 1247, 18934.5,
  true, true,
  NOW() - INTERVAL '60 days', NOW()
),
(
  'driver_002',
  'tenant_test_001',
  'Pedro', 'Reyes',
  'pedro.reyes@fastship.ph',
  '+639171500002',
  'NCR-2020-234567',
  '2026-08-15',
  'van',
  'XYZ 5678',
  'Toyota Hi-Ace Commuter',
  'AVAILABLE',
  14.5995, 120.9842,
  'hub_manila_001',
  4.91, 2345, 42100.0,
  true, true,
  NOW() - INTERVAL '55 days', NOW()
),
(
  'driver_003',
  'tenant_test_001',
  'Maria', 'Garcia',
  'maria.garcia@fastship.ph',
  '+639171500003',
  'CEB-2021-345678',
  '2028-03-20',
  'motorcycle',
  'CEB 9012',
  'Yamaha Mio i125',
  'AVAILABLE',
  10.3157, 123.8854,
  'hub_cebu_001',
  4.76, 876, 12450.0,
  true, true,
  NOW() - INTERVAL '50 days', NOW()
),
(
  'driver_004',
  'tenant_test_001',
  'Antonio', 'Bautista',
  'antonio.bautista@fastship.ph',
  '+639171500004',
  'DAV-2020-456789',
  '2025-11-30',
  'motorcycle',
  'DAV 3456',
  'Honda Beat 110',
  'AVAILABLE',
  7.1907, 125.4553,
  'hub_davao_001',
  4.65, 654, 9823.5,
  true, true,
  NOW() - INTERVAL '45 days', NOW()
),
(
  'driver_005',
  'tenant_test_001',
  'Rosa', 'Mendoza',
  'rosa.mendoza@fastship.ph',
  '+639171500005',
  'NCR-2022-567890',
  '2029-06-15',
  'bicycle',
  NULL,
  'Custom Cargo Bike',
  'AVAILABLE',
  14.5547, 121.0244,
  'hub_manila_001',
  4.95, 432, 3210.0,
  true, false,
  NOW() - INTERVAL '40 days', NOW()
)
ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM drivers WHERE id LIKE 'driver_%'")
success "Drivers: $COUNT records (target: 5)"

# ============================================================
# CARRIERS
# ============================================================
section "Seeding Carriers"

psql_exec <<'SQL'
INSERT INTO carriers (
  id, tenant_id,
  name, code,
  carrier_type,
  contact_email, contact_phone,
  api_base_url, api_key, api_secret,
  tracking_url_template,
  supported_services,
  coverage_zones,
  max_weight_kg, max_dimension_cm,
  base_rate_php, rate_per_kg_php,
  cod_enabled, cod_fee_pct,
  sla_domestic_days, sla_provincial_days,
  status,
  performance_on_time_pct, performance_rating,
  created_at, updated_at
) VALUES
(
  'carrier_lbc_001',
  'tenant_test_001',
  'LBC Express',
  'LBC',
  'domestic',
  'corporate@lbc.com.ph',
  '+6327990000',
  'https://api.lbcexpress.com/v2',
  'lbc_api_key_placeholder_dev',
  'lbc_api_secret_placeholder_dev',
  'https://www.lbcexpress.com/track?awb={awb}',
  '["same_day", "next_day", "standard", "economy", "bulky"]',
  '["NCR", "Luzon", "Visayas", "Mindanao", "International"]',
  50.00,
  '{"length": 150, "width": 100, "height": 100}',
  80.00, 15.00,
  true, 2.5,
  1, 3,
  'active',
  94.2, 4.7,
  NOW() - INTERVAL '90 days', NOW()
),
(
  'carrier_jnt_001',
  'tenant_test_001',
  'J&T Express Philippines',
  'JNT',
  'domestic',
  'ph.partnership@jtexpress.ph',
  '+6329901234',
  'https://api.jtexpress.ph/v1',
  'jnt_api_key_placeholder_dev',
  'jnt_api_secret_placeholder_dev',
  'https://www.jtexpress.ph/trajectoryQuery?bills={awb}',
  '["next_day", "standard", "economy"]',
  '["NCR", "Luzon", "Visayas", "Mindanao"]',
  30.00,
  '{"length": 100, "width": 80, "height": 80}',
  60.00, 12.00,
  true, 2.0,
  1, 3,
  'active',
  91.8, 4.5,
  NOW() - INTERVAL '85 days', NOW()
),
(
  'carrier_ninja_001',
  'tenant_test_001',
  'Ninja Van Philippines',
  'NVN',
  'domestic',
  'ph@ninjavan.co',
  '+6329991234',
  'https://api.ninjavan.co/ph/v2',
  'nvn_api_key_placeholder_dev',
  'nvn_api_secret_placeholder_dev',
  'https://www.ninjavan.co/en-ph/tracking?id={awb}',
  '["next_day", "standard", "bulky", "returns"]',
  '["NCR", "Luzon", "Visayas", "Mindanao"]',
  100.00,
  '{"length": 200, "width": 150, "height": 150}',
  70.00, 18.00,
  true, 3.0,
  1, 4,
  'active',
  89.5, 4.3,
  NOW() - INTERVAL '80 days', NOW()
)
ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM carriers WHERE id LIKE 'carrier_%'")
success "Carriers: $COUNT records (target: 3)"

# ============================================================
# SHIPMENTS
# ============================================================
section "Seeding Shipments (20 sample orders)"

psql_exec <<'SQL'
-- Helper: generate AWB numbers in a predictable pattern
INSERT INTO shipments (
  id, tenant_id, merchant_id,
  awb, reference_number,
  status,
  sender_name, sender_phone, sender_address, sender_city, sender_province, sender_postal,
  recipient_name, recipient_phone, recipient_address, recipient_city, recipient_province, recipient_postal,
  weight_kg, declared_value_php,
  payment_type, cod_amount_php,
  service_type, assigned_carrier_id, assigned_driver_id, assigned_hub_id,
  pickup_scheduled_at, picked_up_at, delivered_at,
  delivery_attempts,
  notes,
  created_at, updated_at
) VALUES
-- 1. Delivered
('ship_001','tenant_test_001','merchant_001_01','LOS2024030001','SHOP-ORD-10001','DELIVERED',
 'Shopee Flash Deals','+639171000002','2F Robinsons Galleria, Ortigas','Pasig','Metro Manila','1600',
 'Ana Reyes','+639181110001','123 Burgos St, Ayala Alabang','Muntinlupa','Metro Manila','1780',
 0.5,1200.00,'cod',1200.00,'standard','carrier_lbc_001','driver_001','hub_manila_001',
 NOW()-INTERVAL '5 days',NOW()-INTERVAL '4 days 8 hours',NOW()-INTERVAL '4 days 2 hours',
 1,'Handle with care',NOW()-INTERVAL '5 days',NOW()-INTERVAL '4 days 2 hours'),

-- 2. In transit
('ship_002','tenant_test_001','merchant_001_02','LOS2024030002','SHOP-ORD-10002','IN_TRANSIT',
 'Shopee Flash Deals','+639171000002','2F Robinsons Galleria, Ortigas','Pasig','Metro Manila','1600',
 'Ben Cruz','+639181110002','456 Molave St, Marikina','Marikina','Metro Manila','1810',
 1.2,2500.00,'prepaid',0.00,'standard','carrier_lbc_001','driver_001','hub_manila_001',
 NOW()-INTERVAL '2 days',NOW()-INTERVAL '1 day 10 hours',NULL,
 0,NULL,NOW()-INTERVAL '2 days',NOW()),

-- 3. Out for delivery
('ship_003','tenant_test_001','merchant_001_03','LOS2024030003','BB-ORD-30003','OUT_FOR_DELIVERY',
 'Balikbayan Padala Hub','+639171000003','123 España Blvd, Sampaloc','Manila','Metro Manila','1008',
 'Carlos Dizon','+639181110003','789 Rizal Ave, Caloocan','Caloocan','Metro Manila','1400',
 5.0,0.00,'prepaid',0.00,'economy','carrier_jnt_001','driver_002','hub_manila_001',
 NOW()-INTERVAL '3 days',NOW()-INTERVAL '2 days',NULL,
 0,'Balikbayan box - fragile electronics',NOW()-INTERVAL '3 days',NOW()),

-- 4. Pending pickup
('ship_004','tenant_test_001','merchant_001_04','LOS2024030004','GROC-ORD-40004','PENDING_PICKUP',
 'GroceryDash PH','+639171000004','456 Quezon Ave, Quezon City','Quezon City','Metro Manila','1103',
 'Diana Santos','+639181110004','321 Kamias Rd, Quezon City','Quezon City','Metro Manila','1102',
 3.5,850.00,'cod',850.00,'same_day',NULL,'driver_001','hub_manila_001',
 NOW()+INTERVAL '2 hours',NULL,NULL,
 0,NULL,NOW(),NOW()),

-- 5. Picked up
('ship_005','tenant_test_001','merchant_001_05','LOS2024030005','MED-ORD-50005','PICKED_UP',
 'MedExpress Pharmacy','+639171000005','789 UN Ave, Paco','Manila','Metro Manila','1007',
 'Eduardo Lim','+639181110005','654 Shaw Blvd, Mandaluyong','Mandaluyong','Metro Manila','1550',
 0.3,1500.00,'prepaid',0.00,'next_day','carrier_ninja_001','driver_002','hub_manila_001',
 NOW()-INTERVAL '1 day',NOW()-INTERVAL '22 hours',NULL,
 0,'Cold chain — keep refrigerated',NOW()-INTERVAL '1 day',NOW()),

-- 6. Failed delivery
('ship_006','tenant_test_001','merchant_001_06','LOS2024030006','TG-ORD-60006','FAILED',
 'TechGadgets Online','+639171000006','SM Megamall B2, Mandaluyong','Mandaluyong','Metro Manila','1550',
 'Filipina Ocampo','+639181110006','987 Taft Ave, Manila','Manila','Metro Manila','1000',
 2.0,8500.00,'cod',8500.00,'standard','carrier_lbc_001','driver_001','hub_manila_001',
 NOW()-INTERVAL '2 days',NOW()-INTERVAL '1 day 12 hours',NULL,
 3,'Consignee refused delivery — COD amount disputed',NOW()-INTERVAL '2 days',NOW()),

-- 7. Returned
('ship_007','tenant_test_001','merchant_001_07','LOS2024030007','FF-ORD-70007','RETURNED',
 'FashionForward PH','+639171000007','Greenbelt 5, Makati','Makati','Metro Manila','1200',
 'Gabriel Torres','+639181110007','246 Session Rd, Baguio','Baguio City','Benguet','2600',
 1.0,3200.00,'cod',3200.00,'standard','carrier_ninja_001',NULL,'hub_manila_001',
 NOW()-INTERVAL '7 days',NOW()-INTERVAL '6 days',NULL,
 2,'Wrong size — consignee requested return',NOW()-INTERVAL '7 days',NOW()-INTERVAL '2 days'),

-- 8. At hub
('ship_008','tenant_test_001','merchant_001_08','LOS2024030008','HD-ORD-80008','AT_HUB',
 'HomeDecor Direct','+639171000008','1010 EDSA, Quezon City','Quezon City','Metro Manila','1103',
 'Helena Villanueva','+639181110008','135 Osmena Blvd, Cebu City','Cebu City','Cebu','6000',
 4.5,6000.00,'prepaid',0.00,'standard','carrier_jnt_001',NULL,'hub_cebu_001',
 NOW()-INTERVAL '3 days',NOW()-INTERVAL '2 days',NULL,
 0,'Glass item — fragile',NOW()-INTERVAL '3 days',NOW()),

-- 9. Created / not yet assigned
('ship_009','tenant_test_001','merchant_001_09','LOS2024030009','OS-ORD-90009','CREATED',
 'OfficeSupply Central','+639171000009','Fort Bonifacio, Taguig','Taguig','Metro Manila','1634',
 'Ivan Ramos','+639181110009','579 A. Mabini St, Davao City','Davao City','Davao del Sur','8000',
 8.0,12000.00,'prepaid',0.00,'economy',NULL,NULL,NULL,
 NULL,NULL,NULL,
 0,NULL,NOW(),NOW()),

-- 10. Delivered COD
('ship_010','tenant_test_001','merchant_001_10','LOS2024030010','AP-ORD-10010','DELIVERED',
 'AutoParts Express PH','+639171000010','PEZA Laguna Technopark','Binan','Laguna','4024',
 'Josephine Castro','+639181110010','802 Magsaysay Blvd, Manila','Manila','Metro Manila','1000',
 6.5,4500.00,'cod',4500.00,'standard','carrier_lbc_001','driver_003','hub_manila_001',
 NOW()-INTERVAL '4 days',NOW()-INTERVAL '3 days 6 hours',NOW()-INTERVAL '3 days',
 1,NULL,NOW()-INTERVAL '4 days',NOW()-INTERVAL '3 days'),

-- 11-20: Cebu tenant shipments
('ship_011','tenant_test_002','merchant_002_01','LOS2024030011','SU-ORD-11001','DELIVERED',
 'Sugbo Ukay-Ukay','+639281000001','Carbon Market, Cebu City','Cebu City','Cebu','6000',
 'Kristine Uy','+639281110001','10 Fuente Osmena, Cebu City','Cebu City','Cebu','6000',
 1.5,350.00,'cod',350.00,'standard',NULL,'driver_003','hub_cebu_001',
 NOW()-INTERVAL '3 days',NOW()-INTERVAL '2 days',NOW()-INTERVAL '2 days 5 hours',
 1,NULL,NOW()-INTERVAL '3 days',NOW()-INTERVAL '2 days 5 hours'),

('ship_012','tenant_test_002','merchant_002_02','LOS2024030012','CL-ORD-12002','IN_TRANSIT',
 'Cebu Lechon Express','+639281000002','Mactan Island, Lapu-Lapu','Lapu-Lapu','Cebu','6015',
 'Lorenzo Tan','+639281110002','25 V. Rama Ave, Cebu City','Cebu City','Cebu','6000',
 3.0,1800.00,'cod',1800.00,'same_day',NULL,'driver_003','hub_cebu_001',
 NOW()-INTERVAL '6 hours',NOW()-INTERVAL '4 hours',NULL,
 0,'Lechon — keep upright',NOW()-INTERVAL '6 hours',NOW()),

('ship_013','tenant_test_002','merchant_002_03','LOS2024030013','VF-ORD-13003','PENDING_PICKUP',
 'Visayas Fresh Produce','+639281000003','Talisay City, Cebu','Talisay','Cebu','6045',
 'Maribel Go','+639281110003','50 M. Velez St, Cebu City','Cebu City','Cebu','6000',
 5.0,600.00,'cod',600.00,'same_day',NULL,NULL,'hub_cebu_001',
 NOW()+INTERVAL '1 hour',NULL,NULL,
 0,NULL,NOW(),NOW()),

('ship_014','tenant_test_002','merchant_002_04','LOS2024030014','IT-ORD-14004','AT_HUB',
 'IT Park Electronics','+639281000004','Cebu IT Park, Lahug','Cebu City','Cebu','6000',
 'Nestor Bohol','+639281110004','15 Jakosalem St, Cebu City','Cebu City','Cebu','6000',
 0.8,4500.00,'prepaid',0.00,'next_day','carrier_jnt_001',NULL,'hub_cebu_001',
 NOW()-INTERVAL '1 day',NOW()-INTERVAL '20 hours',NULL,
 0,NULL,NOW()-INTERVAL '1 day',NOW()),

('ship_015','tenant_test_002','merchant_002_05','LOS2024030015','CH-ORD-15005','DELIVERED',
 'Cebu Handicrafts','+639281000005','Colon Street, Cebu City','Cebu City','Cebu','6000',
 'Ophelia Padilla','+639281110005','300 Osmeña Blvd, Cebu City','Cebu City','Cebu','6000',
 0.5,1200.00,'cod',1200.00,'standard',NULL,'driver_003','hub_cebu_001',
 NOW()-INTERVAL '5 days',NOW()-INTERVAL '4 days',NOW()-INTERVAL '4 days 3 hours',
 1,NULL,NOW()-INTERVAL '5 days',NOW()-INTERVAL '4 days 3 hours'),

('ship_016','tenant_test_002','merchant_002_06','LOS2024030016','BH-ORD-16006','CREATED',
 'Bohol Honey Farm','+639281000006','Tagbilaran City, Bohol','Tagbilaran','Bohol','6300',
 'Pablo Alcantara','+639281110006','75 Jakosalem St, Cebu City','Cebu City','Cebu','6000',
 2.0,800.00,'cod',800.00,'standard',NULL,NULL,NULL,
 NULL,NULL,NULL,
 0,NULL,NOW(),NOW()),

('ship_017','tenant_test_002','merchant_002_07','LOS2024030017','IS-ORD-17007','OUT_FOR_DELIVERY',
 'Iloilo Style House','+639281000007','SM City Iloilo, Mandurriao','Iloilo City','Iloilo','5000',
 'Queenie Flores','+639281110007','120 Iznart St, Iloilo City','Iloilo City','Iloilo','5000',
 0.7,2200.00,'cod',2200.00,'standard','carrier_lbc_001',NULL,'hub_cebu_001',
 NOW()-INTERVAL '2 days',NOW()-INTERVAL '1 day 8 hours',NULL,
 0,NULL,NOW()-INTERVAL '2 days',NOW()),

('ship_018','tenant_test_002','merchant_002_08','LOS2024030018','DB-ORD-18008','DELIVERED',
 'Dumaguete Books','+639281000008','Silliman Ave, Dumaguete','Dumaguete','Negros Oriental','6200',
 'Ramon Estrada','+639281110008','88 South Road, Dumaguete','Dumaguete','Negros Oriental','6200',
 1.2,450.00,'prepaid',0.00,'standard',NULL,NULL,'hub_cebu_001',
 NOW()-INTERVAL '6 days',NOW()-INTERVAL '5 days',NOW()-INTERVAL '5 days 4 hours',
 1,NULL,NOW()-INTERVAL '6 days',NOW()-INTERVAL '5 days 4 hours'),

('ship_019','tenant_test_002','merchant_002_09','LOS2024030019','BS-ORD-19009','IN_TRANSIT',
 'Bacolod Sweet Delights','+639281000009','Lacson Street, Bacolod','Bacolod City','Negros Occidental','6100',
 'Simplicia Garcia','+639281110009','60 North Drive, Bacolod City','Bacolod City','Negros Occidental','6100',
 1.5,900.00,'cod',900.00,'standard','carrier_ninja_001',NULL,'hub_cebu_001',
 NOW()-INTERVAL '2 days',NOW()-INTERVAL '1 day 12 hours',NULL,
 0,'Keep dry — pastries inside',NOW()-INTERVAL '2 days',NOW()),

('ship_020','tenant_test_002','merchant_002_10','LOS2024030020','LS-ORD-20010','PICKED_UP',
 'Leyte Seafoods Direct','+639281000010','Real Street, Tacloban','Tacloban','Leyte','6500',
 'Tomas Navarro','+639281110010','200 Airport Road, Tacloban','Tacloban','Leyte','6500',
 4.0,1600.00,'cod',1600.00,'same_day',NULL,NULL,'hub_cebu_001',
 NOW()-INTERVAL '5 hours',NOW()-INTERVAL '3 hours',NULL,
 0,'Fresh seafood — cold chain required',NOW()-INTERVAL '5 hours',NOW())

ON CONFLICT (id) DO NOTHING;
SQL

COUNT=$(psql_count "SELECT COUNT(*) FROM shipments WHERE id LIKE 'ship_%'")
success "Shipments: $COUNT records (target: 20)"

# ============================================================
# SUMMARY
# ============================================================
echo ""
echo -e "${BOLD}${GREEN}╔══════════════════════════════════════════╗${RESET}"
echo -e "${BOLD}${GREEN}║  Seed Data Summary                       ║${RESET}"
echo -e "${BOLD}${GREEN}╚══════════════════════════════════════════╝${RESET}"
echo ""

TENANT_COUNT=$(psql_count "SELECT COUNT(*) FROM tenants WHERE id LIKE 'tenant_test_%'")
MERCHANT_COUNT=$(psql_count "SELECT COUNT(*) FROM merchants WHERE id LIKE 'merchant_%'")
DRIVER_COUNT=$(psql_count "SELECT COUNT(*) FROM drivers WHERE id LIKE 'driver_%'")
HUB_COUNT=$(psql_count "SELECT COUNT(*) FROM hubs WHERE id LIKE 'hub_%'")
SHIPMENT_COUNT=$(psql_count "SELECT COUNT(*) FROM shipments WHERE id LIKE 'ship_%'")
CARRIER_COUNT=$(psql_count "SELECT COUNT(*) FROM carriers WHERE id LIKE 'carrier_%'")

echo -e "  ${GREEN}✓${RESET} Tenants:     ${BOLD}$TENANT_COUNT${RESET} (target: 2)"
echo -e "  ${GREEN}✓${RESET} Merchants:   ${BOLD}$MERCHANT_COUNT${RESET} (target: 20)"
echo -e "  ${GREEN}✓${RESET} Drivers:     ${BOLD}$DRIVER_COUNT${RESET} (target: 5, all AVAILABLE)"
echo -e "  ${GREEN}✓${RESET} Hubs:        ${BOLD}$HUB_COUNT${RESET} (target: 3)"
echo -e "  ${GREEN}✓${RESET} Shipments:   ${BOLD}$SHIPMENT_COUNT${RESET} (target: 20)"
echo -e "  ${GREEN}✓${RESET} Carriers:    ${BOLD}$CARRIER_COUNT${RESET} (target: 3)"
echo ""

# Shipment status breakdown
echo -e "${BOLD}Shipment Status Breakdown:${RESET}"
psql "$DATABASE_URL" --no-psqlrc -c \
  "SELECT status, COUNT(*) as count FROM shipments WHERE id LIKE 'ship_%' GROUP BY status ORDER BY count DESC;" \
  2>/dev/null || true

echo ""
echo -e "${BOLD}Test Credentials:${RESET}"
echo -e "  Tenant 1 ID:  ${CYAN}tenant_test_001${RESET}  (FastShip Philippines)"
echo -e "  Tenant 2 ID:  ${CYAN}tenant_test_002${RESET}  (QuickDeliver Cebu)"
echo -e "  Sample AWB:   ${CYAN}LOS2024030001${RESET}"
echo ""
success "Development seed data loaded successfully."
