import { apiFetch } from "./client";

/** One thing the mesh's verification found while checking proposed lines. */
export interface BasketConflict {
  /** Opaque to the app — the mesh owns this enum and will add variants. */
  kind: unknown;
  /** The line is already gone. Phrase it as done, not as a decision. */
  blocking: boolean;
  description: string;
}

/** A chosen option, already priced into the line's unit price. */
export interface SelectedModifier {
  group_id: string;
  group_name: string;
  option_id: string;
  option_name: string;
  price_delta_cents: number;
}

export interface BasketLineView {
  id: string;
  item_id: string;
  vendor_id: string;
  /** "Item no longer listed" when the catalog row is gone — the line still
   *  renders and can still be removed. */
  name: string;
  qty: number;
  /** Includes modifier deltas. `subtotal_cents` is this × qty. */
  unit_price_cents: number;
  subtotal_cents: number;
  /** `substituted` is what blocks checkout and what the review screen must
   *  surface; the rest are informational. */
  state: string;
  modifiers: SelectedModifier[];
}

export interface BasketView {
  id: string;
  status: string;
  goods_total_cents: number;
  lines_awaiting_review: number;
  /** Empty for a manually built basket — nothing proposed it, so nothing was
   *  verified. Older responses omit the field entirely; treat it as empty. */
  conflicts?: BasketConflict[];
  /** What is in the basket. Older responses omit it; treat as empty. */
  lines?: BasketLineView[];
}

export function createBasket(): Promise<BasketView> {
  // No body: tenant and customer come from the JWT. A client that could name
  // the customer could open a basket in someone else's name.
  return apiFetch<BasketView>("/v1/omnideliv/baskets", { method: "POST" });
}

export function getBasket(id: string): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${id}`);
}

/**
 * Add a catalog item by hand — the path that works with the mesh switched off.
 *
 * Deliberately sends no price: the server reads it from the catalog. A
 * client-supplied price is a client-supplied discount.
 */
export function addLine(
  basketId: string,
  vendorId: string,
  itemId: string,
  qty = 1,
  /** Chosen modifier option ids. Ids only — the server reads the deltas from
   *  the catalog, for the same reason it reads the base price there. */
  modifiers: string[] = []
): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${basketId}/lines`, {
    method: "POST",
    body: JSON.stringify({ vendor_id: vendorId, item_id: itemId, qty, modifiers }),
  });
}

export function removeLine(basketId: string, lineId: string): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${basketId}/lines/${lineId}`, {
    method: "DELETE",
  });
}
