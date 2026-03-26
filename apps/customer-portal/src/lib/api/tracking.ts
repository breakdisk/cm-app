/**
 * Customer Portal API — branded delivery tracking experience.
 * Public-facing: most calls are unauthenticated (tracking number lookup).
 */

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

async function apiGet<T>(path: string): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
  });
  if (!response.ok) {
    throw { status: response.status };
  }
  return response.json();
}

async function apiPost<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    let err: { error?: { message?: string } } = {};
    try { err = await response.json(); } catch { /* ignore */ }
    throw { status: response.status, message: err?.error?.message };
  }
  if (response.status === 204) return undefined as unknown as T;
  return response.json();
}

export interface TrackingEvent {
  status: string;
  description: string;
  location?: string;
  occurred_at: string;
}

export interface TrackingData {
  tracking_number: string;
  status: string;
  current_status_label: string;
  eta?: string;
  origin_city: string;
  destination_city: string;
  merchant_name?: string;
  events: TrackingEvent[];
  pod?: {
    delivered_at: string;
    photo_urls?: string[];
  };
}

export const trackingApi = {
  /** Look up a shipment by tracking number (public, no auth) */
  track: (trackingNumber: string) =>
    apiGet<{ data: TrackingData }>(
      `/v1/tracking/public/${trackingNumber}`
    ).then((r) => r.data),

  /** Request a delivery reschedule from the tracking page */
  reschedule: (
    shipmentId: string,
    preferredDate: string,
    preferredSlot?: "morning" | "afternoon" | "evening"
  ) =>
    apiPost<{ rescheduled: boolean; new_eta?: string }>(
      `/v1/tracking/${shipmentId}/reschedule`,
      { preferred_date: preferredDate, preferred_time_slot: preferredSlot }
    ),

  /** Submit post-delivery feedback (NPS) */
  submitFeedback: (
    shipmentId: string,
    rating: number,
    comment?: string,
    tags?: string[]
  ) =>
    apiPost<void>(`/v1/tracking/${shipmentId}/feedback`, {
      rating,
      comment,
      tags,
    }),
};
