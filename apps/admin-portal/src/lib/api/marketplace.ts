/**
 * Admin Portal — Marketplace oversight API.
 *
 * Tenant-scoped view across every partner's `marketplace.vehicle_listings`
 * and `marketplace.bookings` (ADR-0013). The tenant_admin sees all rows by
 * virtue of `scope=tenant` in the session GUC — no partner filter applied.
 *
 * Real API: calls /v1/marketplace/* on the carrier service via authFetch.
 * Bus fallback: for bookings, merges in-flight bus rows (partner/merchant
 * portal demo layer) so cross-portal receipts remain visible while the
 * receipt service is still pre-ship.
 */

import { authFetch } from "@/lib/auth/auth-fetch";
import {
  readBus,
  subscribeToBus,
  findReceiptByBookingId as busFindReceiptByBookingId,
  type BusBooking,
  type BusReceipt,
} from "./marketplace-bus";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

export type { BusReceipt } from "./marketplace-bus";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ListingStatus  = "active" | "paused" | "booked" | "expired";
export type BookingStatus  = "pending" | "accepted" | "rejected" | "in_transit" | "delivered" | "cancelled" | "disputed";
export type SizeClass      = "scooter_bicycle" | "motorcycle" | "sedan" | "van" | "1ton" | "3ton" | "7ton" | "10ton" | "trailer" | "refrigerated_truck" | "recovery_truck";
export type VehicleFeature = "tail_lift" | "chiller" | "freezer";
export type PartnerType    = "alliance" | "marketplace";
export type MerchantType   = "business" | "consumer";    // ADR-0013: business = tenant merchant; consumer = walk-up booker

export interface AdminListing {
  id:                   string;
  partner_id:           string;
  partner_display_name: string;
  partner_type:         PartnerType;       // alliance = full; marketplace = vehicle-only
  vehicle_plate:        string;
  size_class:           SizeClass;
  features:             VehicleFeature[];
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
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
}

export interface MarketplaceStats {
  active_listings:       number;
  idle_vehicles_next_6h: number;
  bookings_today:        number;
  gmv_today_cents:       number;
  avg_match_seconds:     number;             // booking intent → carrier accepted
  partners_participating: number;
}

// ── Raw API response shapes (carrier service) ─────────────────────────────────

interface ApiListing {
  id:                       string;
  carrier_id:               string;
  vehicle_plate:             string;
  size_class:               SizeClass;
  features?:                VehicleFeature[];
  max_weight_kg:            number;
  base_price_cents:         number;
  per_km_cents:             number;
  service_area_label:       string;
  idle_until:               string;
  status:                   ListingStatus;
  bookings_today?:          number;
  revenue_today_cents?:     number;
  updated_at:               string;
}

interface ApiBooking {
  id:                   string;
  listing_id:           string;
  shipment_id:          string;
  awb:                  string;
  carrier_id:           string;
  consumer_name:        string;
  consumer_phone:       string | null;
  pickup_label:         string;
  dropoff_label:        string;
  cargo_weight_kg:      number;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;
  created_at:           string;
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
}

// ── API helpers ───────────────────────────────────────────────────────────────

async function apiGet<T>(path: string): Promise<T | null> {
  try {
    const res = await authFetch(`${API_BASE}${path}`);
    if (!res.ok) return null;
    return res.json() as Promise<T>;
  } catch {
    return null;
  }
}

// ── Projection helpers ────────────────────────────────────────────────────────

function apiListingToAdmin(l: ApiListing): AdminListing {
  return {
    id:                   l.id,
    partner_id:           l.carrier_id,
    partner_display_name: l.carrier_id.slice(0, 8),   // filled in by backend once tenant-scope endpoint lands
    partner_type:         "marketplace" as PartnerType,
    vehicle_plate:        l.vehicle_plate,
    size_class:           l.size_class,
    features:             (l.features ?? []) as VehicleFeature[],
    max_weight_kg:        l.max_weight_kg,
    base_price_cents:     l.base_price_cents,
    per_km_cents:         l.per_km_cents,
    service_area_label:   l.service_area_label,
    idle_until:           l.idle_until,
    status:               l.status,
    bookings_today:       l.bookings_today ?? 0,
    revenue_today_cents:  l.revenue_today_cents ?? 0,
    rating:               0,
    updated_at:           l.updated_at,
  };
}

