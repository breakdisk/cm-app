/**
 * Partner Portal — Marketplace API.
 *
 * Mirrors the schema defined in ADR-0013 (Marketplace Discovery addendum):
 *   - marketplace.vehicle_listings  → GET/POST/PUT/DELETE /v1/marketplace/listings
 *   - marketplace.bookings          → GET /v1/marketplace/bookings + transition POSTs
 *
 * Every operation goes through authFetch to the carrier service. A receipt has
 * no record of its own: the receipt modal renders straight from the booking.
 */

import { authFetch } from "@/lib/auth/auth-fetch";

// The gateway routes /v1/marketplace to the carrier service.
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Types (ADR-0013) ──────────────────────────────────────────────────────────

export type ListingStatus = "active" | "paused" | "booked" | "expired";

/** Wire values of the carrier service's `SizeClass` (services/carrier/src/domain/entities/mod.rs). */
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
  carrier_id:               string;
  vehicle_plate:             string;
  size_class:               SizeClass;
  max_weight_kg:            number;
  max_volume_m3:            number | null;
  base_price_cents:         number;
  per_km_cents:             number;
  per_kg_cents:             number | null;
  service_area_label:       string;
  idle_from:                string;  // ISO-8601
  idle_until:               string;  // ISO-8601
  status:                   ListingStatus;
  carrier_response_window_mins: number;
  bookings_today:           number;
  revenue_today_cents:      number;
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

// ── API helpers ───────────────────────────────────────────────────────────────

/** The carrier API's error envelope is `{ error: { code, message } }`; fall back to `fallback`. */
async function errorMessage(res: Response, fallback: string): Promise<string> {
  const body = (await res.json().catch(() => null)) as { error?: { message?: string } } | null;
  return body?.error?.message ?? fallback;
}

async function apiGet<T>(path: string): Promise<T> {
  const res = await authFetch(`${API_BASE}${path}`);
  if (!res.ok) {
    throw new Error(await errorMessage(res, `GET ${path} failed (HTTP ${res.status})`));
  }
  return res.json() as Promise<T>;
}

async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await authFetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    throw new Error(await errorMessage(res, `POST ${path} failed (HTTP ${res.status})`));
  }
  return res.json() as Promise<T>;
}

async function apiPut<T>(path: string, body?: unknown): Promise<T> {
  const res = await authFetch(`${API_BASE}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    throw new Error(await errorMessage(res, `PUT ${path} failed (HTTP ${res.status})`));
  }
  return res.json() as Promise<T>;
}

async function apiDelete(path: string): Promise<void> {
  const res = await authFetch(`${API_BASE}${path}`, { method: "DELETE" });
  if (!res.ok && res.status !== 204) {
    throw new Error(await errorMessage(res, `DELETE ${path} failed (HTTP ${res.status})`));
  }
}

// ── Listings ──────────────────────────────────────────────────────────────────

export async function fetchListings(): Promise<VehicleListing[]> {
  const data = await apiGet<{ listings: VehicleListing[] }>("/v1/marketplace/listings");
  return data.listings;
}

/** Mirrors the carrier service's `CreateListingCommand`. */
export interface CreateListingInput {
  vehicle_plate:                string;
  size_class:                   SizeClass;
  max_weight_kg:                number;
  max_volume_m3:                number | null;
  base_price_cents:             number;
  per_km_cents:                 number;
  per_kg_cents:                 number | null;
  service_area_label:           string;
  idle_from:                    string;  // ISO-8601
  idle_until:                   string;  // ISO-8601
  carrier_response_window_mins: number;
}

export async function createListing(input: CreateListingInput): Promise<VehicleListing> {
  return apiPost<VehicleListing>("/v1/marketplace/listings", input);
}

export async function updateListing(
  id: string,
  patch: Partial<Pick<VehicleListing, "status" | "base_price_cents" | "per_km_cents" | "per_kg_cents" | "idle_until" | "service_area_label" | "max_weight_kg" | "max_volume_m3" | "carrier_response_window_mins">>,
): Promise<VehicleListing | null> {
  return apiPut<VehicleListing>(`/v1/marketplace/listings/${id}`, patch);
}

export async function deleteListing(id: string): Promise<boolean> {
  await apiDelete(`/v1/marketplace/listings/${id}`);
  return true;
}

// ── Bookings ──────────────────────────────────────────────────────────────────

export async function fetchBookings(): Promise<MarketplaceBooking[]> {
  const data = await apiGet<{ bookings: MarketplaceBooking[] }>("/v1/marketplace/bookings");
  return data.bookings;
}

export async function acceptBooking(id: string): Promise<MarketplaceBooking | null> {
  return apiPost<MarketplaceBooking>(`/v1/marketplace/bookings/${id}/accept`);
}

export async function rejectBooking(id: string): Promise<MarketplaceBooking | null> {
  return apiPost<MarketplaceBooking>(`/v1/marketplace/bookings/${id}/reject`);
}

export interface RecordPickupInput {
  booking_id:   string;
  picked_up_by?: string | null;
  pickup_notes?: string | null;
}

export async function recordPickup(input: RecordPickupInput): Promise<MarketplaceBooking | null> {
  return apiPost<MarketplaceBooking>(`/v1/marketplace/bookings/${input.booking_id}/pickup`, {
    picked_up_by: input.picked_up_by ?? null,
    pickup_notes: input.pickup_notes ?? null,
  });
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

export const SIZE_CLASS_CAPACITY_HINT: Record<SizeClass, string> = {
  motorcycle:  "Up to 30 kg · 0.25 m³",
  sedan:       "Up to 200 kg · 1.2 m³",
  van:         "Up to 800 kg · 5 m³",
  l300:        "Up to 1,500 kg · 6.7 m³",
  "6wheeler":  "Up to 5,000 kg · 22.9 m³",
  "10wheeler": "Up to 10,000 kg · 43 m³",
  trailer:     "Up to 25,000 kg · 80 m³ · Flatbed available",
};

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
