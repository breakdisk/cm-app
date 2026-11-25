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

const CARRIER_SVC_URL = process.env.NEXT_PUBLIC_CARRIER_URL ?? "http://localhost:8010";

// ── Types (ADR-0013) ──────────────────────────────────────────────────────────

export type ListingStatus = "active" | "paused" | "booked" | "expired";

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

export interface VehicleListing {
  id:                       string;
  tenant_id:                string;
  carrier_id:               string;
  vehicle_plate:             string;
  size_class:               SizeClass;
  features:                 VehicleFeature[];
  max_weight_kg:            number;
  max_volume_m3:            number | null;
  currency:                 string;   // ISO-4217 code, e.g. "PHP"
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

async function apiGet<T>(path: string): Promise<T> {
  const res = await authFetch(`${CARRIER_SVC_URL}${path}`);
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`GET ${path} failed ${res.status}: ${body}`);
  }
  return res.json() as Promise<T>;
}

async function apiPost<T>(path: string, body?: unknown): Promise<T> {
  const res = await authFetch(`${CARRIER_SVC_URL}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`POST ${path} failed ${res.status}: ${text}`);
  }
  return res.json() as Promise<T>;
}

async function apiPut<T>(path: string, body?: unknown): Promise<T> {
  const res = await authFetch(`${CARRIER_SVC_URL}${path}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`PUT ${path} failed ${res.status}: ${text}`);
  }
  return res.json() as Promise<T>;
}

async function apiDelete(path: string): Promise<void> {
  const res = await authFetch(`${CARRIER_SVC_URL}${path}`, { method: "DELETE" });
  if (!res.ok && res.status !== 204) {
    const text = await res.text().catch(() => "");
    throw new Error(`DELETE ${path} failed ${res.status}: ${text}`);
  }
}

// ── Listings ──────────────────────────────────────────────────────────────────

export async function fetchListings(): Promise<VehicleListing[]> {
  const data = await apiGet<{ listings: VehicleListing[] }>("/v1/marketplace/listings");
  return data.listings;
}

export async function createListing(
  input: Omit<VehicleListing, "id" | "tenant_id" | "carrier_id" | "bookings_today" | "revenue_today_cents" | "created_at" | "updated_at">,
): Promise<VehicleListing> {
  return apiPost<VehicleListing>("/v1/marketplace/listings", input);
}

export async function updateListing(
  id: string,
  patch: Partial<Pick<VehicleListing, "status" | "currency" | "base_price_cents" | "per_km_cents" | "per_kg_cents" | "idle_until" | "service_area_label" | "max_weight_kg" | "max_volume_m3" | "carrier_response_window_mins">>,
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

export function formatCentsPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}
