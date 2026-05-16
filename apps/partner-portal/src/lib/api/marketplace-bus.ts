/**
 * Marketplace Receipt Bus — pre-receipt-service persistence layer.
 *
 * Booking state is now fully managed by the carrier-service backend API.
 * Receipts are the only artifact that still lives in localStorage — until
 * the real `/v1/receipts` endpoint ships, every portal that shares the same
 * origin reads and writes the same `RECEIPTS_KEY`.
 *
 * Safe on SSR: every access checks `typeof window`.
 */

export type BusSizeClass =
  | "motorcycle"
  | "sedan"
  | "van"
  | "l300"
  | "6wheeler"
  | "10wheeler"
  | "trailer";

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
  booking_id:           string;
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
  issued_by_name:       string;
  signed_by:            string | null;
  notes:                string | null;
  issued_at:            string;
}

const RECEIPTS_KEY = "cm:marketplace:receipts:v1";

/**
 * Subscribe to cross-tab receipt writes. Returns an unsubscribe function.
 * The `storage` event fires in *other* tabs when this tab writes a receipt;
 * same-tab writes update React state directly so no event is needed there.
 */
export function subscribeToBus(cb: () => void): () => void {
  if (typeof window === "undefined") return () => {};
  const handler = (e: StorageEvent) => {
    if (e.key === RECEIPTS_KEY) cb();
  };
  window.addEventListener("storage", handler);
  return () => window.removeEventListener("storage", handler);
}

// ── Receipts ──────────────────────────────────────────────────────────────────

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
