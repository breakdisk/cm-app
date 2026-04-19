/**
 * Merchant Portal — Marketplace Discovery API.
 *
 * Merchants (the `business` or `consumer` merchant_type per ADR-0013) *consume*
 * marketplace vehicle listings: they browse idle capacity published by alliance
 * and marketplace partners and create bookings. A booking creates a shipment
 * via order-intake (zero-loss invariant: no shipment bypass, even for
 * marketplace-origin flows).
 *
 * Shape mirrors the partner/admin marketplace APIs but is scoped to the
 * merchant's view:
 *   - `listings`: cross-partner, status=active only (merchants don't see paused
 *     or expired inventory)
 *   - `bookings`: filtered to the current merchant's own bookings
 *
 * Pre-backend stub. Swap to `authFetch` when the service ships.
 *
 * Pre-backend propagation: `createBooking` also writes to the marketplace-bus
 * (shared localStorage) so partner-portal and admin-portal reflect the new
 * booking on next refresh. In production this is replaced by the real service
 * emitting `marketplace.booking_created` on Kafka (ADR-0013 §Booking flow).
 */

import {
  appendBooking as busAppend,
  readBus,
  subscribeToBus,
  type BusBooking,
} from "./marketplace-bus";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ListingStatus = "active" | "booked";
export type BookingStatus =
  | "pending"
  | "accepted"
  | "rejected"
  | "in_transit"
  | "delivered"
  | "cancelled"
  | "disputed";

export type SizeClass =
  | "motorcycle"
  | "sedan"
  | "van"
  | "l300"
  | "6wheeler"
  | "10wheeler"
  | "trailer";

export type PartnerType = "alliance" | "marketplace";

export interface MerchantListing {
  id:                   string;
  partner_id:           string;
  partner_display_name: string;
  partner_type:         PartnerType;
  vehicle_plate:        string;       // revealed only after booking accepted; masked on preview
  size_class:           SizeClass;
  max_weight_kg:        number;
  max_volume_m3:        number | null;
  base_price_cents:     number;
  per_km_cents:         number;
  per_kg_cents:         number | null;
  service_area_label:   string;
  idle_until:           string;
  status:               ListingStatus;
  rating:               number;        // 0..5
  response_window_mins: number;
}

export interface MerchantBooking {
  id:                   string;
  listing_id:           string;
  shipment_id:          string;        // FK to shipments; drives tracking page & order detail link
  awb:                  string;
  partner_id:           string;
  partner_display_name: string;
  merchant_id:          string;        // ADR-0013: business/consumer merchants both carry merchant_id
  size_class:           SizeClass;
  cargo_weight_kg:      number;
  pickup_label:         string;
  dropoff_label:        string;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;
  created_at:           string;
}

// Pre-backend: this portal represents a single merchant session. In production
// the merchant_id comes from the JWT `mid` claim (ADR-0013 §JWT claims).
export const CURRENT_MERCHANT_ID   = "m2000000-0000-0000-0000-000000000001";
export const CURRENT_MERCHANT_NAME = "Acme E-commerce";

export interface MerchantMarketplaceStats {
  available_now:       number;
  avg_rate_per_km:     number;         // cents, weighted across active listings
  partners_reachable:  number;
  my_bookings_active:  number;
}

// ── Mock data ─────────────────────────────────────────────────────────────────

const iso = (d: Date) => d.toISOString();
const addHours = (d: Date, h: number) => new Date(d.getTime() + h * 3_600_000);
const now = () => new Date();

const P_FASTSHIP = { id: "a1b2c3d4-0000-0000-0000-000000000001", name: "FastShip Co.",        type: "alliance"    as PartnerType };
const P_NORTH    = { id: "a1b2c3d4-0000-0000-0000-000000000002", name: "NorthLink Logistics", type: "alliance"    as PartnerType };
const P_MANILA   = { id: "a1b2c3d4-0000-0000-0000-000000000003", name: "Manila MoveIt",       type: "marketplace" as PartnerType };
const P_CEBU     = { id: "a1b2c3d4-0000-0000-0000-000000000004", name: "Cebu Carriers Co-op", type: "marketplace" as PartnerType };

