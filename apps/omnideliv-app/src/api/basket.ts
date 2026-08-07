import { apiFetch } from "./client";

export interface BasketView {
  id: string;
  status: string;
  goods_total_cents: number;
  lines_awaiting_review: number;
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
  qty = 1
): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${basketId}/lines`, {
    method: "POST",
    body: JSON.stringify({ vendor_id: vendorId, item_id: itemId, qty }),
  });
}

export function removeLine(basketId: string, lineId: string): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${basketId}/lines/${lineId}`, {
    method: "DELETE",
  });
}
