/**
 * Partner Portal — Marketplace API.
 *
 * Mirrors the schema defined in ADR-0013 (Marketplace Discovery addendum):
 *   - marketplace.vehicle_listings
 *   - marketplace.bookings
 *
 * Pre-backend stub: returns mock data shaped exactly as the future
 * `GET /v1/marketplace/listings` and `GET /v1/marketplace/bookings` will.
 * Swap to `authFetch` when the service ships — the caller contract is stable.
 */

// ── Types (ADR-0013) ──────────────────────────────────────────────────────────

export type ListingStatus = "active" | "paused" | "booked" | "expired";

export type SizeClass =
  | "motorcycle"
  | "sedan"
  | "van"
  | "l300"
  | "6wheeler"
  | "10wheeler"
  | "trailer";

export interface VehicleListing {
  id:                       string;
  tenant_id:                string;
  partner_id:               string;
  vehicle_id:               string;
  vehicle_plate:             string;   // display-only, resolved from fleet.vehicles
  size_class:               SizeClass;
  max_weight_kg:            number;
  max_volume_m3:            number | null;
  base_price_cents:         number;
  per_km_cents:             number;
  per_kg_cents:             number | null;
  service_area_label:       string;    // display-only, pretty name of the polygon
  idle_from:                string;    // ISO-8601
  idle_until:               string;    // ISO-8601
  status:                   ListingStatus;
  carrier_response_window_mins: number;
  bookings_today:           number;    // computed on server in the real impl
  revenue_today_cents:      number;    // same
  created_at:               string;
  updated_at:               string;
}

export type BookingStatus =
  | "pending"
  | "accepted"
  | "rejected"
  | "in_transit"
  | "delivered"
  | "cancelled"
  | "disputed";

export interface MarketplaceBooking {
  id:                   string;
  listing_id:           string;
  shipment_id:          string;
  awb:                  string;       // CM-{TTT}-{S}{NNNNNNN}{C}
  consumer_name:        string;       // masked until accepted
  consumer_phone:       string | null;
  pickup_label:         string;
  dropoff_label:        string;
  cargo_weight_kg:      number;
  cargo_volume_m3:      number | null;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;       // ISO-8601
  created_at:           string;
}

// ── Mock data ─────────────────────────────────────────────────────────────────

const TENANT_ID  = "00000000-0000-0000-0000-000000000001";
const PARTNER_ID = "a1b2c3d4-0000-0000-0000-000000000001"; // FastShip Co.

const now = () => new Date();
const iso = (d: Date) => d.toISOString();
const addHours = (d: Date, h: number) => new Date(d.getTime() + h * 3_600_000);

