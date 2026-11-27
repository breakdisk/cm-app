/**
 * Delivery PIN — `POST /v1/otps/generate` (pod, through the gateway).
 *
 * Decided 2026-09-15: the PIN is required to complete a delivery, and it is
 * shown from booking. What shapes this file:
 *
 * - **Issued once, at booking.** pod keeps a PIN until delivery (60 days by
 *   default), so the app asks for it when the shipment is booked and keeps it
 *   in SecureStore. Nothing here expires it; the card is only shown while the
 *   shipment is still moving.
 * - **pod does not replace a live PIN unless asked** (`reissue: true`). A
 *   request for a shipment that already has one returns `active: true` and no
 *   code: the PIN exists, just not on this device. Only then — or when the
 *   customer says theirs is not working — does the app offer a new one, which
 *   replaces the old.
 * - **The server decides who sees the code.** Where it withholds it the body is
 *   `sent: true` (texted to the recipient) with no code.
 *
 * `recipient_phone` is still sent. The deployed pod texts the number in the
 * body; the fixed handler reads the shipment record instead.
 */
import * as SecureStore from 'expo-secure-store';
import { getPodClient } from './client';

export type DeliveryPin =
  | { kind: 'code'; code: string; issuedAt: number }
  /** Texted to the recipient; this account is not shown the code. */
  | { kind: 'sent'; issuedAt: number }
  /** A live PIN exists but is not on this device. Never stored. */
  | { kind: 'active'; issuedAt: number };

const storageKey = (shipmentId: string) => `delivery_pin_${shipmentId}`;

export function parseIssuedPin(body: unknown, issuedAt: number): DeliveryPin {
  const data = (body as any)?.data ?? body;
  const code = data?.code;
  if (typeof code === 'string' && /^\d{4,8}$/.test(code)) {
    return { kind: 'code', code, issuedAt };
  }
  if (data?.active === true) {
    return { kind: 'active', issuedAt };
  }
  if (data?.sent === true) {
    return { kind: 'sent', issuedAt };
  }
  throw new Error('The PIN service returned neither a code nor a sent confirmation.');
}

function parseStored(raw: string): DeliveryPin | null {
  try {
    const v = JSON.parse(raw);
    if (typeof v?.issuedAt !== 'number') return null;
    if (v.kind === 'code' && typeof v.code === 'string') return { kind: 'code', code: v.code, issuedAt: v.issuedAt };
    if (v.kind === 'sent') return { kind: 'sent', issuedAt: v.issuedAt };
    return null;
  } catch {
    return null;
  }
}

/** The PIN this device holds for the shipment, or null. Never calls the server. */
export async function getStoredDeliveryPin(shipmentId: string): Promise<DeliveryPin | null> {
  try {
    const raw = await SecureStore.getItemAsync(storageKey(shipmentId));
    return raw ? parseStored(raw) : null;
  } catch {
    return null;
  }
}

type Listener = (shipmentId: string, pin: DeliveryPin) => void;
const listeners = new Set<Listener>();
const inFlight = new Map<string, Promise<DeliveryPin>>();

/**
 * Tells every mounted card for the shipment. Booking, History and Tracking can
 * all be mounted at once; one must show the PIN another issued, not issue over it.
 */
export function onDeliveryPinIssued(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * Asks pod for the shipment's PIN. Without `reissue` a live PIN is kept and
 * comes back as `active`; with it, the old PIN is replaced. A second identical
 * request while the first is open joins it.
 */
export function issueDeliveryPin(
  shipmentId: string,
  recipientPhone: string,
  { reissue = false }: { reissue?: boolean } = {},
): Promise<DeliveryPin> {
  const key = `${shipmentId}:${reissue}`;
  const pending = inFlight.get(key);
  if (pending) return pending;

  const request = (async () => {
    const response = await getPodClient().post('/v1/otps/generate', {
      shipment_id: shipmentId,
      recipient_phone: recipientPhone,
      reissue,
    });
    const pin = parseIssuedPin(response.data, Date.now());
    if (pin.kind !== 'active') {
      // A failed write costs only a "Get a new PIN" after an app restart.
      await SecureStore.setItemAsync(storageKey(shipmentId), JSON.stringify(pin)).catch(() => {});
    }
    listeners.forEach(listener => listener(shipmentId, pin));
    return pin;
  })().finally(() => {
    inFlight.delete(key);
  });

  inFlight.set(key, request);
  return request;
}
