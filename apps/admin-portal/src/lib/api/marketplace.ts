/**
 * Admin Portal — Marketplace oversight API.
 *
 * Tenant-scoped view across every partner's `marketplace.vehicle_listings`
 * and `marketplace.bookings` (ADR-0013). The tenant_admin sees all rows by
 * virtue of `scope=tenant` in the session GUC — no partner filter applied.
 *
 * Real API: calls /v1/marketplace/* on the carrier service via authFetch.
 * A receipt has no record of its own: the receipt modal renders straight from
 * the booking the carrier service returned.
 */

import { authFetch } from "@/lib/auth/auth-fetch";
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ListingStatus  = "active" | "paused" | "booked" | "expired";
export type BookingStatus  = "pending" | "accepted" | "rejected" | "in_transit" | "delivered" | "cancelled" | "disputed";
export type SizeClass      = "scooter_bicycle" | "motorcycle" | "sedan" | "van" | "1ton" | "3ton" | "7ton" | "10ton" | "trailer" | "refrigerated_truck" | "recovery_truck";
export type VehicleFeature = "tail_lift" | "chiller" | "freezer";
export type PartnerType    = "alliance" | "marketplace";

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

export type BookingPaymentMethod = "invoice" | "online";
export type BookingPaymentStatus = "pending" | "authorized" | "captured" | "voided" | "failed";

/** Mirrors the carrier service's serde output (`services/carrier/src/domain/entities/mod.rs`). */
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

/**
 * A booking as the oversight table shows it: the real booking plus the carrier
 * label. The carrier service returns no carrier name, so `partner_display_name`
 * is the short carrier id until a tenant-scope carrier lookup exists.
 */
export interface AdminBooking extends MarketplaceBooking {
  partner_id:           string;
  partner_display_name: string;
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

/** Like `apiGet` but a failure throws, so callers can tell "outage" from "empty". */
async function apiGetStrict<T>(path: string): Promise<T> {
  const res = await authFetch(`${API_BASE}${path}`);
  if (!res.ok) throw new Error(`GET ${path} failed (HTTP ${res.status})`);
  return (await res.json()) as T;
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

function bookingToAdmin(b: MarketplaceBooking): AdminBooking {
  return {
    ...b,
    partner_id:           b.carrier_id,
    partner_display_name: b.carrier_id.slice(0, 8),
  };
}

// ── Public API functions ──────────────────────────────────────────────────────

export async function fetchAllListings(): Promise<AdminListing[]> {
  const json = await apiGet<{ listings?: ApiListing[] } | ApiListing[]>("/v1/marketplace/listings");
  if (!json) return [];
  const raw = Array.isArray(json) ? json : (json.listings ?? []);
  return raw.map(apiListingToAdmin);
}

/** Throws on any failure: an empty array always means the tenant has no bookings. */
export async function fetchAllBookings(): Promise<AdminBooking[]> {
  const json = await apiGetStrict<{ bookings?: MarketplaceBooking[] } | MarketplaceBooking[]>(
    "/v1/marketplace/bookings",
  );
  const raw = Array.isArray(json) ? json : (json.bookings ?? []);
  return raw
    .map(bookingToAdmin)
    .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
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