const MOCK_LISTINGS: VehicleListing[] = [
  {
    id:                           "l1000000-0000-0000-0000-000000000001",
    tenant_id:                    TENANT_ID,
    partner_id:                   PARTNER_ID,
    vehicle_id:                   "v1000000-0000-0000-0000-000000000001",
    vehicle_plate:                "NKT-4821",
    size_class:                   "l300",
    max_weight_kg:                1500,
    max_volume_m3:                8.5,
    base_price_cents:             150000,
    per_km_cents:                 2500,
    per_kg_cents:                 null,
    service_area_label:           "Metro Manila · Luzon",
    idle_from:                    iso(addHours(now(), -2)),
    idle_until:                   iso(addHours(now(), 6)),
    status:                       "active",
    carrier_response_window_mins: 15,
    bookings_today:               3,
    revenue_today_cents:          540000,
    created_at:                   iso(addHours(now(), -72)),
    updated_at:                   iso(addHours(now(), -2)),
  },
  {
    id:                           "l1000000-0000-0000-0000-000000000002",
    tenant_id:                    TENANT_ID,
    partner_id:                   PARTNER_ID,
    vehicle_id:                   "v1000000-0000-0000-0000-000000000002",
    vehicle_plate:                "JBX-9930",
    size_class:                   "motorcycle",
    max_weight_kg:                30,
    max_volume_m3:                0.25,
    base_price_cents:             8000,
    per_km_cents:                 900,
    per_kg_cents:                 1500,
    service_area_label:           "Metro Manila only",
    idle_from:                    iso(addHours(now(), -1)),
    idle_until:                   iso(addHours(now(), 4)),
    status:                       "booked",
    carrier_response_window_mins: 10,
    bookings_today:               7,
    revenue_today_cents:          63000,
    created_at:                   iso(addHours(now(), -120)),
    updated_at:                   iso(addHours(now(), -0.5)),
  },
  {
    id:                           "l1000000-0000-0000-0000-000000000003",
    tenant_id:                    TENANT_ID,
    partner_id:                   PARTNER_ID,
    vehicle_id:                   "v1000000-0000-0000-0000-000000000003",
    vehicle_plate:                "TKL-5501",
    size_class:                   "6wheeler",
    max_weight_kg:                6000,
    max_volume_m3:                22.0,
    base_price_cents:             450000,
    per_km_cents:                 4200,
    per_kg_cents:                 null,
    service_area_label:           "Luzon-wide inc. provincial",
    idle_from:                    iso(addHours(now(), 2)),
    idle_until:                   iso(addHours(now(), 24)),
    status:                       "active",
    carrier_response_window_mins: 30,
    bookings_today:               1,
    revenue_today_cents:          820000,
    created_at:                   iso(addHours(now(), -48)),
    updated_at:                   iso(addHours(now(), -4)),
  },
  {
    id:                           "l1000000-0000-0000-0000-000000000004",
    tenant_id:                    TENANT_ID,
    partner_id:                   PARTNER_ID,
    vehicle_id:                   "v1000000-0000-0000-0000-000000000004",
    vehicle_plate:                "VAN-3372",
    size_class:                   "van",
    max_weight_kg:                800,
    max_volume_m3:                5.0,
    base_price_cents:             90000,
    per_km_cents:                 1800,
    per_kg_cents:                 null,
    service_area_label:           "NCR + Rizal + Cavite",
    idle_from:                    iso(addHours(now(), -6)),
    idle_until:                   iso(addHours(now(), -1)),
    status:                       "expired",
    carrier_response_window_mins: 15,
    bookings_today:               0,
    revenue_today_cents:          0,
    created_at:                   iso(addHours(now(), -200)),
    updated_at:                   iso(addHours(now(), -1)),
  },
  {
    id:                           "l1000000-0000-0000-0000-000000000005",
    tenant_id:                    TENANT_ID,
    partner_id:                   PARTNER_ID,
    vehicle_id:                   "v1000000-0000-0000-0000-000000000005",
    vehicle_plate:                "SDN-1105",
    size_class:                   "sedan",
    max_weight_kg:                200,
    max_volume_m3:                1.2,
    base_price_cents:             35000,
    per_km_cents:                 1200,
    per_kg_cents:                 800,
    service_area_label:           "Metro Manila",
    idle_from:                    iso(addHours(now(), -3)),
    idle_until:                   iso(addHours(now(), 9)),
    status:                       "paused",
    carrier_response_window_mins: 15,
    bookings_today:               0,
    revenue_today_cents:          0,
    created_at:                   iso(addHours(now(), -30)),
    updated_at:                   iso(addHours(now(), -0.2)),
  },
];

const MOCK_BOOKINGS: MarketplaceBooking[] = [
  {
    id:                 "b1000000-0000-0000-0000-000000000001",
    listing_id:         "l1000000-0000-0000-0000-000000000002",
    shipment_id:        "s1000000-0000-0000-0000-000000000001",
    awb:                "CM-PHL-S0000042X",
    consumer_name:      "M. Reyes",
    consumer_phone:     "+63 917 ••• 2811",
    pickup_label:       "Makati CBD",
    dropoff_label:      "BGC, Taguig",
    cargo_weight_kg:    12,
    cargo_volume_m3:    0.15,
    quoted_price_cents: 14500,
    status:             "in_transit",
    pickup_at:          iso(addHours(now(), -0.5)),
    created_at:         iso(addHours(now(), -1)),
  },
  {
    id:                 "b1000000-0000-0000-0000-000000000002",
    listing_id:         "l1000000-0000-0000-0000-000000000001",
    shipment_id:        "s1000000-0000-0000-0000-000000000002",
    awb:                "CM-PHL-E0000099Y",
    consumer_name:      "A. Dela Cruz",
    consumer_phone:     "+63 927 ••• 5043",
    pickup_label:       "Pasig Warehouse",
    dropoff_label:      "Laguna Techno Park",
    cargo_weight_kg:    820,
    cargo_volume_m3:    4.2,
    quoted_price_cents: 285000,
    status:             "pending",
    pickup_at:          iso(addHours(now(), 1.5)),
    created_at:         iso(addHours(now(), -0.3)),
  },
  {
    id:                 "b1000000-0000-0000-0000-000000000003",
    listing_id:         "l1000000-0000-0000-0000-000000000003",
    shipment_id:        "s1000000-0000-0000-0000-000000000003",
    awb:                "CM-PHL-S0000121K",
    consumer_name:      "Sy Lumber Corp.",
    consumer_phone:     "+63 2 8812 ••••",
    pickup_label:       "Valenzuela Lumberyard",
    dropoff_label:      "Tarlac City",
    cargo_weight_kg:    4200,
    cargo_volume_m3:    18.0,
    quoted_price_cents: 920000,
    status:             "accepted",
    pickup_at:          iso(addHours(now(), 4)),
    created_at:         iso(addHours(now(), -2)),
  },
];

