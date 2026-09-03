"use client";
/**
 * One venue: its tables, the codes on them, and which vendors sell there.
 *
 * The vendor list is not decoration. `vendor_is_at_venue` is the guard that
 * makes a table order a VENUE order — a diner's basket takes `vendor_id` from
 * the client, and only vendors linked here may be added to it. A venue with no
 * vendors linked has nothing orderable at any of its tables, which is exactly
 * the state the whole platform was in before this screen existed.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import {
  AlertTriangle,
  ArrowLeft,
  Loader2,
  Pause,
  Play,
  Plus,
  Store,
  Trash2,
  Utensils,
} from "lucide-react";

import { TablePrintSheet } from "@/components/storefront/table-print-sheet";
import {
  DOW_LABELS,
  minuteToHhmm,
  notOrderableReason,
  venuesApi,
  type TableRow,
  type TenantVendorRow,
  type VendorRow,
  type VenueRow,
} from "@/lib/api/venues";

const FIELD =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/30 outline-none transition focus:border-cyan-neon/50 focus:bg-white/10";

/**
 * `"1-12, patio, 14"` -> twelve numbered labels, `patio`, and `14`.
 *
 * Typing twenty labels by hand is the kind of chore that ends in a venue with
 * six tables set up and the rest done "later".
 */
export function parseLabels(input: string): string[] {
  const out: string[] = [];
  for (const rawPart of input.split(",")) {
    const part = rawPart.trim();
    if (part === "") continue;
    const range = part.match(/^(\d+)\s*-\s*(\d+)$/);
    if (range) {
      const from = Number(range[1]);
      const to = Number(range[2]);
      // Bounded: a typo like 1-100000 must not try to create a hundred thousand
      // tables. The server caps at 200 too; this is the friendlier half.
      if (to >= from && to - from < 200) {
        for (let i = from; i <= to; i++) out.push(String(i));
        continue;
      }
    }
    out.push(part);
  }
  // Dedupe, preserving order — two tables called "12" is a support ticket.
  return Array.from(new Set(out));
}

