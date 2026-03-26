import { apiRequest } from "./client";

export interface TrackingEvent {
  status: string;
  description: string;
  location?: string;
  occurred_at: string;
}

export interface PublicTrackingData {
  tracking_number: string;
  status: string;
  current_status_label: string;
  eta?: string;
  origin_city: string;
  destination_city: string;
  events: TrackingEvent[];
  pod?: { delivered_at: string };
}

export interface LiveTrackingData extends PublicTrackingData {
  driver?: {
    name: string;
    lat: number;
    lng: number;
    heading?: number;
  };
}

export const trackingApi = {
  /** Track a shipment by tracking number (no auth — public) */
  getByTrackingNumber: (trackingNumber: string) =>
    apiRequest<{ data: PublicTrackingData }>(
      `/v1/tracking/public/${trackingNumber}`
    ),

  /** Get live tracking with driver location (requires auth) */
  getLive: (shipmentId: string, token: string) =>
    apiRequest<{ data: LiveTrackingData }>(`/v1/tracking/${shipmentId}`, {
      token,
    }),

  /** Request delivery reschedule */
  requestReschedule: (
    shipmentId: string,
    preferredDate: string,
    token: string
  ) =>
    apiRequest<{ rescheduled: boolean; new_eta?: string }>(
      `/v1/tracking/${shipmentId}/reschedule`,
      {
        method: "POST",
        body: { preferred_date: preferredDate },
        token,
      }
    ),

  /** Submit delivery feedback after successful delivery */
  submitFeedback: (
    shipmentId: string,
    rating: number,
    comment: string | undefined,
    token: string
  ) =>
    apiRequest<void>(`/v1/tracking/${shipmentId}/feedback`, {
      method: "POST",
      body: { rating, comment },
      token,
    }),
};
