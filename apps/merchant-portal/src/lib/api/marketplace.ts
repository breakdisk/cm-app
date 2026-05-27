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
  findReceiptByBookingId as busFindReceiptByBookingId,
  type BusBooking,
  type BusReceipt,
} from "./marketplace-bus";

export type { BusReceipt } from "./marketplace-bus";

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
  | "scooter_bicycle"
  | "motorcycle"
  | "sedan"
  | "van"
  | "1ton"
  | "3ton"
  | "7ton"
  | "10ton"
  | "trailer"
  | "refrigerated_truck"
  | "recovery_truck";

export type VehicleFeature = "tail_lift" | "chiller" | "freezer";

export type PartnerType = "alliance" | "marketplace";

export interface MerchantListing {
  id:                   string;
  partner_id:           string;
  partner_display_name: string;
  partner_type:         PartnerType;
  vehicle_plate:        string;       // revealed only after booking accepted; masked on preview
  size_class:           SizeClass;
  features:             VehicleFeature[];
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
  features:             VehicleFeature[];
  cargo_weight_kg:      number;
  cargo_description:    string | null;
  pickup_label:         string;
  dropoff_label:        string;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;
  created_at:           string;
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
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
const P_COLDEX   = { id: "a1b2c3d4-0000-0000-0000-000000000005", name: "ColdEx Freight",      type: "marketplace" as PartnerType };

const MOCK_LISTINGS: MerchantListing[] = [
  {
    id: "l1000000-0000-0000-0000-000000000001",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "NKT-••••", size_class: "1ton", max_weight_kg: 1000, max_volume_m3: 6,
    features: ["tail_lift"],
    base_price_cents: 150000, per_km_cents: 2500, per_kg_cents: null,
    service_area_label: "Metro Manila · Luzon",
    idle_until: iso(addHours(now(), 6)),
    status: "active", rating: 4.8, response_window_mins: 15,
  },
  {
    id: "l1000000-0000-0000-0000-000000000002",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "JBX-••••", size_class: "motorcycle", max_weight_kg: 30, max_volume_m3: 0.25,
    features: [],
    base_price_cents: 8000, per_km_cents: 900, per_kg_cents: 1500,
    service_area_label: "Metro Manila only",
    idle_until: iso(addHours(now(), 4)),
    status: "booked", rating: 4.9, response_window_mins: 10,
  },
  {
    id: "l2000000-0000-0000-0000-000000000001",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name, partner_type: P_NORTH.type,
    vehicle_plate: "TLX-••••", size_class: "10ton", max_weight_kg: 10000, max_volume_m3: 40,
    features: ["tail_lift"],
    base_price_cents: 800000, per_km_cents: 5500, per_kg_cents: null,
    service_area_label: "Luzon inter-provincial",
    idle_until: iso(addHours(now(), 12)),
    status: "active", rating: 4.7, response_window_mins: 30,
  },
  {
    id: "l3000000-0000-0000-0000-000000000001",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name, partner_type: P_MANILA.type,
    vehicle_plate: "MLI-••••", size_class: "van", max_weight_kg: 800, max_volume_m3: 5.0,
    features: [],
    base_price_cents: 90000, per_km_cents: 1800, per_kg_cents: null,
    service_area_label: "NCR + Cavite",
    idle_until: iso(addHours(now(), 3)),
    status: "active", rating: 4.5, response_window_mins: 15,
  },
  {
    id: "l4000000-0000-0000-0000-000000000001",
    partner_id: P_CEBU.id, partner_display_name: P_CEBU.name, partner_type: P_CEBU.type,
    vehicle_plate: "CEB-••••", size_class: "7ton", max_weight_kg: 7000, max_volume_m3: 28.0,
    features: [],
    base_price_cents: 450000, per_km_cents: 4200, per_kg_cents: null,
    service_area_label: "Cebu island",
    idle_until: iso(addHours(now(), 18)),
    status: "active", rating: 4.6, response_window_mins: 30,
  },
  {
    id: "l5000000-0000-0000-0000-000000000001",
    partner_id: P_COLDEX.id, partner_display_name: P_COLDEX.name, partner_type: P_COLDEX.type,
    vehicle_plate: "CLX-••••", size_class: "refrigerated_truck", max_weight_kg: 5000, max_volume_m3: 22,
    features: ["freezer"],
    base_price_cents: 350000, per_km_cents: 4800, per_kg_cents: null,
    service_area_label: "Metro Manila · Luzon · Visayas",
    idle_until: iso(addHours(now(), 8)),
    status: "active", rating: 4.9, response_window_mins: 20,
  },
  {
    id: "l5000000-0000-0000-0000-000000000002",
    partner_id: P_COLDEX.id, partner_display_name: P_COLDEX.name, partner_type: P_COLDEX.type,
    vehicle_plate: "CLX-••••", size_class: "refrigerated_truck", max_weight_kg: 3000, max_volume_m3: 14,
    features: ["chiller"],
    base_price_cents: 280000, per_km_cents: 3800, per_kg_cents: null,
    service_area_label: "Metro Manila · NCR",
    idle_until: iso(addHours(now(), 10)),
    status: "active", rating: 4.7, response_window_mins: 20,
  },
  {
    id: "l6000000-0000-0000-0000-000000000001",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name, partner_type: P_NORTH.type,
    vehicle_plate: "TLX-••••", size_class: "trailer", max_weight_kg: 25000, max_volume_m3: null,
    features: [],
    base_price_cents: 1200000, per_km_cents: 6500, per_kg_cents: null,
    service_area_label: "Nationwide · Flatbed available",
    idle_until: iso(addHours(now(), 24)),
    status: "active", rating: 4.8, response_window_mins: 45,
  },
  {
    id: "l7000000-0000-0000-0000-000000000001",
    partner_id: P_FASTSHIP.id, partner_display_name: P_FASTSHIP.name, partner_type: P_FASTSHIP.type,
    vehicle_plate: "RSQ-••••", size_class: "recovery_truck", max_weight_kg: 5000, max_volume_m3: null,
    features: [],
    base_price_cents: 250000, per_km_cents: 3500, per_kg_cents: null,
    service_area_label: "Metro Manila · Luzon",
    idle_until: iso(addHours(now(), 6)),
    status: "active", rating: 4.6, response_window_mins: 20,
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
    size_class: "1ton", features: ["tail_lift"], cargo_weight_kg: 640,
    cargo_description: "General goods — mixed pallets",
    pickup_label: "Pasig Warehouse", dropoff_label: "Batangas Industrial Park",
    quoted_price_cents: 212000, status: "in_transit",
    pickup_at: iso(addHours(now(), -1.2)), created_at: iso(addHours(now(), -3)),
    picked_up_at: iso(addHours(now(), -1.1)),
    picked_up_by: "Driver R. Villanueva", pickup_notes: null,
  },
  {
    id: "b9000000-0000-0000-0000-000000000002",
    listing_id: "l3000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000002",
    awb: "CM-PHL-S0000312R",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "van", features: [], cargo_weight_kg: 280,
    cargo_description: null,
    pickup_label: "Quezon City Store", dropoff_label: "Antipolo Branch",
    quoted_price_cents: 48000, status: "pending",
    pickup_at: iso(addHours(now(), 2)), created_at: iso(addHours(now(), -0.4)),
    picked_up_at: null, picked_up_by: null, pickup_notes: null,
  },
  {
    id: "b9000000-0000-0000-0000-000000000003",
    listing_id: "l2000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000003",
    awb: "CM-PHL-S0000287P",
    partner_id: P_NORTH.id, partner_display_name: P_NORTH.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "10ton", features: ["tail_lift"], cargo_weight_kg: 8400,
    cargo_description: "Steel plates and structural materials",
    pickup_label: "Valenzuela DC", dropoff_label: "La Union Warehouse",
    quoted_price_cents: 1280000, status: "delivered",
    pickup_at: iso(addHours(now(), -22)), created_at: iso(addHours(now(), -26)),
    picked_up_at: iso(addHours(now(), -21.5)),
    picked_up_by: "Driver E. Ocampo", pickup_notes: null,
  },
  {
    id: "b9000000-0000-0000-0000-000000000004",
    listing_id: "l3000000-0000-0000-0000-000000000001",
    shipment_id: "s9000000-0000-0000-0000-000000000004",
    awb: "CM-PHL-S0000296T",
    partner_id: P_MANILA.id, partner_display_name: P_MANILA.name,
    merchant_id: CURRENT_MERCHANT_ID,
    size_class: "van", features: [], cargo_weight_kg: 420,
    cargo_description: null,
    pickup_label: "Makati Office",  dropoff_label: "Alabang Town Center",
    quoted_price_cents: 82000, status: "disputed",
    pickup_at: iso(addHours(now(), -8)), created_at: iso(addHours(now(), -12)),
    picked_up_at: iso(addHours(now(), -7.5)),
    picked_up_by: "Driver P. Mendoza", pickup_notes: null,
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
    size_class:           b.size_class as SizeClass,
    features:             (b.features ?? []) as VehicleFeature[],
    cargo_weight_kg:      b.cargo_weight_kg,
    cargo_description:    b.cargo_description ?? null,
    pickup_label:         b.pickup_label,
    dropoff_label:        b.dropoff_label,
    quoted_price_cents:   b.quoted_price_cents,
    status:               b.status,
    pickup_at:            b.pickup_at,
    created_at:           b.created_at,
    picked_up_at:         b.picked_up_at,
    picked_up_by:         b.picked_up_by,
    pickup_notes:         b.pickup_notes,
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

// Fetch the shipment receipt for a booking, if one has been issued. The merchant
// only reads — the partner (or admin override) issues receipts.
export async function fetchReceiptForBooking(bookingId: string): Promise<BusReceipt | null> {
  return busFindReceiptByBookingId(bookingId);
}

export interface CreateBookingInput {
  listing_id:        string;
  pickup_label:      string;
  dropoff_label:     string;
  cargo_weight_kg:   number;
  pickup_at:         string;     // ISO-8601
  cargo_description: string | null;
  features:          VehicleFeature[];
  // For recovery_truck / trailer: cargo vehicle / heavy-equipment details
  cargo_dims_m?:     { length: number; width: number; height: number } | null;
  cargo_vehicle_kg?: number | null;
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
    features:             input.features.length > 0 ? input.features : listing.features,
    cargo_weight_kg:      input.cargo_weight_kg,
    cargo_description:    input.cargo_description,
    pickup_label:         input.pickup_label,
    dropoff_label:        input.dropoff_label,
    quoted_price_cents:   quoted,
    status:               "pending",
    pickup_at:            input.pickup_at,
    created_at:           iso(now()),
    picked_up_at:         null,
    picked_up_by:         null,
    pickup_notes:         null,
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
    features:             booking.features,
    cargo_description:    booking.cargo_description,
    cargo_weight_kg:      booking.cargo_weight_kg,
    pickup_label:         booking.pickup_label,
    dropoff_label:        booking.dropoff_label,
    quoted_price_cents:   booking.quoted_price_cents,
    status:               "pending",
    pickup_at:            booking.pickup_at,
    created_at:           booking.created_at,
    updated_at:           booking.created_at,
    picked_up_at:         null,
    picked_up_by:         null,
    pickup_notes:         null,
  });

  return booking;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

export const SIZE_CLASS_LABEL: Record<SizeClass, string> = {
  scooter_bicycle:    "Scooter / Bicycle",
  motorcycle:         "Motorcycle",
  sedan:              "Sedan",
  van:                "Van",
  "1ton":             "1 Ton",
  "3ton":             "3 Ton",
  "7ton":             "7 Ton",
  "10ton":            "10 Ton",
  trailer:            "Trailer",
  refrigerated_truck: "Refrigerated Truck",
  recovery_truck:     "Recovery Truck",
};

export const SIZE_CLASS_CAPACITY_HINT: Record<SizeClass, string> = {
  scooter_bicycle:    "Up to 20 kg · 0.1 m³",
  motorcycle:         "Up to 30 kg · 0.25 m³",
  sedan:              "Up to 200 kg · 1.2 m³",
  van:                "Up to 800 kg · 5 m³",
  "1ton":             "Up to 1,000 kg · 6 m³",
  "3ton":             "Up to 3,000 kg · 14 m³",
  "7ton":             "Up to 7,000 kg · 28 m³",
  "10ton":            "Up to 10,000 kg · 40 m³",
  trailer:            "Up to 25,000 kg · 80 m³ · Flatbed available",
  refrigerated_truck: "Up to 5,000 kg · 22 m³ · Chiller / Freezer",
  recovery_truck:     "Vehicle towing · capacity by cargo vehicle quote",
};

export const VEHICLE_FEATURE_LABEL: Record<VehicleFeature, string> = {
  tail_lift: "Tail-lift",
  chiller:   "Chiller (0–8 °C)",
  freezer:   "Freezer (−18 °C)",
};

// ── AI Cargo Smart-Assignment ─────────────────────────────────────────────────

export interface CargoSuggestion {
  size_class:  SizeClass;
  features:    VehicleFeature[];
  reason:      string;
  needs_dims:  boolean;   // trailer / recovery_truck — ask for L × W × H + kg
  is_vehicle:  boolean;   // cargo is a vehicle — show vehicle-dims form
}

/**
 * Rule-based cargo analyser. Parses the customer's free-text cargo description
 * and returns the recommended size class, required features, and UX guidance.
 * Returns null when the description contains insufficient signal.
 *
 * Production path: replace with a Claude API call (streaming, with JSON tool-use
 * for structured output). Rules here serve as the offline fallback.
 */
export function detectCargoRequirements(description: string): CargoSuggestion | null {
  const d = description.toLowerCase();
  if (!d.trim()) return null;

  // ── Vehicle as cargo → Recovery Truck ──────────────────────────────────────
  const vehicleCargoPattern =
    /\b(car|sedan|suv|pickup truck|delivery van|bus|motorbike|motorcycle as cargo|automobile|auto for towing|tow|broken.?down vehicle|non.?running|disabled vehicle)\b/;
  if (vehicleCargoPattern.test(d)) {
    return {
      size_class:  "recovery_truck",
      features:    [],
      reason:      "Vehicle detected as cargo — please provide the cargo vehicle's dimensions (L × W × H) and weight so we can size the right Recovery Truck.",
      needs_dims:  true,
      is_vehicle:  true,
    };
  }

  // ── Frozen / meat → Refrigerated Truck + Freezer ───────────────────────────
  const frozenPattern =
    /\b(meat|beef|pork|chicken|lamb|goat|seafood|fish|shrimp|prawn|frozen|ice cream|gelato|cold chain|freezer|cryogenic)\b/;
  if (frozenPattern.test(d)) {
    return {
      size_class: "refrigerated_truck",
      features:   ["freezer"],
      reason:     "Frozen or meat cargo detected — a Refrigerated Truck with Freezer (−18 °C) is required to maintain cold chain integrity.",
      needs_dims: false,
      is_vehicle: false,
    };
  }

  // ── Chilled / fresh produce / dairy → Refrigerated Truck + Chiller ─────────
  const chilledPattern =
    /\b(dairy|milk|cheese|yogurt|butter|cream|fresh produce|vegetables?|fruits?|salad|flowers?|medicine|pharmaceuticals?|chilled|refrigerat|cold storage)\b/;
  if (chilledPattern.test(d)) {
    return {
      size_class: "refrigerated_truck",
      features:   ["chiller"],
      reason:     "Chilled goods detected — a Refrigerated Truck with Chiller (0–8 °C) is recommended to preserve freshness.",
      needs_dims: false,
      is_vehicle: false,
    };
  }

  // ── Heavy / irregular equipment → Trailer Flatbed ──────────────────────────
  const heavyPattern =
    /\b(machinery|heavy equipment|excavator|bulldozer|crane|backhoe|generator|transformer|boiler|vessel|tank|oversize|oversized|irregular|flatbed|structural steel|i.?beam|steel plate|precast|concrete slab)\b/;
  if (heavyPattern.test(d)) {
    return {
      size_class: "trailer",
      features:   [],
      reason:     "Heavy or oversized cargo detected — a Trailer (Flatbed) is recommended. Please provide cargo dimensions (L × W × H) and total weight for an accurate quote.",
      needs_dims: true,
      is_vehicle: false,
    };
  }

  return null;
}

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
