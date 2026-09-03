"use client";
/**
 * Creating a venue, including the opening hours that decide whether any of its
 * printed codes will ever scan.
 *
 * The hours editor is the reason this is a component rather than three inputs.
 * A venue with no hours is CLOSED, not always-open — and because every scan
 * refusal is a deliberately indistinguishable 404, an operator who skips this
 * step gets a wall of stickers that silently do nothing. So the form defaults
 * to open-all-week rather than to empty, and says out loud what an empty
 * schedule would mean.
 */
import { useState } from "react";
import { Loader2, Plus, X } from "lucide-react";

import {
  DOW_LABELS,
  hhmmToMinute,
  minuteToHhmm,
  venuesApi,
  type OpeningWindow,
  type VenueKind,
  type VenueRow,
} from "@/lib/api/venues";

/** One editable row per day. `null` times mean the day is closed. */
interface DayRow {
  dow: number;
  open: boolean;
  from: string;
  to: string;
}

function defaultDays(): DayRow[] {
  // Open all week is the common case and the safe default: the alternative
  // default (empty) produces a venue whose codes never scan, with no error to
  // explain it.
  return Array.from({ length: 7 }, (_, i) => ({
    dow: i + 1,
    open: true,
    from: "09:00",
    to: "22:00",
  }));
}

/**
 * Common offsets, spelled out. Both live markets are DST-free, which is what
 * makes a fixed offset safe here — a venue in a DST country needs an IANA zone
 * on the column first.
 */
const OFFSETS = [
  { label: "UTC+08:00 — Philippines", minutes: 480 },
  { label: "UTC+04:00 — UAE", minutes: 240 },
  { label: "UTC+07:00 — Thailand, Vietnam", minutes: 420 },
  { label: "UTC+09:00 — Japan, Korea", minutes: 540 },
  { label: "UTC+00:00", minutes: 0 },
];

const FIELD =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/30 outline-none transition focus:border-cyan-neon/50 focus:bg-white/10";