const MOCK_LISTINGS: MerchantListing[] = [
  {
    id: "l1000000-0000-0000-0000-000000000001",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "NKT-••••", size_class: "l300", max_weight_kg: 1500, max_volume_m3: 8.5,
    base_price_cents: 150000, per_km_cents: 2500, per_kg_cents: null,
    service_area_label: "Metro Manila · Luzon",
    idle_until: iso(addHours(now(), 6)),
    status: "active", rating: 4.8, response_window_mins: 15,
  },
  {
    id: "l1000000-0000-0000-0000-000000000002",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "JBX-••••", size_class: "motorcycle", max_weight_kg: 30, max_volume_m3: 0.25,
    base_price_cents: 8000, per_km_cents: 900, per_kg_cents: 1500,
    service_area_label: "Metro Manila only",
    idle_until: iso(addHours(now(), 4)),
    status: "booked", rating: 4.9, response_window_mins: 10,
  },
  {
    id: "l2000000-0000-0000-0000-000000000001",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name, partner_type: P_NORTH.type,
    vehicle_plate: "TLX-••••", size_class: "10wheeler", max_weight_kg: 12000, max_volume_m3: 40,
    base_price_cents: 800000, per_km_cents: 5500, per_kg_cents: null,
    service_area_label: "Luzon inter-provincial",
    idle_until: iso(addHours(now(), 12)),
    status: "active", rating: 4.7, response_window_mins: 30,
  },
  {
    id: "l3000000-0000-0000-0000-000000000001",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name, partner_type: P_MANILA.type,
    vehicle_plate: "MLI-••••", size_class: "van", max_weight_kg: 800, max_volume_m3: 5.0,
    base_price_cents: 90000, per_km_cents: 1800, per_kg_cents: null,
    service_area_label: "NCR + Cavite",
    idle_until: iso(addHours(now(), 3)),
    status: "active", rating: 4.5, response_window_mins: 15,
  },
  {
    id: "l4000000-0000-0000-0000-000000000001",
    partner_id: P_CEBU.id, partner_display_name: P_CEBU.name, partner_type: P_CEBU.type,
    vehicle_plate: "CEB-••••", size_class: "6wheeler", max_weight_kg: 6000, max_volume_m3: 22.0,
    base_price_cents: 450000, per_km_cents: 4200, per_kg_cents: null,
    service_area_label: "Cebu island",
    idle_until: iso(addHours(now(), 18)),
    status: "active", rating: 4.6, response_window_mins: 30,
  },
];

const MOCK_BOOKINGS: MerchantBooking[] = [
  {
    id: "b9000000-0000-0000-0000-000000000001",
    listing_id: "l1000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000001",
    awb: "CM-PHL-S0000301Q",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "l300", cargo_weight_kg: 640,
    pickup_label: "Pasig Warehouse", dropoff_label: "Batangas Industrial Park",
    quoted_price_cents: 212000, status: "in_transit",
    pickup_at: iso(addHours(now(), -1.2)), created_at: iso(addHours(now(), -3)),
  },
  {
    id: "b9000000-0000-0000-0000-000000000002",
    listing_id: "l3000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000002",
    awb: "CM-PHL-S0000312R",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "van", cargo_weight_kg: 280,
    pickup_label: "Quezon City Store", dropoff_label: "Antipolo Branch",
    quoted_price_cents: 48000, status: "pending",
    pickup_at: iso(addHours(now(), 2)), created_at: iso(addHours(now(), -0.4)),
  },
  {
    id: "b9000000-0000-0000-0000-000000000003",
    listing_id: "l2000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000003",
    awb: "CM-PHL-S0000287P",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "10wheeler", cargo_weight_kg: 8400,
    pickup_label: "Valenzuela DC", dropoff_label: "La Union Warehouse",
    quoted_price_cents: 1280000, status: "delivered",
    pickup_at: iso(addHours(now(), -22)), created_at: iso(addHours(now(), -26)),
  },
  {
    id: "b9000000-0000-0000-0000-000000000004",
    listing_id: "l3000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000004",
    awb: "CM-PHL-S0000296T",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "van", cargo_weight_kg: 420,
    pickup_label: "Makati Office",  dropoff_label: "Alabang Town Center",
    quoted_price_cents: 82000, status: "disputed",
    pickup_at: iso(addHours(now(), -8)), created_at: iso(addHours(now(), -12)),
  },
];