// ── API stubs (swap to authFetch when backend lands) ──────────────────────────

const latency = (ms = 220) => new Promise((r) => setTimeout(r, ms));

export async function fetchListings(): Promise<VehicleListing[]> {
  await latency();
  return structuredClone(MOCK_LISTINGS);
}

export async function fetchBookings(): Promise<MarketplaceBooking[]> {
  await latency();
  return structuredClone(MOCK_BOOKINGS);
}

export async function createListing(
  input: Omit<VehicleListing, "id" | "tenant_id" | "partner_id" | "vehicle_id" | "bookings_today" | "revenue_today_cents" | "created_at" | "updated_at">,
): Promise<VehicleListing> {
  await latency();
  const listing: VehicleListing = {
    ...input,
    id:                  `l1000000-0000-0000-0000-${Date.now().toString().padStart(12, "0")}`,
    tenant_id:           TENANT_ID,
    partner_id:          PARTNER_ID,
    vehicle_id:          `v1000000-0000-0000-0000-${Date.now().toString().padStart(12, "0")}`,
    bookings_today:      0,
    revenue_today_cents: 0,
    created_at:          iso(now()),
    updated_at:          iso(now()),
  };
  MOCK_LISTINGS.unshift(listing);
  return listing;
}

export async function updateListing(
  id: string,
  patch: Partial<Pick<VehicleListing, "status" | "base_price_cents" | "per_km_cents" | "per_kg_cents" | "idle_until" | "service_area_label" | "max_weight_kg" | "max_volume_m3" | "carrier_response_window_mins">>,
): Promise<VehicleListing | null> {
  await latency();
  const i = MOCK_LISTINGS.findIndex((l) => l.id === id);
  if (i === -1) return null;
  MOCK_LISTINGS[i] = { ...MOCK_LISTINGS[i], ...patch, updated_at: iso(now()) };
  return structuredClone(MOCK_LISTINGS[i]);
}

export async function deleteListing(id: string): Promise<boolean> {
  await latency();
  const i = MOCK_LISTINGS.findIndex((l) => l.id === id);
  if (i === -1) return false;
  MOCK_LISTINGS.splice(i, 1);
  return true;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

export const SIZE_CLASS_LABEL: Record<SizeClass, string> = {
  motorcycle: "Motorcycle",
  sedan:      "Sedan",
  van:        "Van",
  l300:       "L300 / Pickup",
  "6wheeler": "6-Wheeler",
  "10wheeler": "10-Wheeler",
  trailer:    "Trailer",
};

export const SIZE_CLASS_CAPACITY_HINT: Record<SizeClass, string> = {
  motorcycle:  "Up to 30 kg · 0.25 m³",
  sedan:       "Up to 200 kg · 1.2 m³",
  van:         "Up to 800 kg · 5 m³",
  l300:        "Up to 1,500 kg · 8.5 m³",
  "6wheeler":  "Up to 6,000 kg · 22 m³",
  "10wheeler": "Up to 12,000 kg · 40 m³",
  trailer:     "Up to 25,000 kg · 80 m³",
};

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
