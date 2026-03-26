import { createApiClient, PaginatedApiResponse } from "./client";

export interface Shipment {
  id: string;
  tracking_number: string;
  status: ShipmentStatus;
  service_type: "standard" | "express" | "same_day" | "balikbayan";
  origin: Address;
  destination: Address;
  customer_name: string;
  customer_phone: string;
  declared_value?: number;
  cod_amount?: number;
  created_at: string;
  estimated_delivery?: string;
}

export type ShipmentStatus =
  | "pending"
  | "confirmed"
  | "pickup_assigned"
  | "picked_up"
  | "in_transit"
  | "at_hub"
  | "out_for_delivery"
  | "delivered"
  | "failed"
  | "cancelled"
  | "returned";

export interface Address {
  line1: string;
  line2?: string;
  city: string;
  province: string;
  postal_code: string;
  coordinates?: { lat: number; lng: number };
}

export interface CreateShipmentPayload {
  service_type: string;
  origin: Omit<Address, "coordinates">;
  destination: Omit<Address, "coordinates">;
  customer_name: string;
  customer_phone: string;
  customer_email?: string;
  weight_grams: number;
  declared_value?: number;
  cod_amount?: number;
  special_instructions?: string;
}

export const shipmentsApi = {
  list: (params: { page?: number; per_page?: number; status?: string; q?: string }, token: string) =>
    createApiClient(token)
      .get<PaginatedApiResponse<Shipment>>("/v1/shipments", { params })
      .then((r) => r.data),

  get: (id: string, token: string) =>
    createApiClient(token)
      .get<{ data: Shipment }>(`/v1/shipments/${id}`)
      .then((r) => r.data.data),

  create: (payload: CreateShipmentPayload, token: string) =>
    createApiClient(token)
      .post<{ data: Shipment }>("/v1/shipments", payload)
      .then((r) => r.data.data),

  cancel: (id: string, reason: string, token: string) =>
    createApiClient(token)
      .post<void>(`/v1/shipments/${id}/cancel`, { reason })
      .then((r) => r.data),

  bulkUpload: (file: File, token: string) => {
    const formData = new FormData();
    formData.append("file", file);
    return createApiClient(token)
      .post<{ data: { created: number; failed: number; errors: string[] } }>(
        "/v1/shipments/bulk",
        formData,
        { headers: { "Content-Type": "multipart/form-data" } }
      )
      .then((r) => r.data.data);
  },
};
