import { ApiError, apiFetch } from "./client";

/** How the customer intends to pay. `cod` is the server default. */
export type PaymentMethod = "cod" | "online";

/** Where an online order's authorization hold stands. Always `pending` for COD. */
export type PaymentStatus = "pending" | "authorized" | "captured" | "voided" | "failed";

export interface CheckoutResponse {
  order_id: string;
  grand_total_cents: number;
  stops: number;
  /**
   * Present only for `payment_method: "online"`. The hosted card page the
   * customer must complete before a courier is ever offered the job — so an
   * online checkout that ignores this leaves an order nobody will ever deliver
   * and that the server cancels ~30 minutes later.
   */
  checkout_url?: string | null;
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
/**
 * `paymentMethod` decides the whole shape of what happens next, and the two
 * are not variations on one flow:
 *
 *  - `cod` — the courier is offered the job inside this request. The response
 *    is the order, and there is nothing left to do.
 *  - `online` — no courier is offered yet. The response carries a
 *    `checkout_url`; the job is only broadcast once the authorization actually
 *    lands, and the hold is released if nobody takes it.
 */
export async function checkout(
  basketId: string,
  tipCents: number,
  lat: number,
  lng: number,
  deliveryNote?: string,
  paymentMethod: PaymentMethod = "cod"
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
      payment_method: paymentMethod,
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
  /**
   * Without these the list cannot tell an order waiting on a courier from one
   * waiting on the customer to finish paying — both read as `status: "placed"`.
   */
  payment_method: PaymentMethod;
  payment_status: PaymentStatus;
  prepaid_amount_cents: number;
  /**
   * What the courier still collects at the door. Equal to the grand total for
   * COD, `0` for a fully prepaid order. Server-computed so no screen re-derives
   * it — see `cashDue` in `money.ts` for the one that would otherwise tell a
   * customer who has already paid to find cash.
   */
  cod_amount_cents: number;
  placed_at: string;
  delivered_at: string | null;
}

/** An order the customer started paying for online and never finished. */
export function isAwaitingPayment(o: {
  payment_method: PaymentMethod;
  payment_status: PaymentStatus;
  status: string;
}): boolean {
  return (
    o.payment_method === "online" &&
    o.payment_status === "pending" &&
    o.status !== "cancelled" &&
    o.status !== "delivered"
  );
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