export default function VenueDetailPage() {
  const params = useParams<{ venueId: string }>();
  const venueId = params.venueId;

  const [venue, setVenue] = useState<VenueRow | null>(null);
  const [tables, setTables] = useState<TableRow[]>([]);
  const [linked, setLinked] = useState<VendorRow[]>([]);
  const [allVendors, setAllVendors] = useState<TenantVendorRow[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [labelInput, setLabelInput] = useState("");
  const [adding, setAdding] = useState(false);
  const [vendorToLink, setVendorToLink] = useState("");
  const [linking, setLinking] = useState(false);
  const [trading, setTrading_] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [v, t, l] = await Promise.all([
        venuesApi.get(venueId),
        venuesApi.tables(venueId),
        venuesApi.vendors(venueId),
      ]);
      setVenue(v);
      setTables(t);
      setLinked(l);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load this venue");
    } finally {
      setLoaded(true);
    }
  }, [venueId]);

  useEffect(() => {
    void refresh();
    // The picker list is independent of the venue and only needed once.
    venuesApi
      .allVendors()
      .then(setAllVendors)
      .catch(() => setAllVendors([]));
  }, [refresh]);

  const pending = useMemo(() => parseLabels(labelInput), [labelInput]);
  const existing = useMemo(() => new Set(tables.map((t) => t.label)), [tables]);
  const clashes = pending.filter((l) => existing.has(l));

  const addTables = async () => {
    if (pending.length === 0) return;
    setAdding(true);
    setError(null);
    try {
      await venuesApi.addTables(venueId, pending);
      setLabelInput("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not add those tables");
    } finally {
      setAdding(false);
    }
  };

  const link = async () => {
    if (!vendorToLink) return;
    setLinking(true);
    setError(null);
    try {
      await venuesApi.linkVendor(venueId, vendorToLink);
      setVendorToLink("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not link that vendor");
    } finally {
      setLinking(false);
    }
  };

  /**
   * The stop button for this venue's entire QR surface.
   *
   * Pausing makes the server refuse every scan here. Sessions already open are
   * deliberately left alone, so people mid-meal finish and simply cannot add
   * anything new -- the alternative strands a half-eaten paid-for order.
   */
  const setTrading = async (status: VenueRow["status"]) => {
    if (
      status !== "active" &&
      !window.confirm(
        `Stop taking orders at ${venue?.name}?\n\nEvery code in the building stops working immediately. Diners already ordering can finish. You can resume at any time and the printed codes stay valid.`,
      )
    ) {
      return;
    }
    setTrading_(true);
    setError(null);
    try {
      setVenue(await venuesApi.update(venueId, { status }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not change trading status");
    } finally {
      setTrading_(false);
    }
  };

  const removeVenue = async () => {
    if (
      !window.confirm(
        `Delete ${venue?.name}?\n\nThis cannot be undone.`,
      )
    ) {
      return;
    }
    setError(null);
    try {
      await venuesApi.remove(venueId);
      window.location.href = "/storefront/venues";
    } catch (e) {
      // The server refuses while tables remain, and says how many. That message
      // is more useful than anything this screen could invent.
      setError(e instanceof Error ? e.message : "Could not delete this venue");
    }
  };

  const unlink = async (v: VendorRow) => {
    if (
      !window.confirm(
        `Stop ${v.name} selling at this venue?\n\nDiners at these tables will no longer be able to order from them.`,
      )
    ) {
      return;
    }
    setError(null);
    try {
      await venuesApi.unlinkVendor(venueId, v.vendor_id);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not unlink that vendor");
    }
  };

  if (!loaded) {
    return (
      <div className="flex items-center justify-center py-24">
        <Loader2 className="h-6 w-6 animate-spin text-white/30" />
      </div>
    );
  }

  if (!venue) {
    return (
      <div className="space-y-4">
        <BackLink />
        <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-3 text-sm text-red-signal">
          {error ?? "Venue not found."}
        </p>
      </div>
    );
  }

  const warning = notOrderableReason(venue);
  const linkable = allVendors.filter(
    (v) => !linked.some((l) => l.vendor_id === v.id),
  );

  return (
    <div className="space-y-6">
      <div className="no-print space-y-4">
        <BackLink />

        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex items-start gap-3">
            <span className="rounded-lg border border-white/10 bg-white/5 p-2.5 text-cyan-neon">
              {venue.kind === "foodcourt" ? (
                <Utensils className="h-5 w-5" />
              ) : (
                <Store className="h-5 w-5" />
              )}
            </span>
            <div>
              <h1 className="font-heading text-2xl font-semibold text-white">
                {venue.name}
              </h1>
              <p className="text-sm capitalize text-white/50">
                {venue.kind} · {venue.status} · {tables.length}{" "}
                {tables.length === 1 ? "table" : "tables"}
              </p>
            </div>
          </div>
        </div>

        <section className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl">
          <div className="min-w-0">
            <h2 className="font-heading text-base font-semibold text-white">
              {venue.status === "active" ? "Taking orders" : "Not taking orders"}
            </h2>
            <p className="mt-0.5 text-xs text-white/40">
              {venue.status === "active"
                ? "Pausing stops every code in the building at once. Diners already ordering can finish."
                : "Every code here is refusing scans. Printed codes stay valid — resuming needs no reprint."}
            </p>
          </div>
          <button
            onClick={() => setTrading(venue.status === "active" ? "paused" : "active")}
            disabled={trading}
            className={
              venue.status === "active"
                ? "inline-flex shrink-0 items-center gap-2 rounded-lg border border-amber-signal/40 bg-amber-signal/10 px-4 py-2 text-sm font-medium text-amber-signal transition hover:bg-amber-signal/20 disabled:opacity-40"
                : "inline-flex shrink-0 items-center gap-2 rounded-lg border border-green-signal/40 bg-green-signal/10 px-4 py-2 text-sm font-medium text-green-signal transition hover:bg-green-signal/20 disabled:opacity-40"
            }
          >
            {trading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : venue.status === "active" ? (
              <Pause className="h-4 w-4" />
            ) : (
              <Play className="h-4 w-4" />
            )}
            {venue.status === "active" ? "Pause ordering" : "Resume ordering"}
          </button>
        </section>

        {warning && (
          <p className="flex items-start gap-2 rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-4 py-3 text-sm text-amber-signal">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            {warning}
          </p>
        )}

        {linked.length === 0 && (
          // Without a linked vendor the venue guard refuses every basket add,
          // so the codes scan and then nothing can be ordered. Worth its own
          // warning because it looks like a working venue until a diner tries.
          <p className="flex items-start gap-2 rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-4 py-3 text-sm text-amber-signal">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
            No vendors sell here yet. Codes will scan, but diners will have nothing to
            order.
          </p>
        )}

        {error && (
          <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-3 text-sm text-red-signal">
            {error}
          </p>
        )}

        <section className="rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl">
          <h2 className="font-heading text-base font-semibold text-white">Opening hours</h2>
          <p className="mt-1 text-xs text-white/40">
            Local time at the venue (UTC
            {venue.utc_offset_minutes >= 0 ? "+" : "−"}
            {minuteToHhmm(Math.abs(venue.utc_offset_minutes))}).
          </p>
          {venue.hours.length === 0 ? (
            <p className="mt-3 text-sm text-white/40">None set — nothing will scan.</p>
          ) : (
            <ul className="mt-3 flex flex-wrap gap-2">
              {[...venue.hours]
                .sort((a, b) => a.dow - b.dow)
                .map((w, i) => (
                  <li
                    key={`${w.dow}-${i}`}
                    className="rounded-lg border border-white/5 bg-white/[0.02] px-2.5 py-1 text-xs text-white/60"
                  >
                    <span className="text-white/80">{DOW_LABELS[w.dow - 1]}</span>{" "}
                    {minuteToHhmm(w.open_minute)}–{minuteToHhmm(w.close_minute)}
                    {w.close_minute > 1440 && (
                      <span className="text-white/30"> (+1d)</span>
                    )}
                  </li>
                ))}
            </ul>
          )}
        </section>

        <section className="rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl">
          <h2 className="font-heading text-base font-semibold text-white">
            Vendors selling here
          </h2>
          <p className="mt-1 text-xs text-white/40">
            Only these can be ordered from at this venue&apos;s tables.
          </p>

          {linked.length > 0 && (
            <ul className="mt-3 flex flex-wrap gap-2">
              {linked.map((v) => (
                <li
                  key={v.vendor_id}
                  className="flex max-w-full items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-1.5 text-sm text-white/80"
                >
                  <span className="min-w-0 truncate">{v.name}</span>
                  <button
                    onClick={() => unlink(v)}
                    aria-label={`Remove ${v.name} from this venue`}
                    className="text-white/30 transition hover:text-red-signal"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}

          <div className="mt-3 flex flex-wrap items-center gap-2">
            <select
              value={vendorToLink}
              onChange={(e) => setVendorToLink(e.target.value)}
              className={`${FIELD} sm:max-w-xs`}
              aria-label="Vendor to add"
            >
              <option value="">
                {linkable.length === 0
                  ? "No other vendors available"
                  : "Add a vendor…"}
              </option>
              {linkable.map((v) => (
                <option key={v.id} value={v.id}>
                  {v.name} ({v.status})
                </option>
              ))}
            </select>
            <button
              onClick={link}
              disabled={!vendorToLink || linking}
              className="inline-flex items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white/70 transition hover:bg-white/10 disabled:opacity-40"
            >
              {linking ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              Add
            </button>
          </div>
        </section>

        <section className="rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl">
          <h2 className="font-heading text-base font-semibold text-white">Add tables</h2>
          <p className="mt-1 text-xs text-white/40">
            Ranges and names both work — <code className="text-white/60">1-12, patio, bar</code>.
          </p>
          <div className="mt-3 flex flex-wrap items-center gap-2">
            <input
              value={labelInput}
              onChange={(e) => setLabelInput(e.target.value)}
              placeholder="1-12, patio, bar"
              className={`${FIELD} sm:max-w-md`}
              aria-label="Table labels"
            />
            <button
              onClick={addTables}
              disabled={pending.length === 0 || clashes.length > 0 || adding}
              className="inline-flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-neon/10 px-4 py-2 text-sm font-medium text-cyan-neon transition hover:bg-cyan-neon/20 disabled:opacity-40"
            >
              {adding ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Plus className="h-4 w-4" />
              )}
              Add {pending.length > 0 && `${pending.length}`}
            </button>
          </div>

          {clashes.length > 0 ? (
            <p className="mt-2 text-xs text-amber-signal">
              Already exists: {clashes.join(", ")}
            </p>
          ) : (
            pending.length > 0 && (
              <p className="mt-2 text-xs text-white/40">
                Will create: {pending.slice(0, 12).join(", ")}
                {pending.length > 12 && ` … and ${pending.length - 12} more`}
              </p>
            )
          )}
        </section>

        <section className="rounded-xl border border-red-signal/20 bg-red-signal/[0.03] p-4">
          <h2 className="font-heading text-base font-semibold text-white">Delete venue</h2>
          <p className="mt-1 text-xs text-white/40">
            Only possible once every table is removed — deleting a venue with tables
            would invalidate all of its printed codes at once.
          </p>
          <button
            onClick={removeVenue}
            className="mt-3 inline-flex items-center gap-2 rounded-lg border border-red-signal/40 bg-red-signal/10 px-3 py-2 text-sm text-red-signal transition hover:bg-red-signal/20"
          >
            <Trash2 className="h-4 w-4" />
            Delete this venue
          </button>
        </section>
      </div>

      <TablePrintSheet
        venueName={venue.name}
        tables={tables}
        onChanged={() => void refresh()}
      />
    </div>
  );
}

function BackLink() {
  return (
    <Link
      href="/storefront/venues"
      className="inline-flex items-center gap-1.5 text-sm text-white/50 transition hover:text-white"
    >
      <ArrowLeft className="h-4 w-4" />
      All venues
    </Link>
  );
}