// Project a BusBooking (canonical cross-portal shape) into this portal's view.
// Filters to the current merchant's own rows — RLS-equivalent for scope=merchant
// (ADR-0013 §RLS extension: merchant scope sees only own merchant_id rows).
function busToMerchantBooking(b: BusBooking): MerchantBooking | null {
  if (b.merchant_id !== CURRENT_MERCHANT_ID) return null;
  return {
    id:                   b.id,
    listing_id:           b.listing_id,
    shipment_id:          b.shipment_id,
    awb:                  b.awb,
    partner_id:           b.partner_id,
    partner_display_name: b.partner_display_name,
    merchant_id:          b.merchant_id,
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

// ── API stubs ─────────────────────────────────────────────────────────────────

const latency = (ms = 220) => new Promise((r) => setTimeout(r, ms));

export async function fetchAvailableListings(): Promise<MerchantListing[]> {
  await latency();
  return structuredClone(MOCK_LISTINGS);
}

export async function fetchMyBookings(): Promise<MerchantBooking[]> {
  await latency();
  // Merge seeded mocks with bus-originated bookings; dedupe by id (bus wins).
  const busRows = readBus()
    .map(busToMerchantBooking)
    .filter((b): b is MerchantBooking => b !== null);
  const byId = new Map<string, MerchantBooking>();
  for (const b of MOCK_BOOKINGS) byId.set(b.id, b);
  for (const b of busRows)      byId.set(b.id, b);  // bus overrides mock (status updates from partner accept/reject)
  return [...byId.values()].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );
}

export async function fetchMarketplaceStats(): Promise<MerchantMarketplaceStats> {
  await latency(150);
  const active = MOCK_LISTINGS.filter((l) => l.status === "active");
  const avgRate = active.length === 0
    ? 0
    : Math.round(active.reduce((s, l) => s + l.per_km_cents, 0) / active.length);
  // Active-bookings count must include bus-originated rows (new merchant bookings
  // that haven't been propagated back into the mock seed).
  const merged = await fetchMyBookings();
  const activeBookings = merged.filter(
    (b) => b.status === "pending" || b.status === "accepted" || b.status === "in_transit"
  ).length;
  return {
    available_now:      active.length,
    avg_rate_per_km:    avgRate,
    partners_reachable: new Set(MOCK_LISTINGS.map((l) => l.partner_id)).size,
    my_bookings_active: activeBookings,
  };
}

// Re-export the bus subscriber so page code can live-refresh on cross-portal
// updates (partner accepts → merchant sees status flip) without another import.
export { subscribeToBus as subscribeToMarketplaceUpdates } from "./marketplace-bus";

export interface CreateBookingInput {
  listing_id:      string;
  pickup_label:    string;
  dropoff_label:   string;
  cargo_weight_kg: number;
  pickup_at:       string;     // ISO-8601
}

// Booking creates a shipment via order-intake; zero-loss invariant preserved
// (ADR-0013 §Booking flow). Stub synthesizes the AWB the way the real
// CM-{TTT}-{S}{NNNNNNN}{C} generator will — partner_id/merchant_id are
// denormalized onto the booking row for RLS, same as the real schema.
// Also publishes to the marketplace-bus so partner-portal and admin-portal
// see the new row on next refresh (stand-in for `marketplace.booking_created`
// on Kafka — ADR-0013 §Booking flow).
export async function createBooking(input: CreateBookingInput): Promise<MerchantBooking> {
  await latency(320);
  const listing = MOCK_LISTINGS.find((l) => l.id === input.listing_id);
  if (!listing) throw new Error(`Listing not found: ${input.listing_id}`);
  const quoted = listing.base_price_cents + listing.per_km_cents * 10;  // rough stub quote
  const stamp  = Date.now();
  const booking: MerchantBooking = {
    id:                   `b9000000-0000-0000-0000-${stamp.toString().padStart(12, "0")}`,
    listing_id:           listing.id,
    shipment_id:          `s9000000-0000-0000-0000-${stamp.toString().padStart(12, "0")}`,
    awb:                  `CM-PHL-S${String(stamp).slice(-7)}Z`,
    partner_id:           listing.partner_id,
    partner_display_name: listing.partner_display_name,
    merchant_id:          CURRENT_MERCHANT_ID,
    size_class:           listing.size_class,
    cargo_weight_kg:      input.cargo_weight_kg,
    pickup_label:         input.pickup_label,
    dropoff_label:        input.dropoff_label,
    quoted_price_cents:   quoted,
    status:               "pending",
    pickup_at:            input.pickup_at,
    created_at:           iso(now()),
  };
  MOCK_BOOKINGS.unshift(booking);

  // Publish to cross-portal bus (canonical superset shape).
  busAppend({
    id:                   booking.id,
    listing_id:           booking.listing_id,
    shipment_id:          booking.shipment_id,
    awb:                  booking.awb,
    partner_id:           booking.partner_id,
    partner_display_name: booking.partner_display_name,
    merchant_id:          CURRENT_MERCHANT_ID,
    merchant_type:        "business",
    merchant_display:     CURRENT_MERCHANT_NAME,
    consumer_display:     CURRENT_MERCHANT_NAME,    // business booking — no masking needed
    size_class:           booking.size_class,
    cargo_weight_kg:      booking.cargo_weight_kg,
    pickup_label:         booking.pickup_label,
    dropoff_label:        booking.dropoff_label,
    quoted_price_cents:   booking.quoted_price_cents,
    status:               "pending",
    pickup_at:            booking.pickup_at,
    created_at:           booking.created_at,
    updated_at:           booking.created_at,
  });

  return booking;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

export const SIZE_CLASS_LABEL: Record<SizeClass, string> = {
  motorcycle:  "Motorcycle",
  sedan:       "Sedan",
  van:         "Van",
  l300:        "L300 / Pickup",
  "6wheeler":  "6-Wheeler",
  "10wheeler": "10-Wheeler",
  trailer:     "Trailer",
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
