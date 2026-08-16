/**
 * Where the customer is.
 *
 * One definition, because three callers need it and they must agree: browsing
 * centres its vendor list here, a mesh run centres every specialist's search
 * here, and checkout offers the job to couriers here. Three copies drift, and
 * the symptom is subtle — the shops you browsed are not the shops the agent
 * found, and the courier is offered a pickup somewhere else again.
 *
 * Now a real, saved address rather than a constant. It is read synchronously
 * from a module-level cache so callers stay simple; `loadDeliveryPoint()` must
 * be awaited once at startup to populate it, which the root layout does.
 */
import * as SecureStore from "expo-secure-store";

export interface DeliveryPoint {
  lat: number;
  lng: number;
  /** What the customer typed, so a screen can show something human. */
  label: string;
}

const KEY = "delivery_point";

/**
 * Fallback only — the centre of Manila.
 *
 * Deliberately not silently used as a real address: `hasDeliveryPoint()` is
 * false until the customer sets one, and the app routes them to the address
 * screen rather than quietly delivering to a default. A wrong address is worse
 * than an absent one, because it looks like it worked.
 */
const FALLBACK: DeliveryPoint = { lat: 14.5995, lng: 120.9842, label: "Manila" };

let cached: DeliveryPoint | null = null;

/** Populate the cache. Call once, before anything reads the point. */
export async function loadDeliveryPoint(): Promise<void> {
  try {
    const raw = await SecureStore.getItemAsync(KEY);
    cached = raw ? (JSON.parse(raw) as DeliveryPoint) : null;
  } catch {
    // A corrupt or unreadable store is the same as not having one: ask again
    // rather than delivering somewhere the customer never chose.
    cached = null;
  }
}

export function hasDeliveryPoint(): boolean {
  return cached !== null;
}

export function currentDeliveryPoint(): DeliveryPoint {
  return cached ?? FALLBACK;
}

export async function saveDeliveryPoint(point: DeliveryPoint): Promise<void> {
  cached = point;
  await SecureStore.setItemAsync(KEY, JSON.stringify(point));
}
