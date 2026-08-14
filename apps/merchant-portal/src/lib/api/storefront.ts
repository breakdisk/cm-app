/**
 * OmniDeliv storefront — merchant-portal client.
 *
 * The merchant portal serves two different kinds of business. A parcel merchant
 * ships goods they sold elsewhere; an OmniDeliv vendor *is* the shop, and their
 * catalog lives here. Only the second kind has a storefront, so the nav item is
 * gated on `useHasStorefront` rather than shown to everyone and explained away
 * with an empty state — a parcel merchant clicking "Storefront" and being told
 * their login is not linked to a store is a navigation bug wearing the costume
 * of an account problem.
 *
 * Hiding the tab is not access control. `GET /catalog/mine` resolves the store
 * from the JWT and 404s for anyone who runs none; that is the enforcement, and
 * it does not move because a menu entry disappeared.
 */
import { useCallback, useEffect, useState } from "react";

import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

export type Availability = "available" | "limited" | "out_of_stock";

/** Where a row's facts came from. `manual` is the only one with a human author. */
export type CatalogSource = "manual" | "shopify" | "woocommerce" | "csv" | "pos";

export interface Item {
  id: string;
  name: string;
  sku: string;
  description: string | null;
  price_cents: number;
  allergens: string[];
  is_listed: boolean;
  availability: Availability;
  /** `null` = nobody has ever confirmed this. Not the same as "confirmed long ago". */
  confirmed_at: string | null;
  source: CatalogSource;
  synced_at: string | null;
  /** False = nobody has stated what is in this. See the storefront page. */
  allergens_declared: boolean;
  /** Whether a photo has been uploaded. Not a URL — see `photoUrl`. */
  has_photo: boolean;
  /** `null` = uncategorised, which an import legitimately produces. */
  category: string | null;
  warrants_substitute: boolean;
}

export interface Catalog {
  tenant_id: string;
  vendor_id: string;
  vendor_name: string;
  items: Item[];
}

export interface IngestReport {
  created: number;
  updated: number;
  rejected: number;
}

export interface ItemInput {
  sku: string;
  name: string;
  description?: string | null;
  price_cents: number;
  /** Omit for "not stated"; `[]` is the real declaration "contains none of these". */
  allergens?: string[];
  dietary_tags?: string[];
  /** "Mains", "Beverages"… `null` clears it; omit to leave it unchanged. */
  category?: string | null;
}

async function expectOk(res: Response, what: string): Promise<void> {
  if (res.ok) return;
  // Handlers answer 400 with a plain-text reason ("this store already has an
  // item with SKU X"). Surfacing it beats "request failed", which tells a
  // vendor nothing about the duplicate they just typed.
  const detail = await res.text().catch(() => "");
  throw new Error(detail?.trim() || `${what}: ${res.status}`);
}

/** The five OmniDeliv verticals the backend accepts. */
export const VERTICALS = [
  { value: "restaurant", label: "Restaurant" },
  { value: "grocery",    label: "Grocery" },
  { value: "pharmacy",   label: "Pharmacy" },
  { value: "florist",    label: "Florist" },
  { value: "retail",     label: "Retail" },
] as const;

export interface VendorApplication {
  vertical: string;
  name:     string;
  address:  string;
  lat:      number;
  lng:      number;
}

export interface VendorProfile {
  id:                string;
  name:              string;
  address:           string;
  prep_time_minutes: number | null;
  status:            string;
}

/**
 * A shop connection, as the connectors service reports it.
 *
 * `webhook_url` is generated server-side and is what the merchant pastes into
 * Shopify/WooCommerce so order events reach us. It is returned on list, not on
 * create, so the panel below reloads after connecting rather than guessing it.
 */
export interface ShopConnection {
  id:          string;
  platform:    string;
  is_active:   boolean;
  webhook_url: string;
  created_at:  string;
}

/** One credential input. `secret` masks it; absent means a plain text field. */
export interface ShopField {
  key:          string;
  label:        string;
  placeholder:  string;
  secret?:      boolean;
}

