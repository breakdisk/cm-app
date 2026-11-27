"use client";
/**
 * Opening hours, for both creating a venue and editing one afterwards.
 *
 * Extracted from `VenueForm` because hours were previously set once at creation
 * and then frozen — the create form had the only editor on the platform, so a
 * venue whose hours changed had no way to say so. Sharing one component means
 * the two paths cannot drift into disagreeing about what a window means.
 *
 * ## Why the presets matter
 *
 * A venue is CLOSED when it has no matching window, and every scan refusal is a
 * deliberately indistinguishable 404 — so hours that do not say what the
 * operator meant produce a wall of codes that silently do nothing. "Open 24
 * hours" is the case that is easiest to get wrong by hand: it is `00:00` to
 * `00:00`, which reads like a mistake and is one keystroke from a window that
 * can never match. So it is a button.
 */
import { Clock } from "lucide-react";

import { DOW_LABELS, hhmmToMinute, type OpeningWindow } from "@/lib/api/venues";

/** One editable row per day. */
export interface DayRow {
  dow: number;
  open: boolean;
  from: string;
  to: string;
}

const FIELD =
  "w-full rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white placeholder:text-white/30 outline-none transition focus:border-cyan-neon/50 focus:bg-white/10";

export function defaultDays(): DayRow[] {
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

/** Server windows -> editable rows. A day with no window is closed. */
export function daysFromHours(hours: OpeningWindow[]): DayRow[] {
  return Array.from({ length: 7 }, (_, i) => {
    const dow = i + 1;
    const w = hours.find((h) => h.dow === dow);
    if (!w) return { dow, open: false, from: "09:00", to: "22:00" };
    const hhmm = (m: number) => {
      const wrapped = m % 1440;
      return `${String(Math.floor(wrapped / 60)).padStart(2, "0")}:${String(
        wrapped % 60,
      ).padStart(2, "0")}`;
    };
    return { dow, open: true, from: hhmm(w.open_minute), to: hhmm(w.close_minute) };
  });
}

/**
 * Editable rows -> server windows, or a message naming the day at fault.
 *
 * A close at or before the open means "past midnight" — 18:00 to 01:00 becomes
 * 1080..1500 — which is also how 00:00 to 00:00 becomes a full 24 hours.
 * Without that, the server would reject it as a window that can never match,
 * which is true of the numbers but not of what the operator meant.
 */
export function hoursFromDays(days: DayRow[]): OpeningWindow[] | string {
  const out: OpeningWindow[] = [];
  for (const d of days) {
    if (!d.open) continue;
    const from = hhmmToMinute(d.from);
    const to = hhmmToMinute(d.to);
    if (from === null || to === null) {
      return `${DOW_LABELS[d.dow - 1]}: use 24-hour times like 09:00.`;
    }
    out.push({ dow: d.dow, open_minute: from, close_minute: to <= from ? to + 1440 : to });
  }
  return out;
}

/** True when every day is open midnight-to-midnight. */
export function isAlwaysOpen(days: DayRow[]): boolean {
  return days.every((d) => d.open && d.from === "00:00" && d.to === "00:00");
}

export function HoursEditor({
  days,
  onChange,
}: {
  days: DayRow[];
  onChange: (next: DayRow[]) => void;
}) {
  const setDay = (dow: number, patch: Partial<DayRow>) =>
    onChange(days.map((d) => (d.dow === dow ? { ...d, ...patch } : d)));

  /** The 24-hour sale, in one click. */
  const allDay = () =>
    onChange(days.map((d) => ({ ...d, open: true, from: "00:00", to: "00:00" })));

  const copyMonday = () => {
    const mon = days.find((d) => d.dow === 1);
    if (!mon) return;
    onChange(days.map((d) => ({ ...d, from: mon.from, to: mon.to })));
  };

  const openCount = days.filter((d) => d.open).length;

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="text-xs font-medium uppercase tracking-wide text-white/50">
          Opening hours
        </span>
        <div className="flex flex-wrap gap-2">
          <button
            type="button"
            onClick={allDay}
            className="inline-flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/10 px-2.5 py-1 text-xs text-cyan-neon transition hover:bg-cyan-neon/20"
          >
            <Clock className="h-3 w-3" />
            Open 24 hours
          </button>
          <button
            type="button"
            onClick={copyMonday}
            className="rounded-lg border border-white/10 bg-white/5 px-2.5 py-1 text-xs text-white/60 transition hover:bg-white/10 hover:text-white"
          >
            Copy Monday to all
          </button>
        </div>
      </div>

      {isAlwaysOpen(days) && (
        <p className="rounded-lg border border-cyan-neon/25 bg-cyan-neon/[0.06] px-3 py-2 text-xs text-cyan-neon">
          Open 24 hours, every day. Codes will scan at any time.
        </p>
      )}

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
                {d.from === "00:00" && d.to === "00:00" ? (
                  <span className="text-xs text-cyan-neon">all day</span>
                ) : (
                  hhmmToMinute(d.to) !== null &&
                  hhmmToMinute(d.from) !== null &&
                  hhmmToMinute(d.to)! <= hhmmToMinute(d.from)! && (
                    <span className="text-xs text-white/40">next day</span>
                  )
                )}
              </div>
            ) : (
              <span className="flex-1 text-sm text-white/30">Closed</span>
            )}
          </div>
        ))}
      </div>

      {openCount === 0 && (
        // The trap this component exists to prevent, stated before it can
        // happen rather than discovered from stickers that do nothing.
        <p className="rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-3 py-2 text-sm text-amber-signal">
          No days are open, so every code at this venue will refuse every scan.
        </p>
      )}
    </div>
  );
}
