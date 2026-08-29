/**
 * The customer's money, as arithmetic.
 *
 * There is no balance here and there is not meant to be. The only real numbers
 * are what is owed at the door and what has already been handed over. A
 * customer stored-value wallet is a regulated product, not a screen.
 *
 * Not every order is cash on delivery any more — an order can be paid by card,
 * or split between the two — so "what is owed at the door" is
 * `cod_amount_cents` and never the grand total. That distinction is the whole
 * reason `cashDue` filters on an amount rather than only on a status.
 */
import type { OrderListItem } from "./api/orders";

/** Statuses where the courier has not yet been paid at the door. */
const IN_FLIGHT = ["placed", "awaiting_courier", "collecting", "delivering"];

export function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

export interface Rollup {
  cents: number;
  count: number;
}

/**
 * What has actually been spent this calendar month.
 *
 * Delivered only. Counting an in-flight order would tell someone they have
 * spent money they are still holding, and counting a cancelled one would be
 * simply false.
 */
export function monthToDate(orders: OrderListItem[]): Rollup {
  const now = new Date();
  return orders.reduce<Rollup>(
    (acc, o) => {
      if (o.status !== "delivered") return acc;
      const at = new Date(o.placed_at);
      if (at.getMonth() !== now.getMonth() || at.getFullYear() !== now.getFullYear()) return acc;
      return { cents: acc.cents + o.grand_total_cents, count: acc.count + 1 };
    },
    { cents: 0, count: 0 },
  );
}

/**
 * The order whose cash is still owed, if any.
 *
 * The list arrives newest first, so the first match is the most recent — which
 * is the one someone is about to answer the door for.
 *
 * `cod_amount_cents > 0` is doing real work here, not defensive filtering: a
 * card-paid order is in flight exactly like a cash one, and without this the
 * panel would tell someone who has already paid to have the full amount ready
 * at the door.
 */
export function cashDue(orders: OrderListItem[]): OrderListItem | null {
  return (
    orders.find((o) => IN_FLIGHT.includes(o.status) && o.cod_amount_cents > 0) ?? null
  );
}
