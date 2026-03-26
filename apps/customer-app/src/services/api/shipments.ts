import { apiRequest } from "./client";

export interface BookingPayload {
  service_type: "standard" | "express" | "same_day" | "balikbayan";
  origin: {
    line1: string;
    city: string;
    province: string;
    postal_code: string;
  };
  destination: {
    line1: string;
    city: string;
    province: string;
    postal_code: string;
  };
  customer_name: string;
  customer_phone: string;
  weight_grams: number;
  declared_value?: number;
  cod_amount?: number;
  special_instructions?: string;
}

export interface Shipment {
  id: string;
  tracking_number: string;
  status: string;
  service_type: string;
  estimated_delivery?: string;
  created_at: string;
}

export const shipmentsApi = {
  /** Get the customer's shipment history */
  list: (
    params: { page?: number; per_page?: number; status?: string },
    token: string
  ) =>
    apiRequest<{ data: Shipment[]; total: number }>("/v1/shipments", {
      params,
      token,
    }),

  /** Get a single shipment */
  get: (id: string, token: string) =>
    apiRequest<{ data: Shipment }>(`/v1/shipments/${id}`, { token }),

  /** Book a new shipment */
  book: (payload: BookingPayload, token: string) =>
    apiRequest<{ data: Shipment }>("/v1/shipments", {
      method: "POST",
      body: payload,
      token,
    }),

  /** Cancel a shipment */
  cancel: (id: string, reason: string, token: string) =>
    apiRequest<void>(`/v1/shipments/${id}/cancel`, {
      method: "POST",
      body: { reason },
      token,
    }),
};
