/**
 * Admin Portal — Marketplace oversight API.
 *
 * Tenant-scoped view across every partner's `marketplace.vehicle_listings`
 * and `marketplace.bookings` (ADR-0013). The tenant_admin sees all rows by
 * virtue of `scope=tenant` in the session GUC — no partner filter applied.
 *
 * Pre-backend stub. Swap to `authFetch` when the service ships.
 *
 * Cross-portal propagation (pre-backend): merchant-portal and partner-portal
 * publish to the shared marketplace-bus; this module merges all rows in
 * `fetchAllBookings()` without partner filter (ADR-0013 §RLS extension:
 * scope=tenant matches any partner_id / merchant_id within the tenant).
 */

import {
  readBus,
  subscribeToBus,
  type BusBooking,
} from "./marketplace-bus";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ListingStatus  = "active" | "paused" | "booked" | "expired";
export type BookingStatus  = "pending" | "accepted" | "rejected" | "in_transit" | "delivered" | "cancelled" | "disputed";
export type SizeClass      = "motorcycle" | "sedan" | "van" | "l300" | "6wheeler" | "10wheeler" | "trailer";
export type PartnerType    = "alliance" | "marketplace";
export type MerchantType   = "business" | "consumer";    // ADR-0013: business = tenant merchant; consumer = walk-up booker

export interface AdminListing {
  id:                   string;
  partner_id:           string;
  partner_display_name: string;
  partner_type:         PartnerType;       // alliance = full; marketplace = vehicle-only
  vehicle_plate:        string;
  size_class:           SizeClass;
  max_weight_kg:        number;
  base_price_cents:     number;
  per_km_cents:         number;
  service_area_label:   string;
  idle_until:           string;
  status:               ListingStatus;
  bookings_today:       number;
  revenue_today_cents:  number;
  rating:               number;             // 0..5, computed from carrier response history
  updated_at:           string;
}

export interface AdminBooking {
  id:                   string;
  shipment_id:          string;             // FK to shipments; used for dispatch deep-link
  awb:                  string;
  partner_id:           string;
  partner_display_name: string;
  merchant_type:        MerchantType;       // business = tenant merchant; consumer = individual booker
  merchant_id:          string | null;      // populated when merchant_type=business
  consumer_display:     string;             // masked display, pre-accept
  size_class:           SizeClass;
  cargo_weight_kg:      number;
  pickup_label:         string;
  dropoff_label:        string;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;
  created_at:           string;
}

export interface MarketplaceStats {
  active_listings:       number;
  idle_vehicles_next_6h: number;
  bookings_today:        number;
  gmv_today_cents:       number;
  avg_match_seconds:     number;             // booking intent → carrier accepted
  partners_participating: number;
}

// ── Mock data ─────────────────────────────────────────────────────────────────

const iso = (d: Date) => d.toISOString();
const addHours = (d: Date, h: number) => new Date(d.getTime() + h * 3_600_000);
const now = () => new Date();

const P_FASTSHIP = { id: "a1b2c3d4-0000-0000-0000-000000000001", name: "FastShip Co.",      type: "alliance"    as PartnerType };
const P_NORTH    = { id: "a1b2c3d4-0000-0000-0000-000000000002", name: "NorthLink Logistics", type: "alliance"   as PartnerType };
const P_MANILA   = { id: "a1b2c3d4-0000-0000-0000-000000000003", name: "Manila MoveIt",     type: "marketplace" as PartnerType };
const P_CEBU     = { id: "a1b2c3d4-0000-0000-0000-000000000004", name: "Cebu Carriers Co-op", type: "marketplace" as PartnerType };