export function VenueForm({
  onCreated,
  onCancel,
}: {
  onCreated: (v: VenueRow) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<VenueKind>("standalone");
  const [offset, setOffset] = useState(480);
  const [days, setDays] = useState<DayRow[]>(defaultDays);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const setDay = (dow: number, patch: Partial<DayRow>) =>
    setDays((ds) => ds.map((d) => (d.dow === dow ? { ...d, ...patch } : d)));

  /** Apply Monday's times to every open day — the usual case in one click. */
  const copyMonday = () => {
    const mon = days.find((d) => d.dow === 1);
    if (!mon) return;
    setDays((ds) => ds.map((d) => ({ ...d, from: mon.from, to: mon.to })));
  };

  const buildHours = (): OpeningWindow[] | string => {
    const out: OpeningWindow[] = [];
    for (const d of days) {
      if (!d.open) continue;
      const from = hhmmToMinute(d.from);
      const to = hhmmToMinute(d.to);
      if (from === null || to === null) {
        return `${DOW_LABELS[d.dow - 1]}: use 24-hour times like 09:00.`;
      }
      // A close at or before the open is how you say "past midnight" — 18:00 to
      // 01:00 becomes 1080..1500. Without this the server would reject it as a
      // window that can never match, which is true of 1080..60 but not of what
      // the operator meant.
      const close = to <= from ? to + 1440 : to;
      out.push({ dow: d.dow, open_minute: from, close_minute: close });
    }
    return out;
  };

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    const hours = buildHours();
    if (typeof hours === "string") {
      setError(hours);
      return;
    }

    setSaving(true);
    try {
      const venue = await venuesApi.create({
        name,
        kind,
        hours,
        utc_offset_minutes: offset,
      });
      onCreated(venue);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not create the venue");
    } finally {
      setSaving(false);
    }
  };

  const openCount = days.filter((d) => d.open).length;

  return (
    <form
      onSubmit={submit}
      className="space-y-6 rounded-xl border border-white/10 bg-white/[0.03] p-4 backdrop-blur-xl sm:p-6"
    >
      <div className="flex items-start justify-between gap-4">
        <div>
          <h2 className="font-heading text-lg font-semibold text-white">New venue</h2>
          <p className="mt-1 text-sm text-white/50">
            A place with tables. One restaurant, or one mall foodcourt with many stalls.
          </p>
        </div>
        <button
          type="button"
          onClick={onCancel}
          aria-label="Cancel"
          className="rounded-lg border border-white/10 bg-white/5 p-2 text-white/60 transition hover:bg-white/10 hover:text-white"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      <div className="grid gap-4 sm:grid-cols-2">
        <label className="space-y-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Name
          </span>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            maxLength={120}
            placeholder="Kanto Freestyle, SM Megamall"
            className={FIELD}
          />
        </label>

        <label className="space-y-1.5">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Type
          </span>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as VenueKind)}
            className={FIELD}
          >
            <option value="standalone">Standalone — one restaurant</option>
            <option value="foodcourt">Foodcourt — many stalls</option>
          </select>
        </label>

        <label className="space-y-1.5 sm:col-span-2">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Timezone
          </span>
          <select
            value={offset}
            onChange={(e) => setOffset(Number(e.target.value))}
            className={FIELD}
          >
            {OFFSETS.map((o) => (
              <option key={o.minutes} value={o.minutes}>
                {o.label}
              </option>
            ))}
          </select>
          <span className="block text-xs text-white/30">
            Opening hours below are in this venue&apos;s local time. Fixed offset — not
            suitable for a country that observes daylight saving.
          </span>
        </label>
      </div>

      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-xs font-medium uppercase tracking-wide text-white/50">
            Opening hours
          </span>
          <button
            type="button"
            onClick={copyMonday}
            className="rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-xs text-white/60 transition hover:bg-white/10 hover:text-white"
          >
            Copy Monday to all
          </button>
        </div>

        <div className="space-y-2">
          {days.map((d) => (
            <div
              key={d.dow}
              className="flex flex-wrap items-center gap-2 rounded-lg border border-white/5 bg-white/[0.02] px-3 py-2 sm:flex-nowrap"
            >
              <label className="flex w-24 shrink-0 items-center gap-2 text-sm text-white/70">
                <input
                  type="checkbox"
                  checked={d.open}
                  onChange={(e) => setDay(d.dow, { open: e.target.checked })}
                  className="h-4 w-4 rounded border-white/20 bg-white/10 accent-cyan-neon"
                />
                {DOW_LABELS[d.dow - 1]}
              </label>

              {d.open ? (
                <div className="flex flex-1 flex-wrap items-center gap-2">
                  <input
                    type="time"
                    value={d.from}
                    onChange={(e) => setDay(d.dow, { from: e.target.value })}
                    className={`${FIELD} max-w-[8rem]`}
                  />
                  <span className="text-white/30">to</span>
                  <input
                    type="time"
                    value={d.to}
                    onChange={(e) => setDay(d.dow, { to: e.target.value })}
                    className={`${FIELD} max-w-[8rem]`}
                  />
                  {hhmmToMinute(d.to) !== null &&
                    hhmmToMinute(d.from) !== null &&
                    hhmmToMinute(d.to)! <= hhmmToMinute(d.from)! && (
                      <span className="text-xs text-white/40">next day</span>
                    )}
                </div>
              ) : (
                <span className="flex-1 text-sm text-white/30">Closed</span>
              )}
            </div>
          ))}
        </div>

        {openCount === 0 && (
          // The trap this whole component exists to prevent, stated before it
          // can happen rather than discovered from stickers that do nothing.
          <p className="rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-3 py-2 text-sm text-amber-signal">
            No days are open, so every code printed for this venue will refuse every
            scan. You can save it and set hours later.
          </p>
        )}
      </div>

      {error && (
        <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-sm text-red-signal">
          {error}
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        <button
          type="submit"
          disabled={saving || name.trim() === ""}
          className="inline-flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-neon/10 px-4 py-2 text-sm font-medium text-cyan-neon transition hover:bg-cyan-neon/20 disabled:opacity-40"
        >
          {saving ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <Plus className="h-4 w-4" />
          )}
          Create venue
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-lg border border-white/10 bg-white/5 px-4 py-2 text-sm text-white/70 transition hover:bg-white/10"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
