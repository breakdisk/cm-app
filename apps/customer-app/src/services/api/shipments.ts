/**
 * Shipments API service — wraps the order-intake service (port 8004).
 * Request shapes match CreateShipmentCommand in services/order-intake.
 */
import { getOrderClient } from './client';

// ── Request types (match order-intake CreateShipmentCommand) ──────────────────

export interface PieceInput {
  weight_grams: number;
  length_cm?: number;
  width_cm?: number;
  height_cm?: number;
  description?: string;
}

export interface AddressInput {
  line1: string;
  line2?: string;
  barangay?: string;
  city: string;
  province: string;
  postal_code: string;
  country_code: string; // "PH", "AE", etc.
}

export interface CreateShipmentRequest {
  customer_name: string;
  customer_phone: string;
  customer_email?: string;
  origin: AddressInput;
  destination: AddressInput;
  service_type: 'standard' | 'express' | 'same_day' | 'balikbayan';
  weight_grams: number;
  length_cm?: number;
  width_cm?: number;
  height_cm?: number;
  declared_value_cents?: number;
  cod_amount_cents?: number;
  description?: string;
  special_instructions?: string;
  merchant_reference?: string;
  piece_count?: number;
  pieces?: PieceInput[];
  quote_token?: string;
  idempotency_key?: string;
}

// ── Response types ─────────────────────────────────────────────────────────────

export interface ShipmentResponse {
  id: string;
  awb: string;
  tracking_number: string;
  status: string;
  service_type: string;
  origin: AddressInput;
  destination: AddressInput;
  customer_name: string;
  customer_phone: string;
  weight_grams: number;
  cod_amount_cents?: number;
  declared_value_cents?: number;
  estimated_delivery?: string;
  created_at: string;
  // Computed fields added by the app
  fee?: number;
  currency?: string;
  payment_status?: 'not_required' | 'awaiting_payment' | 'paid' | 'payment_failed';
  checkout_url?: string;
}

export interface ShipmentsListResponse {
  shipments: ShipmentResponse[];
  total: number;
}

// ── API calls ──────────────────────────────────────────────────────────────────

export async function createShipment(request: CreateShipmentRequest): Promise<ShipmentResponse> {
  const client = getOrderClient();
  const response = await client.post<ShipmentResponse>('/v1/shipments', request);
  return response.data;
}

export async function getShipment(id: string): Promise<ShipmentResponse> {
  const client = getOrderClient();
  const response = await client.get<ShipmentResponse>(`/v1/shipments/${id}`);
  return response.data;
}

export async function listShipments(
  params: { status?: string; skip?: number; limit?: number } = {}
): Promise<ShipmentsListResponse> {
  const client = getOrderClient();
  const response = await client.get<ShipmentsListResponse>('/v1/shipments', {
    params: {
      skip: params.skip ?? 0,
      limit: params.limit ?? 20,
      ...(params.status ? { status: params.status } : {}),
    },
  });
  return response.data;
}

export async function cancelShipment(id: string, reason: string): Promise<void> {
  const client = getOrderClient();
  await client.post(`/v1/shipments/${id}/cancel`, { reason });
}

// ── Legacy shape used by BookingScreen ────────────────────────────────────────
// BookingScreen collects flat strings; this helper maps them to AddressInput.

export function parseAddress(flat: string, countryCode = 'PH'): AddressInput {
  const parts = flat.split(',').map(s => s.trim());
  return {
    line1:        parts[0] ?? flat,
    city:         parts[1] ?? 'Unknown',
    province:     parts[2] ?? 'Unknown',
    postal_code:  parts[3] ?? '0000',
    country_code: countryCode,
  };
}

// ── Quote (AE-region only) ──────────────────────────────────────────────────

export interface QuotePieceInput {
  weight_grams: number;
}

export interface QuoteRequest {
  service_type: 'standard' | 'express' | 'same_day' | 'balikbayan';
  weight_grams: number;
  pieces?: QuotePieceInput[];
}

export interface QuoteResponse {
  amount_cents: number;
  currency: string;
  quote_token: string;
  expires_at: string;
}

export async function getShipmentQuote(request: QuoteRequest): Promise<QuoteResponse> {
  const client = getOrderClient();
  const response = await client.post<QuoteResponse>('/v1/shipments/quote', request);
  return response.data;
}
