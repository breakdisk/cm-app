import { getOrderClient, ApiError } from './client';

export interface CreateShipmentRequest {
  origin: string;
  destination: string;
  recipientName: string;
  recipientPhone: string;
  recipientEmail?: string;
  weight: number;
  description: string;
  cargoType: string;
  type: 'local' | 'international';
  serviceType: 'standard' | 'express' | 'nextday' | 'air' | 'sea';
  codAmount?: number;
}

export interface ShipmentResponse {
  awb: string;
  status: string;
  origin: string;
  destination: string;
  createdAt: string;
  fee: number;
  currency: string;
}

export interface ShipmentsListResponse {
  shipments: ShipmentResponse[];
  total: number;
  skip: number;
  limit: number;
}

export async function createShipment(customerId: string, request: CreateShipmentRequest): Promise<ShipmentResponse> {
  try {
    const orderClient = getOrderClient();
    const response = await orderClient.post<ShipmentResponse>('/v1/shipments', {
      customerId,
      ...request,
    });
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function getShipment(awb: string): Promise<ShipmentResponse> {
  try {
    const orderClient = getOrderClient();
    const response = await orderClient.get<ShipmentResponse>(`/v1/shipments/${awb}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function listShipments(
  customerId: string,
  { status, skip = 0, limit = 20 }: { status?: string; skip?: number; limit?: number }
): Promise<ShipmentsListResponse> {
  try {
    const orderClient = getOrderClient();
    const params = new URLSearchParams({
      customerId,
      skip: String(skip),
      limit: String(limit),
    });
    if (status) params.append('status', status);

    const response = await orderClient.get<ShipmentsListResponse>(`/v1/shipments?${params.toString()}`);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

export async function updateShipment(
  awb: string,
  updates: { status?: string; deliveryDate?: string }
): Promise<ShipmentResponse> {
  try {
    const orderClient = getOrderClient();
    const response = await orderClient.put<ShipmentResponse>(`/v1/shipments/${awb}`, updates);
    return response.data;
  } catch (error) {
    if (error instanceof ApiError) {
      throw new Error(error.message);
    }
    throw error;
  }
}

// Legacy API for backward compatibility
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
  ) => {
    const orderClient = getOrderClient();
    return orderClient.get<{ data: Shipment[]; total: number }>("/v1/shipments", { params });
  },

  /** Get a single shipment */
  get: (id: string, token: string) => {
    const orderClient = getOrderClient();
    return orderClient.get<{ data: Shipment }>(`/v1/shipments/${id}`);
  },

  /** Book a new shipment */
  book: (payload: BookingPayload, token: string) => {
    const orderClient = getOrderClient();
    return orderClient.post<{ data: Shipment }>("/v1/shipments", payload);
  },

  /** Cancel a shipment */
  cancel: (id: string, reason: string, token: string) => {
    const orderClient = getOrderClient();
    return orderClient.post<void>(`/v1/shipments/${id}/cancel`, { reason });
  },
};
