/**
 * Merchant Portal — Marketplace API (carrier service, via the gateway).
 *
 * Merchants consume marketplace vehicle listings: they browse idle capacity
 * published by carriers and book it for one of their own shipments. Every
 * route here requires `marketplace:book`. Types mirror the carrier service's
 * serde output (`services/carrier/src/domain/entities/mod.rs`) exactly.
 *
 * Money is integer cents; weights/volumes/distances are f32 on the wire.
 */

import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

// ── Types ─────────────────────────────────────────────────────────────────────

export type SizeClass =
  | "motorcycle"
  | "sedan"
  | "van"
  | "l300"
  | "6wheeler"
  | "10wheeler"
  | "trailer";

export type ListingStatus = "active" | "paused" | "booked" | "expired";

export type BookingStatus =
  | "pending"
  | "accepted"
  | "rejected"
  | "in_transit"
  | "delivered"
  | "cancelled"
  | "disputed";

export type BookingPaymentMethod = "invoice" | "online";
export type BookingPaymentStatus = "pending" | "authorized" | "captured" | "voided" | "failed";

export interface VehicleListing {
  id:                           string;
  tenant_id:                    string;
  carrier_id:                   string;
  vehicle_plate:                string;
  size_class:                   SizeClass;
  max_weight_kg:                number;
  max_volume_m3:                number | null;
  base_price_cents:             number;
  per_km_cents:                 number;
  per_kg_cents:                 number | null;
  service_area_label:           string;
  idle_from:                    string;
  idle_until:                   string;
  status:                       ListingStatus;
  carrier_response_window_mins: number;
  bookings_today:               number;
  revenue_today_cents:          number;
  created_at:                   string;
  updated_at:                   string;
}

export interface MarketplaceBooking {
  id:                    string;
  tenant_id:             string;
  listing_id:            string;
  carrier_id:            string;
  shipment_id:           string;
  awb:                   string;
  consumer_name:         string;
  consumer_phone:        string | null;
  pickup_label:          string;
  dropoff_label:         string;
  cargo_weight_kg:       number;
  cargo_volume_m3:       number | null;
  quoted_price_cents:    number;
  status:                BookingStatus;
  pickup_at:             string;
  picked_up_at:          string | null;
  picked_up_by:          string | null;
  pickup_notes:          string | null;
  booked_by_user_id:     string | null;
  payment_method:        BookingPaymentMethod;
  payment_status:        BookingPaymentStatus;
  payment_intent_id:     string | null;
  payment_checkout_url:  string | null;
  payment_authorized_at: string | null;
  response_window_mins:  number;
  distance_km:           number;
  created_at:            string;
  updated_at:            string;
}

export interface MerchantMarketplaceStats {
  available_now:      number;
  avg_rate_per_km:    number;   // cents, mean across active listings
  carriers_reachable: number;
  my_bookings_active: number;
}

export interface CreateBookingInput {
  listing_id:       string;
  shipment_id:      string;
  awb:              string;
  consumer_name:    string;
  consumer_phone?:  string;
  pickup_label:     string;
  dropoff_label:    string;
  cargo_weight_kg:  number;
  cargo_volume_m3?: number;
  distance_km:      number;
  pickup_at:        string;   // RFC 3339
  payment_method:   BookingPaymentMethod;
}

export interface CreateBookingResult {
  booking:      MarketplaceBooking;
  checkout_url: string | null;
}

/** A shipment the merchant can book a vehicle for (the fields the picker needs). */
export interface BookableShipment {
  id:             string;
  awb:            string;
  status:         string;
  customer_name:  string;
  customer_phone: string;
  pickup_label:   string;
  dropoff_label:  string;
  weight_kg:      number | null;
}

// ── Errors ────────────────────────────────────────────────────────────────────

/** Carries the server's message for 4xx responses (BusinessRule texts are user-facing). */
export class MarketplaceApiError extends Error {
  readonly status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = "MarketplaceApiError";
    this.status = status;
  }
}

async function expectOk(res: Response, what: string): Promise<void> {
  if (res.ok) return;
  let detail = "";
  try {
    const body = (await res.json()) as { error?: { message?: string } };
    detail = body?.error?.message ?? "";
  } catch {
    // Non-JSON body (proxy error page, empty 502): fall through to the generic text.
  }
  // 4xx messages are written for the user; a 5xx is not worth quoting.
  const message = res.status >= 400 && res.status < 500 && detail
    ? detail
    : `${what} failed (HTTP ${res.status}). Please try again.`;
  throw new MarketplaceApiError(message, res.status);
}

const JSON_HEADERS = { "Content-Type": "application/json" };

// ── API ───────────────────────────────────────────────────────────────────────

