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
export async function checkout(
  basketId: string,
  tipCents: number,
  lat: number,
  lng: number
): Promise<CheckoutResponse> {
  return apiFetch<CheckoutResponse>("/v1/omnideliv/orders/checkout", {
    method: "POST",
    body: JSON.stringify({
      basket_id: basketId,
      tip_cents: tipCents,
      delivery_lat: lat,
      delivery_lng: lng,
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
