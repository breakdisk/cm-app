import { createApiClient, ApiResponse } from "./client";

export interface PickupAddress {
  id: string;
  user_id: string;
  tenant_id: string;
  label: string;
  address: string;
  city: string;
  province: string;
  postal_code: string;
  country: string;
  lat: number | null;
  lng: number | null;
  is_default: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreatePickupAddressPayload {
  label: string;
  address: string;
  city?: string;
  province?: string;
  postal_code?: string;
  country?: string;
  lat?: number;
  lng?: number;
}

export const addressesApi = {
  async list(): Promise<PickupAddress[]> {
    const client = createApiClient();
    const res = await client.get<ApiResponse<PickupAddress[]>>("/v1/pickup-addresses");
    return res.data.data;
  },

  async create(payload: CreatePickupAddressPayload): Promise<PickupAddress> {
    const client = createApiClient();
    const res = await client.post<ApiResponse<PickupAddress>>("/v1/pickup-addresses", payload);
    return res.data.data;
  },

  async setDefault(id: string): Promise<void> {
    const client = createApiClient();
    await client.patch(`/v1/pickup-addresses/${id}/default`);
  },

  async remove(id: string): Promise<void> {
    const client = createApiClient();
    await client.delete(`/v1/pickup-addresses/${id}`);
  },
};