const MOCK_LISTINGS: AdminListing[] = [
  {
    id: "l1000000-0000-0000-0000-000000000001",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "NKT-4821", size_class: "l300", max_weight_kg: 1500,
    base_price_cents: 150000, per_km_cents: 2500,
    service_area_label: "Metro Manila · Luzon",
    idle_until: iso(addHours(now(), 6)),
    status: "active", bookings_today: 3, revenue_today_cents: 540000, rating: 4.8,
    updated_at: iso(addHours(now(), -2)),
  },
  {
    id: "l1000000-0000-0000-0000-000000000002",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "JBX-9930", size_class: "motorcycle", max_weight_kg: 30,
    base_price_cents: 8000, per_km_cents: 900,
    service_area_label: "Metro Manila only",
    idle_until: iso(addHours(now(), 4)),
    status: "booked", bookings_today: 7, revenue_today_cents: 63000, rating: 4.9,
    updated_at: iso(addHours(now(), -0.5)),
  },
  {
    id: "l2000000-0000-0000-0000-000000000001",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name, partner_type: P_NORTH.type,
    vehicle_plate: "TLX-7765", size_class: "10wheeler", max_weight_kg: 12000,
    base_price_cents: 800000, per_km_cents: 5500,
    service_area_label: "Luzon inter-provincial",
    idle_until: iso(addHours(now(), 12)),
    status: "active", bookings_today: 2, revenue_today_cents: 1640000, rating: 4.7,
    updated_at: iso(addHours(now(), -1)),
  },
  {
    id: "l3000000-0000-0000-0000-000000000001",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name, partner_type: P_MANILA.type,
    vehicle_plate: "MLI-2211", size_class: "van", max_weight_kg: 800,
    base_price_cents: 90000, per_km_cents: 1800,
    service_area_label: "NCR + Cavite",
    idle_until: iso(addHours(now(), 3)),
    status: "active", bookings_today: 4, revenue_today_cents: 380000, rating: 4.5,
    updated_at: iso(addHours(now(), -0.3)),
  },
  {
    id: "l3000000-0000-0000-0000-000000000002",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name, partner_type: P_MANILA.type,
    vehicle_plate: "MLI-4483", size_class: "sedan", max_weight_kg: 200,
    base_price_cents: 35000, per_km_cents: 1200,
    service_area_label: "Metro Manila",
    idle_until: iso(addHours(now(), 8)),
    status: "paused", bookings_today: 0, revenue_today_cents: 0, rating: 4.2,
    updated_at: iso(addHours(now(), -0.1)),
  },
  {
    id: "l4000000-0000-0000-0000-000000000001",
    partner_id: P_CEBU.id, partner_display_name: P_CEBU.name, partner_type: P_CEBU.type,
    vehicle_plate: "CEB-9001", size_class: "6wheeler", max_weight_kg: 6000,
    base_price_cents: 450000, per_km_cents: 4200,
    service_area_label: "Cebu island",
    idle_until: iso(addHours(now(), 18)),
    status: "active", bookings_today: 1, revenue_today_cents: 510000, rating: 4.6,
    updated_at: iso(addHours(now(), -3)),
  },
];

