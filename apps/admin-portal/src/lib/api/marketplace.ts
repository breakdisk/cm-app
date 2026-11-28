/**
 * Admin Portal — Marketplace oversight API.
 *
 * Tenant-scoped view across every partner's vehicle listings and bookings
 * (ADR-0013). Reads and writes go through the carrier service's
 * `/v1/marketplace/admin/*` routes (`carriers:manage`), which scope by the
 * caller's tenant. The plain `/v1/marketplace/listings|bookings` routes resolve
 * the caller's *own* carrier from their email and cannot serve an operator.
 * A receipt has no record of its own: the receipt modal renders straight from
 * the booking the carrier service returned.
 */

import { authFetch } from "@/lib/auth/auth-fetch";
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Types ─────────────────────────────────────────────────────────────────────

export type ListingStatus  = "active" | "paused" | "booked" | "expired";
export type BookingStatus  = "pending" | "accepted" | "rejected" | "in_transit" | "delivered" | "cancelled" | "disputed";
/** Wire values of the carrier service's `SizeClass` (services/carrier/src/domain/entities/mod.rs). */
export type SizeClass      = "motorcycle" | "sedan" | "van" | "l300" | "6wheeler" | "10wheeler" | "trailer";
export type PartnerType    = "alliance" | "marketplace";

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
 * is the short carrier id; the carrier service does not return carrier names here.
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

/** GET that throws on any failure, so callers can tell "outage" from "empty". */
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
    partner_display_name: l.carrier_id.slice(0, 8),
    partner_type:         "marketplace" as PartnerType,
    vehicle_plate:        l.vehicle_plate,
    size_class:           l.size_class,
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

// ── Pagination ────────────────────────────────────────────────────────────────
//
// The carrier service pages `/v1/marketplace/admin/*` (default 50, capped at
// 100) and reports `total` beside the page. Reading only the first response
// silently drops row 51 onward, so every reader here either asks for a page and
// keeps `total`, or walks the pages until it has them all.

/** Rows per request the carrier service will honour. */
export const ADMIN_PAGE_SIZE = 50;
const MAX_PAGE_SIZE = 100;
/** Safety stop for "fetch everything" callers; a tenant beyond this needs a server-side aggregate. */
const MAX_ALL_ROWS = 5_000;

export interface Page<T> {
  items:  T[];
  /** Every row the tenant has, not just this page. */
  total:  number;
  limit:  number;
  offset: number;
}

interface PagedEnvelope {
  total?:  number;
  limit?:  number;
  offset?: number;
  [key: string]: unknown;
}

async function fetchPage<Raw>(
  path: string, key: "listings" | "bookings", offset: number, limit: number,
): Promise<Page<Raw>> {
  const json = await apiGetStrict<PagedEnvelope | Raw[]>(
    `${path}?limit=${limit}&offset=${offset}`,
  );
  const items = (Array.isArray(json) ? json : ((json[key] as Raw[] | undefined) ?? []));
  // An older carrier service sends no `total`: fall back to what we can see.
  const total = !Array.isArray(json) && typeof json.total === "number"
    ? json.total
    : offset + items.length;
  return { items, total, limit, offset };
}

/** Walks pages until `want` rows are held or the tenant has no more. */
async function fetchUpTo<Raw>(
  path: string, key: "listings" | "bookings", want: number,
): Promise<{ items: Raw[]; total: number }> {
  const items: Raw[] = [];
  let total = 0;
  while (items.length < want) {
    const page = await fetchPage<Raw>(path, key, items.length, Math.min(MAX_PAGE_SIZE, want - items.length));
    items.push(...page.items);
    total = page.total;
    if (page.items.length === 0 || items.length >= total) break;
  }
  return { items, total };
}

const sortNewestFirst = (a: AdminBooking, b: AdminBooking) =>
  new Date(b.created_at).getTime() - new Date(a.created_at).getTime();

/** One page of listings. Throws on any failure. */
export async function fetchListingsPage(offset = 0, limit = ADMIN_PAGE_SIZE): Promise<Page<AdminListing>> {
  const p = await fetchPage<ApiListing>("/v1/marketplace/admin/listings", "listings", offset, limit);
  return { ...p, items: p.items.map(apiListingToAdmin) };
}

/** One page of bookings, newest first. Throws on any failure. */
export async function fetchBookingsPage(offset = 0, limit = ADMIN_PAGE_SIZE): Promise<Page<AdminBooking>> {
  const p = await fetchPage<MarketplaceBooking>("/v1/marketplace/admin/bookings", "bookings", offset, limit);
  return { ...p, items: p.items.map(bookingToAdmin).sort(sortNewestFirst) };
}

/** The first `want` listings (at least one page) plus the tenant total. Throws on any failure. */
export async function fetchListingsUpTo(want: number): Promise<{ items: AdminListing[]; total: number }> {
  const r = await fetchUpTo<ApiListing>("/v1/marketplace/admin/listings", "listings", Math.max(want, ADMIN_PAGE_SIZE));
  return { items: r.items.map(apiListingToAdmin), total: r.total };
}

/** The first `want` bookings, newest first (at least one page), plus the tenant total. Throws on any failure. */
export async function fetchBookingsUpTo(want: number): Promise<{ items: AdminBooking[]; total: number }> {
  const r = await fetchUpTo<MarketplaceBooking>("/v1/marketplace/admin/bookings", "bookings", Math.max(want, ADMIN_PAGE_SIZE));
  return { items: r.items.map(bookingToAdmin).sort(sortNewestFirst), total: r.total };
}

/** Every listing, across all pages. Throws on any failure: an empty array always means the tenant has none. */
export async function fetchAllListings(): Promise<AdminListing[]> {
  return (await fetchListingsUpTo(MAX_ALL_ROWS)).items;
}

/** Every booking, across all pages. Throws on any failure: an empty array always means the tenant has none. */
export async function fetchAllBookings(): Promise<AdminBooking[]> {
  return (await fetchBookingsUpTo(MAX_ALL_ROWS)).items;
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
  /** The carrier the listing belongs to; must be in the caller's tenant. */
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
  carrier_response_window_mins: number;
}

export async function createVehicleListing(payload: CreateListingPayload): Promise<void> {
  const res = await authFetch(`${API_BASE}/v1/marketplace/admin/listings`, {
    method:  "POST",
    headers: { "Content-Type": "application/json" },
    body:    JSON.stringify(payload),
  });
  if (!res.ok) {
    // The carrier API's error envelope is { error: { code, message } }.
    const body = (await res.json().catch(() => null)) as { error?: { message?: string } } | null;
    throw new Error(body?.error?.message ?? `Marketplace listing failed (HTTP ${res.status})`);
  }
}

export interface CarrierOption { id: string; name: string }

/** Carriers in the caller's tenant, for the listing-owner picker. Throws on failure. */
export async function fetchCarrierOptions(): Promise<CarrierOption[]> {
  const json = await apiGetStrict<{ carriers?: CarrierOption[] }>("/v1/carriers?limit=100");
  return (json.carriers ?? []).map((c) => ({ id: c.id, name: c.name }));
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
