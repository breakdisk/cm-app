import { getTrackingClient, ApiError } from './client';

export interface TrackingEventData {
  timestamp: string;
  status: string;
  description: string;
  location?: string;
  coordinates?: { lat: number; lng: number };
}

export interface TrackingResponse {
  awb: string;
  currentStatus: string;
  eta?: string;
  driverName?: string;
  driverPhone?: string;
  currentLocation?: { lat: number; lng: number };
  events: TrackingEventData[];
  lastUpdate: string;
}

export async function getTracking(awb: string): Promise<TrackingResponse> {
  try {
    const trackingClient = getTrackingClient();
    const response = await trackingClient.get<TrackingResponse>(`/v1/tracking/${awb}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function subscribeToTrackingUpdates(
  awb: string,
  callback: (data: TrackingResponse) => void
): Promise<() => void> {
  // Polling-based subscription implementation
  const interval = setInterval(async () => {
    try {
      const data = await getTracking(awb);
      callback(data);
    } catch (error) {
      console.error('Error fetching tracking update:', error);
    }
  }, 30000); // Poll every 30 seconds

  return () => clearInterval(interval);
}

// Legacy API for backward compatibility
export interface TrackingEvent {
  status: string;
  description: string;
  location?: string;
  occurred_at: string;
}

export interface PublicTrackingData {
  tracking_number: string;
  status: string;
  status_label?: string;
  current_status_label?: string;
  estimated_delivery?: string;
  eta?: string;
  origin?: string;
  origin_city?: string;
  destination?: string;
  destination_city?: string;
  history?: TrackingEvent[];
  events?: TrackingEvent[];
  driver_location?: { lat: number; lng: number };
  delivered_at?: string;
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
  getByTrackingNumber: (trackingNumber: string) => {
    const trackingClient = getTrackingClient();
    return trackingClient.get<PublicTrackingData>(
      `/track/${trackingNumber}`
    );
  },

  /** Get live tracking with driver location (requires auth) */
  getLive: (shipmentId: string, token: string) => {
    const trackingClient = getTrackingClient();
    return trackingClient.get<{ data: LiveTrackingData }>(`/v1/tracking/${shipmentId}`);
  },

  /** Request delivery reschedule */
  requestReschedule: (
    shipmentId: string,
    preferredDate: string,
    token: string
  ) => {
    const trackingClient = getTrackingClient();
    return trackingClient.post<{ rescheduled: boolean; new_eta?: string }>(
      `/v1/tracking/${shipmentId}/reschedule`,
      { preferred_date: preferredDate }
    );
  },

  /** Submit delivery feedback after successful delivery */
  submitFeedback: (
    shipmentId: string,
    rating: number,
    comment: string | undefined,
    token: string
  ) => {
    const trackingClient = getTrackingClient();
    return trackingClient.post<void>(`/v1/tracking/${shipmentId}/feedback`, {
      rating,
      comment,
    });
  },

  /** Customer confirms they received their package */
  confirmReceipt: (trackingNumber: string) => {
    const trackingClient = getTrackingClient();
    return trackingClient.post<{ confirmed: boolean; invoice_id?: string }>(
      `/v1/tracking/${trackingNumber}/confirm-receipt`,
      {}
    );
  },

  /** Customer requests their receipt to be emailed */
  sendReceiptByEmail: (trackingNumber: string, email: string) => {
    const trackingClient = getTrackingClient();
    return trackingClient.post<{ sent: boolean; email: string }>(
      `/v1/tracking/${trackingNumber}/send-receipt`,
      { email }
    );
  },
};
