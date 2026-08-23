import { ApiError, apiFetch } from "./client";

export interface CheckoutResponse {
  order_id: string;
  grand_total_cents: number;
  stops: number;
}

/**
 * Place the order. The tip is the only money the client chooses — goods prices
 * and the delivery fee are computed server-side.
 *
 * A 409 means the basket still has swaps awaiting a decision, which is the cue
 * to keep the customer on the review screen rather than an error to show.
 * A 503 means no courier could be found: nothing was charged, so retrying later
 * is safe and the basket is still good.
 */
/**
 * `deliveryNote` is the customer's instruction to the courier — "unit 12B, gate
 * code 4417". An order carries no street address, only a point, so this is the
 * only place anyone can say where the door actually is.
 *
 * Sent as `null` when blank rather than `""`: the server treats both as absent,
 * and an empty string would render a blank line on the courier's manifest that
 * looks like a rendering fault.
 */
export async function checkout(
  basketId: string,
  tipCents: number,
  lat: number,
  lng: number,
  deliveryNote?: string
): Promise<CheckoutResponse> {
  const note = deliveryNote?.trim();
  return apiFetch<CheckoutResponse>("/v1/omnideliv/orders/checkout", {
    method: "POST",
    body: JSON.stringify({
      basket_id: basketId,
      tip_cents: tipCents,
      delivery_lat: lat,
      delivery_lng: lng,
      delivery_note: note ? note : null,
    }),
  });
}

export type CheckoutFailure = "awaiting_review" | "no_courier" | "rejected" | "unknown";

/** Classify a checkout failure so the UI can respond rather than just apologise. */
export function classifyCheckoutError(e: unknown): CheckoutFailure {
  if (!(e instanceof ApiError)) return "unknown";
  if (e.status === 409) return "awaiting_review";
  if (e.status === 503) return "no_courier";
  if (e.status === 400 || e.status === 404) return "rejected";
  return "unknown";
}

export interface OrderListItem {
  order_id: string;
  status: string;
  grand_total_cents: number;
  goods_total_cents: number;
  delivery_fee_cents: number;
  tip_cents: number;
  stops_total: number;
  /** Comma-joined shop names. Empty if an order somehow has no legs. */
  vendor_names: string;
  placed_at: string;
  delivered_at: string | null;
}

/**
 * The signed-in customer's orders, newest first.
 *
 * The customer is taken from the token server-side — there is no parameter
 * here that could name someone else's history.
 *
 * Without this an order was unreachable the moment the app closed: checkout
 * hands you a tracking screen and nothing ever linked back to it again.
 */
export async function listMyOrders(): Promise<OrderListItem[]> {
  return apiFetch<OrderListItem[]>("/v1/omnideliv/orders");
}
