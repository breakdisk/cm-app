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
 *
 * Cross-portal propagation (pre-backend): merchant-portal publishes new
 * bookings to the shared marketplace-bus; this module merges them into
 * `fetchBookings()` filtered to this partner's scope, and writes accept/reject
 * back through the bus. Production replaces the bus with Kafka events
 * (ADR-0013 §Booking flow).
 */

import {
  readBus,
  updateBookingStatus as busUpdateStatus,
  markPickedUp as busMarkPickedUp,
  subscribeToBus,
  appendReceipt as busAppendReceipt,
  findReceiptByBookingId as busFindReceiptByBookingId,
  type BusBooking,
  type BusReceipt,
} from "./marketplace-bus";
import { getCurrentPartner, getCurrentPartnerId } from "./partner-identity";

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
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
}

// ── Mock data ─────────────────────────────────────────────────────────────────

const TENANT_ID  = "00000000-0000-0000-0000-000000000001";
// PARTNER_ID is resolved per-call via getCurrentPartnerId() so the demo can
// flip which carrier this session is "acting as". When backend auth lands,
// this becomes `claims.pid` from the JWT (ADR-0013 §Auth).

const now = () => new Date();
const iso = (d: Date) => d.toISOString();
const addHours = (d: Date, h: number) => new Date(d.getTime() + h * 3_600_000);

const MOCK_LISTINGS: VehicleListing[] = [
  {
    id:                           "l1000000-0000-0000-0000-000000000001",
    tenant_id:                    TENANT_ID,
    partner_id:                   "__current__", // re-tagged on fetch
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
    partner_id:                   "__current__", // re-tagged on fetch
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
    partner_id:                   "__current__", // re-tagged on fetch
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
    partner_id:                   "__current__", // re-tagged on fetch
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
    partner_id:                   "__current__", // re-tagged on fetch
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
    picked_up_at:       iso(addHours(now(), -0.4)),
    picked_up_by:       "Driver J. Santos",
    pickup_notes:       null,
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
    picked_up_at:       null,
    picked_up_by:       null,
    pickup_notes:       null,
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
    picked_up_at:       null,
    picked_up_by:       null,
    pickup_notes:       null,
  },
];

// ── API stubs (swap to authFetch when backend lands) ──────────────────────────

const latency = (ms = 220) => new Promise((r) => setTimeout(r, ms));

export async function fetchListings(): Promise<VehicleListing[]> {
  await latency();
  const pid = getCurrentPartnerId();
  return MOCK_LISTINGS.map((l) => ({ ...l, partner_id: pid }));
}

// Project a canonical bus row into this partner's MarketplaceBooking view.
// RLS-equivalent: filter to rows owned by this partner (ADR-0013 §RLS extension:
// scope=partner sees only own partner_id rows).
function busToPartnerBooking(b: BusBooking): MarketplaceBooking | null {
  if (b.partner_id !== getCurrentPartnerId()) return null;
  // Pre-accept: mask consumer PII (§"Consumer PII leaks" risk R12).
  const masked =
    b.status === "pending" || b.status === "rejected"
      ? b.consumer_display
      : b.merchant_display;
  return {
    id:                 b.id,
    listing_id:         b.listing_id,
    shipment_id:        b.shipment_id,
    awb:                b.awb,
    consumer_name:      masked,
    consumer_phone:     b.status === "accepted" || b.status === "in_transit" || b.status === "delivered"
                          ? "+63 9•• ••• ••••"    // placeholder; real field decrypted on accept
                          : null,
    pickup_label:       b.pickup_label,
    dropoff_label:      b.dropoff_label,
    cargo_weight_kg:    b.cargo_weight_kg,
    cargo_volume_m3:    null,
    quoted_price_cents: b.quoted_price_cents,
    status:             b.status,
    pickup_at:          b.pickup_at,
    created_at:         b.created_at,
    picked_up_at:       b.picked_up_at,
    picked_up_by:       b.picked_up_by,
    pickup_notes:       b.pickup_notes,
  };
}

export async function fetchBookings(): Promise<MarketplaceBooking[]> {
  await latency();
  const busRows = readBus()
    .map(busToPartnerBooking)
    .filter((b): b is MarketplaceBooking => b !== null);
  const byId = new Map<string, MarketplaceBooking>();
  for (const b of MOCK_BOOKINGS) byId.set(b.id, b);
  for (const b of busRows)      byId.set(b.id, b);
  return [...byId.values()].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );
}

/**
 * Carrier (partner) accepts a pending booking (ADR-0013 §Booking flow:
 * "accept → booking.status='accepted', shipment enters dispatch flow").
 * In production, emits `marketplace.booking_accepted` via Kafka outbox;
 * dispatch service enqueues the task; driver-ops pushes to driver-app.
 * Pre-backend: we only flip the bus status — dispatch-queue stand-in is in
 * admin-portal.
 */