export async function fetchAvailableListings(
  opts: { min_weight_kg?: number; size_class?: SizeClass; limit?: number } = {},
): Promise<VehicleListing[]> {
  const qs = new URLSearchParams();
  if (opts.min_weight_kg !== undefined) qs.set("min_weight_kg", String(opts.min_weight_kg));
  if (opts.size_class)                  qs.set("size_class", opts.size_class);
  if (opts.limit !== undefined)         qs.set("limit", String(opts.limit));
  const q = qs.toString();
  const res = await authFetch(`${API_BASE}/v1/marketplace/available${q ? `?${q}` : ""}`);
  await expectOk(res, "Loading listings");
  const body = (await res.json()) as { listings?: VehicleListing[] };
  return body.listings ?? [];
}

export async function fetchMyBookings(limit = 50, offset = 0): Promise<MarketplaceBooking[]> {
  const res = await authFetch(
    `${API_BASE}/v1/marketplace/my-bookings?limit=${limit}&offset=${offset}`,
  );
  await expectOk(res, "Loading bookings");
  const body = (await res.json()) as { bookings?: MarketplaceBooking[] };
  return body.bookings ?? [];
}

export async function createBooking(input: CreateBookingInput): Promise<CreateBookingResult> {
  const res = await authFetch(`${API_BASE}/v1/marketplace/bookings`, {
    method:  "POST",
    headers: JSON_HEADERS,
    body:    JSON.stringify(input),
  });
  await expectOk(res, "Booking");
  return (await res.json()) as CreateBookingResult;
}

/** Only `pending` bookings can be cancelled; the server enforces it. */
export async function cancelBooking(bookingId: string): Promise<MarketplaceBooking> {
  const res = await authFetch(
    `${API_BASE}/v1/marketplace/bookings/${encodeURIComponent(bookingId)}/cancel`,
    { method: "POST" },
  );
  await expectOk(res, "Cancelling");
  const body = (await res.json()) as { booking: MarketplaceBooking };
  return body.booking;
}

/** Stats are derived from the listings and bookings already fetched; no extra route. */
export function computeMarketplaceStats(
  listings: VehicleListing[],
  bookings: MarketplaceBooking[],
): MerchantMarketplaceStats {
  const active = listings.filter((l) => l.status === "active");
  const avgRate = active.length === 0
    ? 0
    : Math.round(active.reduce((s, l) => s + l.per_km_cents, 0) / active.length);
  return {
    available_now:      active.length,
    avg_rate_per_km:    avgRate,
    carriers_reachable: new Set(active.map((l) => l.carrier_id)).size,
    my_bookings_active: bookings.filter(
      (b) => b.status === "pending" || b.status === "accepted" || b.status === "in_transit",
    ).length,
  };
}

// ── The merchant's own shipments (for the booking picker) ─────────────────────

/** Terminal states: a vehicle cannot usefully be booked for these. */
const NOT_BOOKABLE = new Set(["delivered", "cancelled", "returned", "failed"]);

interface RawAddress {
  line1?: string;
  city?: string;
  province?: string;
}

function addressLabel(a: RawAddress | string | null | undefined): string {
  if (!a) return "";
  if (typeof a === "string") return a;
  return [a.line1, a.city, a.province].filter(Boolean).join(", ");
}

/**
 * The merchant's shipments, from the same `GET /v1/shipments` the Shipments page
 * reads. The AWB is `awb` (older rows: `tracking_number`); weight is grams.
 */
export async function fetchBookableShipments(): Promise<BookableShipment[]> {
  const res = await authFetch(`${API_BASE}/v1/shipments?per_page=100`);
  await expectOk(res, "Loading shipments");
  const body = (await res.json()) as {
    shipments?: Array<{
      id:               string;
      awb?:             string;
      tracking_number?: string;
      status:           string;
      customer_name?:   string;
      customer_phone?:  string;
      origin?:          RawAddress | string;
      destination?:     RawAddress | string;
      weight?:          { grams?: number } | null;
    }>;
  };
  return (body.shipments ?? [])
    .filter((s) => !NOT_BOOKABLE.has(s.status))
    .map((s) => ({
      id:             s.id,
      awb:            s.awb ?? s.tracking_number ?? "",
      status:         s.status,
      customer_name:  s.customer_name ?? "",
      customer_phone: s.customer_phone ?? "",
      pickup_label:   addressLabel(s.origin),
      dropoff_label:  addressLabel(s.destination),
      weight_kg:      s.weight?.grams ? Math.round((s.weight.grams / 1000) * 100) / 100 : null,
    }))
    .filter((s) => s.awb !== "");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

export const SIZE_CLASS_LABEL: Record<SizeClass, string> = {
  motorcycle:  "Motorcycle",
  sedan:       "Sedan",
  van:         "Cargo van",
  l300:        "L300",
  "6wheeler":  "6-wheeler",
  "10wheeler": "10-wheeler",
  trailer:     "Trailer",
};

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