export interface ShopPlatform {
  value:  string;
  label:  string;
  fields: ShopField[];
}

/** What each platform needs before a sync can run. */
export const SHOP_PLATFORMS: ShopPlatform[] = [
  {
    value:  "shopify",
    label:  "Shopify",
    fields: [
      { key: "shop_domain",     label: "Shop domain",     placeholder: "my-store.myshopify.com" },
      { key: "admin_api_token", label: "Admin API token", placeholder: "shpat_…", secret: true },
    ],
  },
  {
    value:  "woocommerce",
    label:  "WooCommerce",
    fields: [
      { key: "store_url",       label: "Store URL",       placeholder: "https://mystore.com" },
      { key: "consumer_key",    label: "Consumer key",    placeholder: "ck_…" },
      { key: "consumer_secret", label: "Consumer secret", placeholder: "cs_…", secret: true },
    ],
  },
];

export const connectorsApi = {
  async list(): Promise<ShopConnection[]> {
    const res = await authFetch(`${API_BASE}/v1/connectors/credentials`);
    if (res.status === 404) return [];
    await expectOk(res, "connections");
    return res.json();
  },

  /**
   * Connect a shop.
   *
   * `omnideliv_vendor_id` is passed rather than asked for. The connector is
   * keyed on (tenant, merchant, platform) and a storefront is a separate
   * object, so something has to associate the two — and this page is the one
   * place that already knows both. Without it a sync has no storefront to land
   * in and quietly imports nothing.
   */
  async connect(input: {
    platform:       string;
    webhook_secret: string;
    config:         Record<string, string>;
    vendorId:       string;
  }): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/connectors/credentials`, {
      method:  "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        platform:       input.platform,
        webhook_secret: input.webhook_secret,
        config: { ...input.config, omnideliv_vendor_id: input.vendorId },
      }),
    });
    await expectOk(res, "connect");
  },

  async disconnect(platform: string): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/connectors/credentials/${platform}`, {
      method: "DELETE",
    });
    await expectOk(res, "disconnect");
  },
};

