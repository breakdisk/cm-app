import { apiFetch } from "./client";

export interface TimelineEntry {
  event_type: string;
  /** Device clock where there was one — when it happened, not when we heard. */
  at: string;
  payload: Record<string, unknown>;
}

export interface LatLng {
  lat: number;
  lng: number;
}

export interface CourierFix extends LatLng {
  heading_deg: number | null;
  smoothed_speed_kph: number | null;
  age_seconds: number;
}

export interface EtaEstimate {
  low_minutes: number;
  high_minutes: number;
}

export interface StopView extends LatLng {
  vendor_name: string;
  picked_up: boolean;
}

export interface TrackResponse {
  order_id: string;
  status:
    | "placed"
    | "awaiting_courier"
    | "collecting"
    | "delivering"
    | "delivered"
    | "cancelled";
  grand_total_cents: number;
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
  stops_total: number;
  stops_collected: number;
  delivered_at: string | null;
  timeline: TimelineEntry[];
  /** Null unless the order is in motion and the fix is fresh. Never a stale point. */
  courier: CourierFix | null;
  eta: EtaEstimate | null;
  /** Null for orders placed before the destination was recorded. */
  destination: LatLng | null;
  stops: StopView[];
}

export function trackOrder(orderId: string): Promise<TrackResponse> {
  return apiFetch<TrackResponse>(`/v1/omnideliv/orders/${orderId}/track`);
}

/**
 * How often to poll, by state. After checkout the interesting events are
 * minutes apart, so a fixed one-second poll would spend most requests learning
 * nothing — and a delivered order needs no polling at all.
 */
export function pollIntervalMs(status: TrackResponse["status"]): number | null {
  switch (status) {
    case "delivered":
    case "cancelled":
      return null; // terminal — stop
    case "delivering":
      return 5_000; // the courier is moving; the map wants freshness
    default:
      return 15_000;
  }
}
