/**
 * Shipment API client — wraps all shipment management endpoints.
 *
 * All paths route through the API gateway:
 *   /v1/shipments  → order-intake service
 *
 * Uses the shared Axios client which auto-attaches the freshest JWT from the
 * __los_at cookie via the request interceptor.
 */
import { createApiClient } from "./client";

export interface AddressDto {
  line1: string;
  line2?: string | null;
  barangay?: string | null;
  city: string;
  province: string;
  postal_code: string;
  country_code: string;
  coordinates?: { lat: number; lng: number } | null;
}

export interface ShipmentDto {
  id: string;
  tenant_id: string;
  merchant_id: string;
  customer_id: string;
  customer_name: string;
  customer_phone: string;
  customer_email?: string | null;
  awb: string;
  piece_count: number;
  status: string;
  service_type: string;
  origin: AddressDto;
  destination: AddressDto;
  weight: { grams: number };
  cod_amount?: { amount: number; currency: string } | null;
  declared_value?: { amount: number; currency: string } | null;
  special_instructions?: string | null;
  created_at: string;
  updated_at: string;
  booked_by_customer: boolean;
  auto_dispatch: boolean;
}

export interface RescheduleRequest {
  preferred_date: string;             // "YYYY-MM-DD"
  preferred_time_slot?: string | null; // "morning" | "afternoon" | "anytime"
  reason: string;
}

export interface OverrideStatusRequest {
  status: string;
  reason?: string;
}

export interface ListShipmentsResult {
  shipments: ShipmentDto[];
  total: number;
  page: number;
  per_page: number;
}

/** List shipments with optional filters and pagination. */
export const listShipments = (params?: {
  status?: string;
  q?: string;
  page?: number;
  per_page?: number;
}) =>
  createApiClient()
    .get<ListShipmentsResult>("/v1/shipments", { params })
    .then((r) => r.data);

/** Get a single shipment by ID. */
export const getShipment = (id: string) =>
  createApiClient()
    .get<ShipmentDto>(`/v1/shipments/${id}`)
    .then((r) => r.data);

/** Cancel a shipment (Pending/Confirmed only). */
export const cancelShipment = (id: string, reason: string) =>
  createApiClient()
    .post<void>(`/v1/shipments/${id}/cancel`, { reason })
    .then(() => undefined);

/** Reschedule a failed/attempted delivery. */
export const rescheduleShipment = (id: string, req: RescheduleRequest) =>
  createApiClient()
    .post<void>(`/v1/shipments/${id}/reschedule`, req)
    .then(() => undefined);

/** Admin: override shipment status directly (requires SHIPMENT_UPDATE). */
export const overrideShipmentStatus = (id: string, req: OverrideStatusRequest) =>
  createApiClient()
    .put<void>(`/v1/shipments/${id}/status`, req)
    .then(() => undefined);
