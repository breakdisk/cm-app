/**
 * Delivery PIN — `POST /v1/otps/generate` (pod, through the gateway).
 *
 * Three backend facts shape this file:
 *
 * - **A PIN lives 15 minutes** (`OtpCode::new`, services/pod/src/domain/entities/otp.rs).
 *   Issued at booking it would be dead long before the driver arrives, so the
 *   app asks for it at handover time instead.
 * - **Every call issues a new code** and texts the recipient again, and the
 *   driver's submit checks only the newest one. A screen that issued on mount
 *   would replace the code the recipient already holds each time it rendered.
 *   So the app issues only on an explicit tap, and keeps the result until it
 *   expires.
 * - **The server decides who sees the code.** Where it withholds it the body
 *   carries `sent: true` and no code. That is kept too, so the app does not
 *   re-issue a PIN it will never be shown.
 *
 * `recipient_phone` is still sent. The deployed pod texts the number in the
 * body; the fixed handler (PR #161) ignores it and reads the shipment record.
 */
import * as SecureStore from 'expo-secure-store';
import { getPodClient } from './client';

/** Mirrors `OtpCode::new`. The server does not return an expiry. */
export const DELIVERY_PIN_TTL_MS = 15 * 60 * 1000;

export type DeliveryPin =
  | { kind: 'code'; code: string; issuedAt: number }
  | { kind: 'sent'; issuedAt: number };

const storageKey = (shipmentId: string) => `delivery_pin_${shipmentId}`;

export function isPinLive(pin: DeliveryPin | null, now: number = Date.now()): pin is DeliveryPin {
  return !!pin && now - pin.issuedAt < DELIVERY_PIN_TTL_MS;
}

export function parseIssuedPin(body: unknown, issuedAt: number): DeliveryPin {
  const data = (body as any)?.data ?? body;
  const code = data?.code;
  if (typeof code === 'string' && /^\d{4,8}$/.test(code)) {
    return { kind: 'code', code, issuedAt };
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

/** A live PIN this device already holds, or null. Never calls the server. */
export async function getStoredDeliveryPin(shipmentId: string, now: number = Date.now()): Promise<DeliveryPin | null> {
  try {
    const raw = await SecureStore.getItemAsync(storageKey(shipmentId));
    const pin = raw ? parseStored(raw) : null;
    return isPinLive(pin, now) ? pin : null;
  } catch {
    return null;
  }
}

type Listener = (shipmentId: string, pin: DeliveryPin) => void;
const listeners = new Set<Listener>();
const inFlight = new Map<string, Promise<DeliveryPin>>();

/**
 * Tells every mounted card for the shipment. History and Tracking both stay
 * mounted as tabs; one must show the PIN the other issued, not issue over it.
 */
export function onDeliveryPinIssued(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Issues a new PIN. A second tap while the first request is open joins it. */
export function issueDeliveryPin(shipmentId: string, recipientPhone: string): Promise<DeliveryPin> {
  const pending = inFlight.get(shipmentId);
  if (pending) return pending;

  const request = (async () => {
    // Stamped before the request, so the local expiry never outlives the server's.
    const issuedAt = Date.now();
    const response = await getPodClient().post('/v1/otps/generate', {
      shipment_id: shipmentId,
      recipient_phone: recipientPhone,
    });
    const pin = parseIssuedPin(response.data, issuedAt);
    // A failed write costs only a re-issue after an app restart.
    await SecureStore.setItemAsync(storageKey(shipmentId), JSON.stringify(pin)).catch(() => {});
    listeners.forEach(listener => listener(shipmentId, pin));
    return pin;
  })().finally(() => {
    inFlight.delete(shipmentId);
  });

  inFlight.set(shipmentId, request);
  return request;
}
