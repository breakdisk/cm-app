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

export interface BusReceipt {
  id:                   string;
  booking_id:           string;
  receipt_no:           string;
  awb:                  string;
  partner_display_name: string;
  issued_by_name:       string;
  merchant_display:     string;
  consumer_display:     string;
  pickup_label:         string;
  dropoff_label:        string;
  pickup_at:            string;
  cargo_weight_kg:      number;
  size_class:           BusSizeClass;
  quoted_price_cents:   number;
  issued_at:            string;
  signed_by?:           string;
  notes?:               string;
}

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
  merchant_id:          string | null;      // null when consumer
  merchant_type:        BusMerchantType;
  merchant_display:     string;             // visible name on merchant/admin views
  consumer_display:     string;             // masked preview visible on partner side pre-accept
  size_class:           BusSizeClass;
  cargo_weight_kg:      number;
  pickup_label:         string;
  dropoff_label:        string;
  quoted_price_cents:   number;
  status:               BusBookingStatus;
  pickup_at:            string;
  created_at:           string;
  updated_at:           string;
  // Pickup transition — populated when carrier records cargo collected.
  // Status flips accepted → in_transit; in production this emits
  // `shipment.picked_up` on Kafka to unblock tracking + ETA updates.
  picked_up_at:         string | null;
  picked_up_by:         string | null;
  pickup_notes:         string | null;
}

const BUS_KEY      = "cm:marketplace:bookings:v1";
const RECEIPTS_KEY = "cm:marketplace:receipts:v1";

/**
 * Shipment receipt — the customer-facing artifact issued by the carrier after
 * pickup (or by admin override). In production this is emitted as
 * `shipment.receipt_issued` on Kafka; the engagement engine dispatches it to
 * the consumer via their preferred channel (SMS/WhatsApp/email). Pre-backend
 * we persist to localStorage so every portal can render the same receipt
 * consistently for the demo.
 */
export interface BusReceipt {
  id:                   string;
  receipt_no:           string;         // customer-visible: R-{YYYYMMDD}-{nnnn}
  booking_id:           string;         // FK to BusBooking.id
  awb:                  string;
  shipment_id:          string;
  partner_id:           string;
  partner_display_name: string;
  merchant_id:          string | null;
  merchant_display:     string;
  consumer_display:     string;
  pickup_label:         string;
  dropoff_label:        string;
  pickup_at:            string;
  size_class:           BusSizeClass;
  cargo_weight_kg:      number;
  quoted_price_cents:   number;
  issued_by:            "partner" | "admin" | "merchant";
  issued_by_name:       string;         // "FastShip Co. · Driver M. Cruz"
  signed_by:            string | null;  // handover signatory (driver or sender)
  notes:                string | null;
  issued_at:            string;
}

function safeParse(raw: string | null): BusBooking[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Back-fill pickup fields on older rows written before the schema gained
    // them — keeps the demo functional after an in-place upgrade.
    return (parsed as Partial<BusBooking>[]).map((b) => ({
      picked_up_at:  null,
      picked_up_by:  null,
      pickup_notes:  null,
      ...b,
    })) as BusBooking[];
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

/**
 * Carrier records cargo collected — transitions accepted → in_transit and
 * stamps pickup metadata. No-op if booking is not in `accepted`. Returns the
 * updated row, or null if not found / wrong status.
 */
export function markPickedUp(
  id: string,
  input: { picked_up_by: string | null; pickup_notes: string | null },
): BusBooking | null {
  const current = readBus();
  const i = current.findIndex((b) => b.id === id);
  if (i === -1) return null;
  if (current[i].status !== "accepted") return null;
  const now = new Date().toISOString();
  const updated: BusBooking = {
    ...current[i],
    status:        "in_transit",
    picked_up_at:  now,
    picked_up_by:  input.picked_up_by,
    pickup_notes:  input.pickup_notes,
    updated_at:    now,
  };
  current[i] = updated;
  writeBus(current);
  return updated;
}

/**
 * Subscribe to cross-tab / cross-portal bus updates (bookings OR receipts).
 * Returns an unsubscribe. The `storage` event fires in *other* tabs when this
 * tab writes; same-tab writes still need a manual refresh.
 */
export function subscribeToBus(cb: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (e: StorageEvent) => {
    if (e.key === BUS_KEY || e.key === RECEIPTS_KEY) cb();
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}

// ── Receipts ─────────────────────────────────────────────────────────────────

function safeParseReceipts(raw: string | null): BusReceipt[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? (parsed as BusReceipt[]) : [];
  } catch {
    return [];
  }
}

export function readReceipts(): BusReceipt[] {
  if (typeof window === "undefined") return [];
  return safeParseReceipts(window.localStorage.getItem(RECEIPTS_KEY));
}

export function writeReceipts(receipts: BusReceipt[]): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(RECEIPTS_KEY, JSON.stringify(receipts));
}

export function appendReceipt(r: BusReceipt): void {
  const current = readReceipts();
  // Dedupe by booking_id — one receipt per shipment, first write wins.
  if (current.some((x) => x.booking_id === r.booking_id)) return;
  writeReceipts([r, ...current]);
}

export function findReceiptByBookingId(bookingId: string): BusReceipt | null {
  return readReceipts().find((r) => r.booking_id === bookingId) ?? null;
}

export function findReceiptByAwb(awb: string): BusReceipt | null {
  return readReceipts().find((r) => r.awb === awb) ?? null;
}
