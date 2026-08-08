/**
 * Where the customer is.
 *
 * One definition, because three different callers need it and they must agree:
 * browsing centres its vendor list here, a mesh run centres every specialist's
 * search here, and checkout offers the job to couriers here. Three copies of a
 * "default location" constant drift, and the symptom is subtle — the shops you
 * browsed are not the shops the agent found, and the courier is offered a
 * pickup somewhere else again.
 *
 * SLICE ONE PLACEHOLDER. This is a fixed point in Manila. Replacing it with the
 * device's real location (expo-location) or a saved address is a product
 * decision about permissions and address management, not a code gap — but note
 * that everything downstream already carries the point per request, so the only
 * change needed here is what this function returns.
 */
export interface DeliveryPoint {
  lat: number;
  lng: number;
}

const MANILA: DeliveryPoint = { lat: 14.5995, lng: 120.9842 };

export function currentDeliveryPoint(): DeliveryPoint {
  return MANILA;
}
