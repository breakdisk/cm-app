/**
 * Partner Portal — Marketplace API.
 *
 * Mirrors the schema defined in ADR-0013 (Marketplace Discovery addendum):
 *   - marketplace.vehicle_listings  → GET/POST/PUT/DELETE /v1/marketplace/listings
 *   - marketplace.bookings          → GET /v1/marketplace/bookings + transition POSTs
 *
 * Cross-portal receipt propagation (pre-receipt-service): receipts are still
 * written to the marketplace-bus localStorage layer until the receipt service
 * ships. All other operations use authFetch against the carrier service.
 */

import { authFetch } from "@/lib/auth/auth-fetch";
import {
  appendReceipt as busAppendReceipt,
  findReceiptByBookingId as busFindReceiptByBookingId,
  subscribeToBus,
  type BusReceipt,
} from "./marketplace-bus";
import { getCurrentPartner } from "./partner-identity";

const CARRIER_SVC_URL = process.env.NEXT_PUBLIC_CARRIER_URL ?? "http://localhost:8010";

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

export interface MarketplaceBooking {
  id:                   string;
  listing_id:           string;
  shipment_id:          string;
  awb:                  string;
  consumer_name:        string;  // masked to initials when pending
  consumer_phone:       string | null;  // null when pending/rejected
  pickup_label:         string;
  dropoff_label:        string;
  cargo_weight_kg:      number;
  cargo_volume_m3:      number | null;
  quoted_price_cents:   number;
  status:               BookingStatus;
  pickup_at:            string;
  created_at:           string;
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
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

export { subscribeToBus as subscribeToMarketplaceUpdates };

// ── Receipts (pre-receipt-service: still uses bus) ────────────────────────────

export interface IssueReceiptInput {
  booking_id: string;
  signed_by?: string | null;
  notes?:     string | null;
}

export async function issueReceipt(input: IssueReceiptInput): Promise<BusReceipt | null> {
  const existing = busFindReceiptByBookingId(input.booking_id);
  if (existing) return existing;

  // Receipt service not yet shipped — build the artifact locally so the
  // partner portal can render it immediately. Production will POST to
  // /v1/receipts and the engagement engine will forward it to the consumer.
  const partner = getCurrentPartner();
  const issuedAt = new Date();
  const yyyy = issuedAt.getUTCFullYear();
  const mm   = String(issuedAt.getUTCMonth() + 1).padStart(2, "0");
  const dd   = String(issuedAt.getUTCDate()).padStart(2, "0");
  const seq  = String(issuedAt.getTime()).slice(-4);

  // Fetch the booking to populate the receipt fields.
  let awb = "";
  let shipmentId = "";
  try {
    const data = await apiGet<{ bookings: MarketplaceBooking[] }>("/v1/marketplace/bookings");
    const booking = data.bookings.find((b) => b.id === input.booking_id);
    if (booking) { awb = booking.awb; shipmentId = booking.shipment_id; }
  } catch {
    // Non-critical — receipt is still issued with empty AWB
  }

  const receipt: BusReceipt = {
    id:                   `r9000000-0000-0000-0000-${issuedAt.getTime().toString().padStart(12, "0")}`,
    receipt_no:           `R-${yyyy}${mm}${dd}-${seq}`,
    booking_id:           input.booking_id,
    awb,
    shipment_id:          shipmentId,
    partner_id:           partner.id,
    partner_display_name: partner.name,
    merchant_id:          null,
    merchant_display:     "",
    consumer_display:     "",
    pickup_label:         "",
    dropoff_label:        "",
    pickup_at:            new Date().toISOString(),
    size_class:           "van",
    cargo_weight_kg:      0,
    quoted_price_cents:   0,
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