function apiBookingToAdmin(b: ApiBooking): AdminBooking {
  return {
    id:                   b.id,
    shipment_id:          b.shipment_id,
    awb:                  b.awb,
    partner_id:           b.carrier_id,
    partner_display_name: b.carrier_id.slice(0, 8),
    merchant_type:        "consumer" as MerchantType,
    merchant_id:          null,
    consumer_display:     b.consumer_name,
    size_class:           "van" as SizeClass,          // not in booking response; default
    cargo_weight_kg:      b.cargo_weight_kg,
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
    consumer_display:     b.merchant_display,
    size_class:           b.size_class,
    cargo_weight_kg:      b.cargo_weight_kg,
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

// ── Public API functions ──────────────────────────────────────────────────────

export async function fetchAllListings(): Promise<AdminListing[]> {
  const json = await apiGet<{ listings?: ApiListing[] } | ApiListing[]>("/v1/marketplace/listings");
  if (!json) return [];
  const raw = Array.isArray(json) ? json : (json.listings ?? []);
  return raw.map(apiListingToAdmin);
}

export async function fetchAllBookings(): Promise<AdminBooking[]> {
  const busRows = readBus().map(busToAdminBooking);
  const json = await apiGet<{ bookings?: ApiBooking[] } | ApiBooking[]>("/v1/marketplace/bookings");
  const apiRows = json
    ? (Array.isArray(json) ? json : (json.bookings ?? [])).map(apiBookingToAdmin)
    : [];

  // Bus rows overlay API rows (bus has richer display names from the pre-service demo layer)
  const byId = new Map<string, AdminBooking>();
  for (const b of apiRows)  byId.set(b.id, b);
  for (const b of busRows)  byId.set(b.id, b);
  return [...byId.values()].sort(
    (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );
}

export async function fetchMarketplaceStats(): Promise<MarketplaceStats> {
  const [listings, bookings] = await Promise.all([fetchAllListings(), fetchAllBookings()]);
  const active      = listings.filter((l) => l.status === "active" || l.status === "booked").length;
  const todayGmv    = listings.reduce((s, l) => s + l.revenue_today_cents, 0);
  const partners    = new Set(listings.map((l) => l.partner_id)).size;
  const now         = Date.now();
  const idleNext6h  = listings.filter((l) => {
    const ms = new Date(l.idle_until).getTime() - now;
    return ms > 0 && ms < 6 * 3_600_000;
  }).length;
  return {
    active_listings:        active,
    idle_vehicles_next_6h:  idleNext6h,
    bookings_today:         bookings.length,
    gmv_today_cents:        todayGmv,
    avg_match_seconds:      0,
    partners_participating: partners,
  };
}

export interface CreateListingPayload {
  vehicle_plate:                string;
  size_class:                   SizeClass;
  features:                     VehicleFeature[];
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
}

export async function createVehicleListing(payload: CreateListingPayload): Promise<void> {
  const res = await authFetch(`${API_BASE}/v1/marketplace/listings`, {
    method:  "POST",
    headers: { "Content-Type": "application/json" },
    body:    JSON.stringify(payload),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`POST /v1/marketplace/listings failed ${res.status}: ${text}`);
  }
}

export { subscribeToBus as subscribeToMarketplaceUpdates };

export async function fetchReceiptForBooking(bookingId: string): Promise<BusReceipt | null> {
  return busFindReceiptByBookingId(bookingId);
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

export const VEHICLE_FEATURE_LABEL: Record<VehicleFeature, string> = {
  tail_lift: "Tail-lift",
  chiller:   "Chiller (0–8 °C)",
  freezer:   "Freezer (−18 °C)",
};

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