export const storefrontApi = {
  /**
   * Apply to run a store. Idempotent server-side: a login that already has a
   * vendor gets that vendor back rather than a second one.
   *
   * The new store lands in `onboarding`, not `active` — an operator approves
   * it before customers can see it. The catalog is editable immediately, so
   * the vendor can have their menu ready when that happens.
   */
  async apply(input: VendorApplication): Promise<VendorProfile> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/apply`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    });
    await expectOk(res, "apply");
    return res.json();
  },

  /**
   * Public URL for an item's photo.
   *
   * Built rather than stored: the server returns `has_photo`, not a link. A
   * stored URL goes stale the moment the backing store moves, and this one is
   * derivable from ids the console already holds.
   *
   * Unauthenticated by design — an <img> tag cannot send a bearer token.
   */
  photoUrl(tenantId: string, itemId: string): string {
    return `${API_BASE}/v1/omnideliv/public/catalog/${tenantId}/items/${itemId}/photo`;
  },

  /** Upload or replace an item's photo. JPEG, PNG or WebP, up to 5 MB. */
  async uploadPhoto(itemId: string, file: File): Promise<void> {
    const body = new FormData();
    body.append("file", file);
    // No Content-Type header: the browser must set the multipart boundary, and
    // naming the type here overwrites it with one that has no boundary at all.
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items/${itemId}/photo`, {
      method: "POST",
      body,
    });
    await expectOk(res, "photo");
  },

  async catalog(): Promise<Catalog | null> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/mine`);
    if (res.status === 404) return null; // runs no store — an absence, not a failure
    await expectOk(res, "catalog");
    return res.json();
  },

  async createItem(input: ItemInput): Promise<{ id: string }> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    });
    await expectOk(res, "create");
    return res.json();
  },

  async updateItem(id: string, patch: Partial<ItemInput> & { is_listed?: boolean }): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items/${id}`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(patch),
    });
    await expectOk(res, "update");
  },

  async delistItem(id: string): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items/${id}`, {
      method: "DELETE",
    });
    await expectOk(res, "delist");
  },

  async setAvailability(id: string, state: Availability): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items/${id}/availability`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ state }),
    });
    await expectOk(res, "availability");
  },

  async declareAllergens(id: string, allergens: string[]): Promise<void> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/items/${id}/allergens`, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ allergens }),
    });
    await expectOk(res, "declaration");
  },

  /** One human act covering every listed, in-stock item. */
  async confirmAll(): Promise<number> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/confirm-all`, {
      method: "POST",
    });
    await expectOk(res, "confirm");
    const body = (await res.json()) as { confirmed: number };
    return body.confirmed;
  },

  /** The ingest port. Same endpoint every adapter will use. */
  async ingest(source: Exclude<CatalogSource, "manual">, items: unknown[]): Promise<IngestReport> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/ingest`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ source, items }),
    });
    await expectOk(res, "ingest");
    return res.json();
  },

  /**
   * Upload a spreadsheet.
   *
   * The adapter for vendors with no shop system at all — most of them. Posts
   * the raw file; parsing lives server-side so the rules sit next to the port
   * they feed rather than being reimplemented by every future client.
   *
   * Unreadable rows come back with line numbers, because a vendor holding a
   * 200-row sheet cannot act on a count.
   */
  async importCsv(file: File): Promise<CsvImportResult> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/catalog/ingest/csv`, {
      method: "POST",
      headers: { "Content-Type": "text/csv" },
      body: await file.text(),
    });
    await expectOk(res, "import");
    return res.json();
  },

  /**
   * Pull the merchant's shop products into this storefront.
   *
   * Lives on the connectors service, not omnideliv: the shop credentials are
   * there, and the sync is a service-to-service call the browser never makes.
   *
   * `platform` is optional — the server resolves it when exactly one shop is
   * linked to a store, and asks when more than one is. Guessing here would let
   * a WooCommerce menu quietly overwrite a Shopify one.
   *
   * No client-side check for whether a connector exists. The server
   * knows, and it answers with a sentence a merchant can act on ("connect your
   * shop first", "not linked to an OmniDeliv store") — which `expectOk`
   * surfaces verbatim. A button that hides itself teaches nobody why.
   */
  async syncCatalog(platform?: "shopify" | "woocommerce"): Promise<SyncResult> {
    const qs = platform ? `?platform=${platform}` : "";
    const res = await authFetch(`${API_BASE}/v1/connectors/catalog/sync${qs}`, { method: "POST" });
    await expectOk(res, "sync");
    return res.json();
  },
};

export interface CsvRowError {
  /** 1-based, header counted — matches the spreadsheet's own gutter. */
  line: number;
  reason: string;
}

export interface CsvImportResult {
  created: number;
  updated: number;
  rejected: number;
  row_errors: CsvRowError[];
  next_step: string;
}

export interface SyncResult {
  platform: string;
  fetched: number;
  created: number;
  updated: number;
  rejected: number;
  /** Variable products whose variations were not fetched (fan-out cap). */
  deferred: number;
  /** Rows with no usable price. Reported so a partial sync cannot look whole. */
  unpriced: number;
  next_step: string;
}

/**
 * Does this login run an OmniDeliv store?
 *
 * `null` while unknown. The nav renders nothing during that window rather than
 * showing the tab and retracting it — a control that appears and vanishes reads
 * as a glitch, and this resolves in one request.
 */
export function useHasStorefront(): boolean | null {
  const [has, setHas] = useState<boolean | null>(null);

  const check = useCallback(async () => {
    try {
      const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me`);
      setHas(res.ok);
    } catch {
      // A network failure is not evidence of absence, but the honest fallback
      // is still "don't show it" — a tab that 404s is worse than a missing one,
      // and the vendor's next page load re-checks.
      setHas(false);
    }
  }, []);

  useEffect(() => {
    void check();
  }, [check]);

  return has;
}
