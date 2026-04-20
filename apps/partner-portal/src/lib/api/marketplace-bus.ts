/**
 * Marketplace Bus — pre-backend demo propagation layer.
 *
 * All three portals (merchant, partner, admin) run on the same origin with
 * different basePaths, so localStorage is shared across them. This module
 * provides a single key (`BUS_KEY`) that each portal writes to and reads
 * from, simulating the real backend's marketplace.bookings table for the
 * UI-only demo.
 *
 * When the real `/v1/marketplace/bookings` endpoints ship, each portal's
 * marketplace.ts replaces calls here with `authFetch` — the bus goes away.
 *
 * Safe on SSR: every access checks `typeof window`.
 */

export type BusBookingStatus =
  | "pending"
  | "accepted"
  | "rejected"
  | "in_transit"
  | "delivered"
  | "cancelled"
  | "disputed";

export type BusSizeClass =
  | "motorcycle"
  | "sedan"
  | "van"
  | "l300"
  | "6wheeler"
  | "10wheeler"
  | "trailer";

export type BusMerchantType = "business" | "consumer";

export interface BusBooking {
  id:                   string;
  listing_id:           string;
  shipment_id:          string;
  awb:                  string;
  partner_id:           string;
  partner_display_name: string;
  merchant_id:          string | null;
  merchant_type:        BusMerchantType;
  merchant_display:     string;
  consumer_display:     string;
  size_class:           BusSizeClass;
  cargo_weight_kg:      number;
  pickup_label:         string;
  dropoff_label:        string;
  quoted_price_cents:   number;
  status:               BusBookingStatus;
  pickup_at:            string;
  created_at:           string;
  updated_at:           string;
}

const BUS_KEY = "cm:marketplace:bookings:v1";

function safeParse(raw: string | null): BusBooking[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as BusBooking[]) : [];
  } catch {
    return [];
  }
}

export function readBus(): BusBooking[] {
  if (typeof window === "undefined") return [];
  return safeParse(window.localStorage.getItem(BUS_KEY));
}

export function writeBus(bookings: BusBooking[]): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(BUS_KEY, JSON.stringify(bookings));
}

export function appendBooking(b: BusBooking): void {
  const current = readBus();
  writeBus([b, ...current]);
}

export function updateBookingStatus(
  id: string,
  status: BusBookingStatus,
): BusBooking | null {
  const current = readBus();
  const i = current.findIndex((b) => b.id === id);
  if (i === -1) return null;
  const updated: BusBooking = { ...current[i], status, updated_at: new Date().toISOString() };
  current[i] = updated;
  writeBus(current);
  return updated;
}

export function subscribeToBus(cb: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (e: StorageEvent) => {
    if (e.key === BUS_KEY) cb();
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}