const MOCK_BOOKINGS: AdminBooking[] = [
  {
    id: "b1000000-0000-0000-0000-000000000001",
    shipment_id: "s1000000-0000-0000-0000-000000000001",
    awb: "CM-PHL-S0000042X",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name,
    merchant_type: "consumer", merchant_id: null,
    consumer_display: "M. Reyes",
    size_class: "motorcycle", cargo_weight_kg: 12,
    pickup_label: "Makati CBD", dropoff_label: "BGC, Taguig",
    quoted_price_cents: 14500, status: "in_transit",
    pickup_at: iso(addHours(now(), -0.5)), created_at: iso(addHours(now(), -1)),
  },
  {
    id: "b1000000-0000-0000-0000-000000000002",
    shipment_id: "s1000000-0000-0000-0000-000000000002",
    awb: "CM-PHL-E0000099Y",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name,
    merchant_type: "business", merchant_id: "m2000000-0000-0000-0000-000000000001",
    consumer_display: "A. Dela Cruz",
    size_class: "l300", cargo_weight_kg: 820,
    pickup_label: "Pasig Warehouse", dropoff_label: "Laguna Techno Park",
    quoted_price_cents: 285000, status: "pending",
    pickup_at: iso(addHours(now(), 1.5)), created_at: iso(addHours(now(), -0.3)),
  },
  {
    id: "b2000000-0000-0000-0000-000000000001",
    shipment_id: "s2000000-0000-0000-0000-000000000001",
    awb: "CM-PHL-S0000121K",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name,
    merchant_type: "business", merchant_id: "m2000000-0000-0000-0000-000000000002",
    consumer_display: "Sy Lumber Corp.",
    size_class: "10wheeler", cargo_weight_kg: 9400,
    pickup_label: "Valenzuela", dropoff_label: "Tarlac City",
    quoted_price_cents: 1420000, status: "accepted",
    pickup_at: iso(addHours(now(), 4)), created_at: iso(addHours(now(), -2)),
  },
  {
    id: "b3000000-0000-0000-0000-000000000001",
    shipment_id: "s3000000-0000-0000-0000-000000000001",
    awb: "CM-PHL-S0000155M",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name,
    merchant_type: "consumer", merchant_id: null,
    consumer_display: "R. Santos",
    size_class: "van", cargo_weight_kg: 340,
    pickup_label: "Quezon City", dropoff_label: "Antipolo",
    quoted_price_cents: 54000, status: "disputed",
    pickup_at: iso(addHours(now(), -3)), created_at: iso(addHours(now(), -4)),
  },
  {
    id: "b4000000-0000-0000-0000-000000000001",
    shipment_id: "s4000000-0000-0000-0000-000000000001",
    awb: "CM-PHL-S0000188P",
    partner_id: P_CEBU.id, partner_display_name: P_CEBU.name,
    merchant_type: "business", merchant_id: "m2000000-0000-0000-0000-000000000003",
    consumer_display: "Mactan Traders",
    size_class: "6wheeler", cargo_weight_kg: 4800,
    pickup_label: "Mactan Port", dropoff_label: "Cebu IT Park",
    quoted_price_cents: 380000, status: "delivered",
    pickup_at: iso(addHours(now(), -6)), created_at: iso(addHours(now(), -8)),
  },
];

// ── API stubs ─────────────────────────────────────────────────────────────────

const latency = (ms = 220) => new Promise((r) => setTimeout(r, ms));

export async function fetchAllListings(): Promise<AdminListing[]> {
  await latency();
  return structuredClone(MOCK_LISTINGS);
}

// Project a canonical bus row into admin's AdminBooking view. No partner
// filter: scope=tenant sees every partner's row (ADR-0013).
function busToAdminBooking(b: BusBooking): AdminBooking {
  return {
    id:                   b.id,
    shipment_id:          b.shipment_id,
    awb:                  b.awb,
    partner_id:           b.partner_id,
    partner_display_name: b.partner_display_name,
    merchant_type:        b.merchant_type,
    merchant_id:          b.merchant_id,
    consumer_display:     b.merchant_display,     // admin sees unmasked merchant/consumer name
    size_class:           b.size_class,
    cargo_weight_kg:      b.cargo_weight_kg,
    pickup_label:         b.pickup_label,
    dropoff_label:        b.dropoff_label,
    quoted_price_cents:   b.quoted_price_cents,
    status:               b.status,
    pickup_at:            b.pickup_at,
    created_at:           b.created_at,
  };
}

export async function fetchAllBookings(): Promise<AdminBooking[]> {
  await latency();
  const busRows = readBus().map(busToAdminBooking);
  const byId = new Map<string, AdminBooking>();
  for (const b of MOCK_BOOKINGS) byId.set(b.id, b);
  for (const b of busRows)      byId.set(b.id, b);
  return [...byId.values()].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );
}

export { subscribeToBus as subscribeToMarketplaceUpdates };

export async function fetchMarketplaceStats(): Promise<MarketplaceStats> {
  await latency(150);
  const active  = MOCK_LISTINGS.filter((l) => l.status === "active" || l.status === "booked").length;
  const todayGmv = MOCK_LISTINGS.reduce((s, l) => s + l.revenue_today_cents, 0);
  const bookings = MOCK_BOOKINGS.length;
  const partners = new Set(MOCK_LISTINGS.map((l) => l.partner_id)).size;
  return {
    active_listings:        active,
    idle_vehicles_next_6h:  MOCK_LISTINGS.filter((l) =>
      new Date(l.idle_until).getTime() - Date.now() < 6 * 3_600_000 &&
      new Date(l.idle_until).getTime() > Date.now()
    ).length,
    bookings_today:         bookings,
    gmv_today_cents:        todayGmv,
    avg_match_seconds:      42,
    partners_participating: partners,
  };
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

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
