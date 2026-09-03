/**
 * OmniDeliv venues and tables — merchant-portal client.
 *
 * The setup half of QR table ordering. Everything downstream of "a venue and a
 * table exist" shipped and deployed working; nothing could create either, so
 * the feature was unreachable without hand-written SQL. These are the endpoints
 * that close that.
 *
 * `scan_url` always comes from the server, never assembled here. The portal
 * does not know the public scan origin — that is `TABLE_SCAN_BASE_URL` on the
 * service — and a second copy of it in a `NEXT_PUBLIC_*` would be compiled into
 * the bundle at build time and wrong for every tenant it was not built for.
 */
import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

/** 1 = Monday .. 7 = Sunday, matching ISO-8601 and the server. */
export interface OpeningWindow {
  dow: number;
  /** Minutes past local midnight. 540 is 09:00. */
  open_minute: number;
  /** May exceed 1440: a kitchen open until 01:00 is 1500, not a second window. */
  close_minute: number;
}

export type VenueKind = "standalone" | "foodcourt";

export interface VenueRow {
  venue_id: string;
  name: string;
  kind: VenueKind;
  status: "active" | "paused" | "closed";
  utc_offset_minutes: number;
  hours: OpeningWindow[];
  /**
   * `null` when a printed code would scan right now, otherwise why it would
   * not. Operator-only — a diner gets one indistinguishable 404 for all of
   * these, on purpose.
   */
  not_orderable: "venue_not_active" | "table_closed" | "outside_opening_hours" | null;
}

export interface TableRow {
  table_id: string;
  label: string;
  status: "open" | "closed";
  scan_url: string;
  printed_at: string | null;
}

export interface NewTableRow {
  table_id: string;
  label: string;
  scan_url: string;
}

export interface VendorRow {
  vendor_id: string;
  name: string;
}

/**
 * A vendor of this tenant, for the "which stalls sell here" picker.
 *
 * Borrowed from the existing admin vendor list rather than adding a second
 * listing endpoint — it is already gated on the same `vendors:manage`
 * permission every route here uses.
 */
export interface TenantVendorRow {
  id: string;
  name: string;
  status: string;
  vertical: string;
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await authFetch(`${API_BASE}${path}`, init);
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(detail || "That did not go through. Try again.");
  }
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}

function json(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  };
}

export const venuesApi = {
  list: () => req<VenueRow[]>("/v1/omnideliv/venues"),

  get: (venueId: string) => req<VenueRow>(`/v1/omnideliv/venues/${venueId}`),

  create: (body: {
    name: string;
    kind: VenueKind;
    hours: OpeningWindow[];
    /** Always sent explicitly — the server has no default, deliberately. */
    utc_offset_minutes: number;
  }) => req<VenueRow>("/v1/omnideliv/venues", json("POST", body)),

  /**
   * Edit a venue, or stop it trading.
   *
   * `status: "paused"` is the kill switch: the server refuses every scan at
   * this venue while it is not active, so this is how table ordering stops
   * across the whole building at once. Before it existed the only recourse was
   * rotating every table's code one at a time, which permanently kills every
   * printed sticker.
   */
  update: (
    venueId: string,
    patch: {
      name?: string;
      hours?: OpeningWindow[];
      utc_offset_minutes?: number;
      status?: VenueRow["status"];
    },
  ) => req<VenueRow>(`/v1/omnideliv/venues/${venueId}`, json("PATCH", patch)),

  /** Refused by the server while the venue still has tables. */
  remove: (venueId: string) =>
    req<void>(`/v1/omnideliv/venues/${venueId}`, { method: "DELETE" }),

  /**
   * Open or close one table. The printed code stays valid either way, so
   * reopening is a click rather than a reprint.
   */
  setTableStatus: (tableId: string, status: TableRow["status"]) =>
    req<void>(
      `/v1/omnideliv/venues/tables/${tableId}`,
      json("PATCH", { status }),
    ),

  /** Refused by the server while a diner session is open at that table. */
  removeTable: (tableId: string) =>
    req<void>(`/v1/omnideliv/venues/tables/${tableId}`, { method: "DELETE" }),

  tables: (venueId: string) =>
    req<TableRow[]>(`/v1/omnideliv/venues/${venueId}/tables`),

  /** Batch: a restaurant sets up twenty tables at once. */
  addTables: (venueId: string, labels: string[]) =>
    req<NewTableRow[]>(
      `/v1/omnideliv/venues/${venueId}/tables`,
      json("POST", { labels }),
    ),

  /** Replaces the code on the wall. The old one stops working immediately. */
  rotate: (tableId: string) =>
    req<{ table_id: string; scan_url: string }>(
      `/v1/omnideliv/venues/tables/${tableId}/rotate`,
      { method: "POST" },
    ),

  markPrinted: (tableId: string) =>
    req<void>(`/v1/omnideliv/venues/tables/${tableId}/printed`, {
      method: "POST",
    }),

  vendors: (venueId: string) =>
    req<VendorRow[]>(`/v1/omnideliv/venues/${venueId}/vendors`),

  linkVendor: (venueId: string, vendorId: string) =>
    req<void>(
      `/v1/omnideliv/venues/${venueId}/vendors`,
      json("POST", { vendor_id: vendorId }),
    ),

  /** Every vendor this tenant has, to pick from when linking. */
  allVendors: () => req<TenantVendorRow[]>("/v1/omnideliv/admin/vendors"),

  unlinkVendor: (venueId: string, vendorId: string) =>
    req<void>(`/v1/omnideliv/venues/${venueId}/vendors/${vendorId}`, {
      method: "DELETE",
    }),
};

/** `540` -> `"09:00"`, and `1500` -> `"01:00"` on the following day. */
export function minuteToHhmm(m: number): string {
  const wrapped = m % 1440;
  const h = Math.floor(wrapped / 60);
  const min = wrapped % 60;
  return `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
}

/** `"09:00"` -> `540`. Returns null for anything that is not a time. */
export function hhmmToMinute(s: string): number | null {
  const m = s.trim().match(/^(\d{1,2}):(\d{2})$/);
  if (!m) return null;
  const h = Number(m[1]);
  const min = Number(m[2]);
  if (h > 23 || min > 59) return null;
  return h * 60 + min;
}

export const DOW_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** Why nothing will scan, in words an operator can act on. */
export function notOrderableReason(v: VenueRow): string | null {
  switch (v.not_orderable) {
    case "outside_opening_hours":
      return v.hours.length === 0
        ? "No opening hours set — every code at this venue will refuse every scan."
        : "Outside opening hours right now — codes will not scan until it opens.";
    case "venue_not_active":
      return `Venue is ${v.status} — codes will not scan.`;
    case "table_closed":
      return "This table is closed.";
    default:
      return null;
  }
}
