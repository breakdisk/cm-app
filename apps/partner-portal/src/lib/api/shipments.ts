import { createApiClient } from "./client";

// Mirrors the order-intake `Shipment` entity (services/order-intake/src/domain/
// entities/shipment.rs) as returned by `GET /v1/shipments`. Typed ids are
// newtypes over Uuid, which serde writes as bare strings. Only the fields this
// portal reads are declared.

export type ShipmentStatus =
  | "pending" | "confirmed" | "pickup_assigned" | "picked_up" | "in_transit"
  | "at_hub" | "out_for_delivery" | "delivery_attempted" | "delivered"
  | "partial_delivery" | "piece_exception" | "customs_hold" | "failed"
  | "cancelled" | "returned";

export interface ShipmentAddress {
  line1: string;
  line2?: string | null;
  city: string;
  province: string;
  country_code: string;
}

export interface ShipmentMoney {
  amount: number; // smallest currency unit
  currency: string;
}

export interface Shipment {
  id: string;
  awb: string;
  status: ShipmentStatus;
  /** Rust enum variant name, e.g. "Standard", "Balikbayan", "International". */
  service_type: string;
  customer_name: string;
  origin: ShipmentAddress;
  destination: ShipmentAddress;
  weight: { grams: number };
  declared_value?: ShipmentMoney | null;
  cod_amount?: ShipmentMoney | null;
  special_instructions?: string | null;
  created_at: string;
  updated_at: string;
}

/** The response of `GET /v1/shipments`: one page and the unpaged total. */
export interface ShipmentList {
  shipments: Shipment[];
  total: number;
}

export interface ListShipmentsParams {
  /** Exact shipment status filter. */
  status?: ShipmentStatus;
  /** 1-based. */
  page?: number;
  /** Server clamps to 1..=100, default 20. */
  perPage?: number;
}

export const shipmentsApi = {
  /**
   * The caller's shipments. The server scopes this to the partner the token is
   * bound to; the portal never sends a partner or merchant id.
   */
  async list(params: ListShipmentsParams = {}): Promise<ShipmentList> {
    const { data } = await createApiClient().get<Partial<ShipmentList>>("/v1/shipments", {
      params: { status: params.status, page: params.page, per_page: params.perPage },
    });
    return { shipments: data.shipments ?? [], total: data.total ?? 0 };
  },
};
