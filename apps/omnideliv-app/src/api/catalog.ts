import { apiFetch } from "./client";

export interface SearchHit {
  item_id: string;
  name: string;
  price_cents: number;
  availability: "available" | "limited" | "out_of_stock";
  /** Why a substitute was proposed — surfaced so the UI can explain itself. */
  warrants_substitute: boolean;
}

export function searchCatalog(
  vendorId: string,
  query: string,
  avoid: string[] = [],
  limit = 20
): Promise<SearchHit[]> {
  const params = new URLSearchParams({
    vendor_id: vendorId,
    q: query,
    limit: String(limit),
  });
  if (avoid.length > 0) params.set("avoid", avoid.join(","));

  // Note: no tenant_id parameter. The server reads it from the JWT — a
  // client-supplied tenant would be a cross-tenant read.
  return apiFetch<SearchHit[]>(`/v1/omnideliv/catalog/search?${params.toString()}`);
}
