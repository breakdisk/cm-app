import { API_BASE, apiFetch } from "./client";

/** One choice inside a group — "Large", "Extra shot". */
export interface ModifierOption {
  id: string;
  name: string;
  /** Added to the base price when chosen. Signed: negative is a discount. */
  price_delta_cents: number;
}

/** A set of choices offered against an item. */
export interface ModifierGroup {
  id: string;
  name: string;
  /** 0 means the group can be skipped. */
  min_select: number;
  /** 1 is pick-one; more allows several. */
  max_select: number;
  options: ModifierOption[];
}

export interface SearchHit {
  item_id: string;
  /** Needed to build the public photo URL; the app holds a slug, not this id. */
  tenant_id: string;
  name: string;
  price_cents: number;
  /** Whether a photo exists. Not a URL — see `itemPhotoUrl`. */
  has_photo: boolean;
  availability: "available" | "limited" | "out_of_stock";
  /** Why a substitute was proposed — surfaced so the UI can explain itself. */
  warrants_substitute: boolean;
  /** Choices to make before this can be added. Empty for most items.
   *  Older responses omit the field entirely; treat it as empty. */
  modifiers?: ModifierGroup[];
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

export interface VendorSummary {
  id: string;
  name: string;
  address: string;
  prep_time_minutes: number;
}

/** Orderable vendors of a vertical near the customer, nearest first. */
export function vendorsNear(
  vertical: string,
  lat: number,
  lng: number,
  radiusKm = 5
): Promise<VendorSummary[]> {
  const params = new URLSearchParams({
    vertical,
    lat: String(lat),
    lng: String(lng),
    radius_km: String(radiusKm),
  });
  return apiFetch<VendorSummary[]>(`/v1/omnideliv/vendors?${params.toString()}`);
}

/**
 * Public URL for an item's photo.
 *
 * Unauthenticated on purpose: an <Image> cannot carry a bearer token, and a
 * product photo is what someone looks at *before* they have any relationship
 * with the shop. Derived rather than stored, so a moved backing store does not
 * strand links in cached responses.
 */
export function itemPhotoUrl(tenantId: string, itemId: string): string {
  return `${API_BASE}/v1/omnideliv/public/catalog/${tenantId}/items/${itemId}/photo`;
}
