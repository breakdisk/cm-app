/**
 * Maps the public tracking lookup (`GET /v1/tracking/public/:tracking_number`,
 * delivery-experience) into what TrackingScreen renders.
 *
 * Pulled out of the screen so search and the live refresh share one mapping.
 * They used to diverge: search mapped this response, while the "live"
 * subscription polled `/v1/tracking/:shipment_id` with an AWB in the id slot —
 * a route that never matched — and its result was never rendered anyway.
 */

export type ShipmentStatus =
  | "pending" | "confirmed" | "picked_up"
  | "in_transit" | "out_for_delivery"
  | "delivery_attempted" | "delivered" | "returned" | "cancelled";

export interface TimelineEvent {
  status:      ShipmentStatus;
  description: string;
  location?:   string;
  occurred_at: string;
}

export interface TrackingResult {
  awb:              string;
  status:           ShipmentStatus;
  origin_city:      string;
  destination_city: string;
  eta?:             string;
  driver_name?:     string;
  driver_phone?:    string;
  driver_location?: { lat: number; lng: number };
  timeline:         TimelineEvent[];
}

/** While the tracking screen is focused. At 30 seconds a moving vehicle reads as a frozen map. */
export const LIVE_TRACKING_POLL_MS = 5_000;

const TERMINAL_STATUSES: readonly string[] = ["delivered", "returned", "cancelled"];

/** Nothing left to watch — the live refresh stops. */
export function isTerminalStatus(status: string): boolean {
  return TERMINAL_STATUSES.includes(status);
}

/**
 * A coordinate pair, or undefined. The server sends `lat: null` when it has a
 * driver but no fix yet, and `Number(null)` is 0 — which would pin the driver
 * to 0,0 rather than hide the map.
 */
function toPoint(lat: unknown, lng: unknown): { lat: number; lng: number } | undefined {
  if (lat === null || lat === undefined || lng === null || lng === undefined) return undefined;
  const a = Number(lat);
  const b = Number(lng);
  return Number.isFinite(a) && Number.isFinite(b) ? { lat: a, lng: b } : undefined;
}

export function mapPublicTracking(body: any, fallbackAwb: string): TrackingResult {
  const data = body?.data ?? body ?? {};
  const driverLocation = data.driver_location
    ? toPoint(data.driver_location.lat, data.driver_location.lng)
    : toPoint(data.driver?.lat, data.driver?.lng);

  return {
    awb:              data.tracking_number ?? fallbackAwb,
    status:           (data.status ?? "pending") as ShipmentStatus,
    origin_city:      data.origin ?? data.origin_city ?? "",
    destination_city: data.destination ?? data.destination_city ?? "",
    eta:              data.estimated_delivery ?? data.eta,
    driver_name:      data.driver?.name,
    driver_phone:     undefined,
    driver_location:  driverLocation,
    timeline: (data.history ?? data.events ?? []).map((e: any) => ({
      status:      (e.status ?? "pending") as ShipmentStatus,
      description: e.description ?? e.status_label ?? "",
      location:    e.location,
      occurred_at: e.occurred_at ?? e.timestamp ?? "",
    })),
  };
}
