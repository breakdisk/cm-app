import { createApiClient, ApiResponse } from "./client";

export interface TrackingEvent {
  id: string;
  shipment_id: string;
  status: string;
  description: string;
  location?: string;
  driver_name?: string;
  occurred_at: string;
}

export interface LiveTrackingData {
  shipment_id: string;
  tracking_number: string;
  status: string;
  current_status_label: string;
  eta?: string;
  driver?: {
    name: string;
    phone: string;
    lat: number;
    lng: number;
    heading?: number;
  };
  events: TrackingEvent[];
  pod?: {
    signature_url?: string;
    photo_urls: string[];
    delivered_at: string;
    recipient_name?: string;
  };
}

export interface TrackingPublicView {
  tracking_number: string;
  status: string;
  current_status_label: string;
  eta?: string;
  origin_city: string;
  destination_city: string;
  events: Array<{
    status: string;
    description: string;
    occurred_at: string;
  }>;
  pod?: {
    delivered_at: string;
  };
}

export interface RescheduleRequest {
  preferred_date: string;
  preferred_time_slot?: "morning" | "afternoon" | "evening";
  special_instructions?: string;
}

export const trackingApi = {
  /** Get full live tracking data for a shipment (auth required) */
  getLiveTracking: (shipmentId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<LiveTrackingData>>(`/v1/tracking/${shipmentId}`)
      .then((r) => r.data.data),

  /** Get public tracking view by tracking number (no auth) */
  getPublic: (trackingNumber: string) =>
    createApiClient()
      .get<ApiResponse<TrackingPublicView>>(
        `/v1/tracking/public/${trackingNumber}`
      )
      .then((r) => r.data.data),

  /** Get tracking event history for a shipment */
  getEvents: (shipmentId: string, token: string) =>
    createApiClient(token)
      .get<ApiResponse<TrackingEvent[]>>(`/v1/tracking/${shipmentId}/events`)
      .then((r) => r.data.data),

  /** Customer requests delivery reschedule from tracking page */
  requestReschedule: (
    shipmentId: string,
    payload: RescheduleRequest,
    token: string
  ) =>
    createApiClient(token)
      .post<ApiResponse<{ rescheduled: boolean; new_eta?: string }>>(
        `/v1/tracking/${shipmentId}/reschedule`,
        payload
      )
      .then((r) => r.data.data),

  /** Submit delivery feedback (NPS + comment) */
  submitFeedback: (
    shipmentId: string,
    payload: { rating: number; comment?: string; tags?: string[] },
    token: string
  ) =>
    createApiClient(token)
      .post<void>(`/v1/tracking/${shipmentId}/feedback`, payload)
      .then((r) => r.data),
};
