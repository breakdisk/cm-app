"use client";
/**
 * Venues — the places with tables that QR ordering happens in.
 *
 * This screen is what made the feature reachable. The scan endpoint, the diner
 * principal, the venue-scoped basket and the dine-in order all shipped and
 * deployed working, on top of a schema that nothing could put a row in.
 */
import { useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { AlertTriangle, ChevronRight, Loader2, Plus, Store, Utensils } from "lucide-react";

import { VenueForm } from "@/components/storefront/venue-form";
import {
  DOW_LABELS,
  minuteToHhmm,
  notOrderableReason,
  venuesApi,
  type VenueRow,
} from "@/lib/api/venues";

/** "Mon–Sun 09:00–22:00" where it collapses, otherwise a per-day list. */
function hoursSummary(v: VenueRow): string {
  if (v.hours.length === 0) return "No opening hours";
  const sorted = [...v.hours].sort((a, b) => a.dow - b.dow);
  const first = sorted[0];
  const uniform =
    sorted.length === 7 &&
    sorted.every(
      (w) => w.open_minute === first.open_minute && w.close_minute === first.close_minute,
    );
  if (uniform) {
    return `Every day ${minuteToHhmm(first.open_minute)}–${minuteToHhmm(first.close_minute)}`;
  }
  return sorted
    .map(
      (w) =>
        `${DOW_LABELS[w.dow - 1]} ${minuteToHhmm(w.open_minute)}–${minuteToHhmm(w.close_minute)}`,
    )
    .join(" · ");
}

export default function VenuesPage() {
  const [venues, setVenues] = useState<VenueRow[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setVenues(await venuesApi.list());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load venues");
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="font-heading text-2xl font-semibold text-white">Venues</h1>
          <p className="text-sm text-white/50">
            Places with tables. Print a code per table and diners order from their seat.
          </p>
        </div>
        {!creating && (
          <button
            onClick={() => setCreating(true)}
            className="inline-flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-neon/10 px-4 py-2 text-sm font-medium text-cyan-neon transition hover:bg-cyan-neon/20"
          >
            <Plus className="h-4 w-4" />
            New venue
          </button>
        )}
      </div>

      {creating && (
        <VenueForm
          onCancel={() => setCreating(false)}
          onCreated={(v) => {
            setCreating(false);
            setVenues((vs) => [v, ...vs]);
          }}
        />
      )}

      {error && (
        <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-3 text-sm text-red-signal">
          {error}
        </p>
      )}

      {!loaded ? (
        <div className="flex items-center justify-center py-16">
          <Loader2 className="h-6 w-6 animate-spin text-white/30" />
        </div>
      ) : venues.length === 0 && !creating ? (
        <div className="flex flex-col items-center gap-3 rounded-xl border border-white/10 bg-white/[0.02] py-16 text-center">
          <Store className="h-8 w-8 text-white/25" />
          <p className="text-sm text-white/50">No venues yet.</p>
          <p className="max-w-sm text-xs text-white/30">
            Create one to start printing table codes. A standalone restaurant is one
            venue; a mall foodcourt is one venue with a stall per vendor.
          </p>
        </div>
      ) : (
        <ul className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {venues.map((v) => {
            const warning = notOrderableReason(v);
            return (
              <li key={v.venue_id}>
                <Link
                  href={`/storefront/venues/${v.venue_id}`}
                  className="group flex h-full flex-col gap-3 rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl transition hover:border-cyan-neon/30 hover:bg-white/[0.06]"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex items-start gap-3">
                      <span className="rounded-lg border border-white/10 bg-white/5 p-2 text-cyan-neon">
                        {v.kind === "foodcourt" ? (
                          <Utensils className="h-4 w-4" />
                        ) : (
                          <Store className="h-4 w-4" />
                        )}
                      </span>
                      <div className="min-w-0">
                        <p className="truncate font-heading font-semibold text-white">
                          {v.name}
                        </p>
                        <p className="text-xs capitalize text-white/40">
                          {v.kind} · {v.status}
                        </p>
                      </div>
                    </div>
                    <ChevronRight className="h-4 w-4 shrink-0 text-white/20 transition group-hover:text-cyan-neon" />
                  </div>

                  <p className="text-xs text-white/50">{hoursSummary(v)}</p>

                  {warning && (
                    // Operator-only. A diner gets one indistinguishable 404 for
                    // every one of these — this is the other half of that
                    // decision, so the person who can fix it is told.
                    <p className="mt-auto flex items-start gap-1.5 rounded-lg border border-amber-signal/25 bg-amber-signal/10 px-2.5 py-1.5 text-[11px] leading-snug text-amber-signal">
                      <AlertTriangle className="mt-px h-3.5 w-3.5 shrink-0" />
                      {warning}
                    </p>
                  )}
                </Link>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