export async function acceptBooking(id: string): Promise<MarketplaceBooking | null> {
  await latency(180);
  const updated = busUpdateStatus(id, "accepted");
  return updated ? busToPartnerBooking(updated) : null;
}

export async function rejectBooking(id: string): Promise<MarketplaceBooking | null> {
  await latency(180);
  const updated = busUpdateStatus(id, "rejected");
  return updated ? busToPartnerBooking(updated) : null;
}

export interface RecordPickupInput {
  booking_id:   string;
  picked_up_by?: string | null;
  pickup_notes?: string | null;
}

/**
 * Carrier records cargo collected from pickup point (ADR-0013 §Booking flow:
 * "pickup → booking.status='in_transit', tracking channel activates").
 * In production this emits `shipment.picked_up` on Kafka; the engagement engine
 * triggers the customer "driver has your package" notification and the
 * tracking page starts streaming ETA updates. Pre-backend: flip bus status +
 * stamp metadata, partner scope enforced below.
 */
export async function recordPickup(input: RecordPickupInput): Promise<MarketplaceBooking | null> {
  await latency(180);
  const booking = readBus().find((b) => b.id === input.booking_id);
  if (!booking) return null;
  if (booking.partner_id !== getCurrentPartnerId()) {
    // RLS-equivalent: a partner can only transition their own bookings.
    throw new Error("Booking does not belong to the acting partner");
  }
  const updated = busMarkPickedUp(input.booking_id, {
    picked_up_by: input.picked_up_by ?? null,
    pickup_notes: input.pickup_notes ?? null,
  });
  return updated ? busToPartnerBooking(updated) : null;
}

export { subscribeToBus as subscribeToMarketplaceUpdates };

// ── Receipts ─────────────────────────────────────────────────────────────────

export interface IssueReceiptInput {
  booking_id: string;
  signed_by?: string | null;
  notes?:     string | null;
}

/**
 * Carrier issues the shipment receipt for a booking (ADR-0013 §Booking flow:
 * receipt is the handover artifact to the consumer after pickup). In production
 * this triggers `shipment.receipt_issued` on Kafka; the engagement engine
 * forwards the receipt to the consumer via their preferred channel. Pre-backend
 * we write a canonical `BusReceipt` row keyed on booking_id so every portal
 * renders the same artifact.
 */
export async function issueReceipt(input: IssueReceiptInput): Promise<BusReceipt | null> {
  await latency(180);
  const booking = readBus().find((b) => b.id === input.booking_id);
  if (!booking) return null;
  const partner = getCurrentPartner();
  if (booking.partner_id !== partner.id) {
    // RLS-equivalent: a partner can only issue receipts for their own bookings.
    throw new Error("Booking does not belong to the acting partner");
  }
  const existing = busFindReceiptByBookingId(input.booking_id);
  if (existing) return existing;

  const issuedAt = new Date();
  const yyyy = issuedAt.getUTCFullYear();
  const mm   = String(issuedAt.getUTCMonth() + 1).padStart(2, "0");
  const dd   = String(issuedAt.getUTCDate()).padStart(2, "0");
  const seq  = String(issuedAt.getTime()).slice(-4);
  const receipt: BusReceipt = {
    id:                   `r9000000-0000-0000-0000-${issuedAt.getTime().toString().padStart(12, "0")}`,
    receipt_no:           `R-${yyyy}${mm}${dd}-${seq}`,
    booking_id:           booking.id,
    awb:                  booking.awb,
    shipment_id:          booking.shipment_id,
    partner_id:           booking.partner_id,
    partner_display_name: booking.partner_display_name,
    merchant_id:          booking.merchant_id,
    merchant_display:     booking.merchant_display,
    consumer_display:     booking.consumer_display,
    pickup_label:         booking.pickup_label,
    dropoff_label:        booking.dropoff_label,
    pickup_at:            booking.pickup_at,
    size_class:           booking.size_class,
    cargo_weight_kg:      booking.cargo_weight_kg,
    quoted_price_cents:   booking.quoted_price_cents,
    issued_by:            "partner",
    issued_by_name:       partner.name,
    signed_by:            input.signed_by ?? null,
    notes:                input.notes ?? null,
    issued_at:            issuedAt.toISOString(),
  };
  busAppendReceipt(receipt);
  return receipt;
}

export async function fetchReceiptForBooking(bookingId: string): Promise<BusReceipt | null> {
  return busFindReceiptByBookingId(bookingId);
}

export async function createListing(
  input: Omit<VehicleListing, "id" | "tenant_id" | "partner_id" | "vehicle_id" | "bookings_today" | "revenue_today_cents" | "created_at" | "updated_at">,
): Promise<VehicleListing> {
  await latency();
  const listing: VehicleListing = {
    ...input,
    id:                  `l1000000-0000-0000-0000-${Date.now().toString().padStart(12, "0")}`,
    tenant_id:           TENANT_ID,
    partner_id:          getCurrentPartnerId(),
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
