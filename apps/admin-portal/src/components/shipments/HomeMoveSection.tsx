"use client";
/**
 * The whole-home detail on a home-move shipment: the property, the survey and
 * move times, and the inventory as it was priced — what ops need to assign a
 * crew. Read from `GET /v1/shipments/:id/home`, which admits tenant-wide staff.
 */
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { authFetch } from "@/lib/auth/auth-fetch";
import { GlassCard } from "@/components/ui/glass-card";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";
const FLOORS = ["Ground", "1st", "2nd", "3rd", "4th", "5th +"];

interface HomeItem {
  room: string;
  name: string;
  qty: number;
  volume_l: number;
  weight_kg: number;
  dismantle: boolean;
  packing: boolean;
}

interface HomeMove {
  property: {
    type: string;
    size: string;
    pickup_floor: number;
    pickup_has_lift: boolean;
    dropoff_floor: number;
    dropoff_has_lift: boolean;
    long_carry: boolean;
  };
  items: HomeItem[];
  plan: string;
  distance_centikm: number;
  survey_required: boolean;
  survey_at: string | null;
  move_at: string;
  total_cents?: number;
  currency?: string;
  trucks?: number;
  crew_total?: number;
  survey_cents?: number;
  /** 10000 is none. */
  surge_bps?: number;
  surge_cents?: number;
}

/** The lead's pay — sent to staff and the lead, never the customer. */
export interface HomePay {
  lead_gross_cents: number;
  commission_cents: number;
  lead_payout_cents: number;
  survey_payout_cents: number;
  survey_commission_cents: number;
}

function money(cents: number, currency = "PHP"): string {
  return `${currency} ${(cents / 100).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

/** What ops read off the move's money: the fare, any surge, and what the lead and platform take. */
export function payLines(home: Pick<HomeMove, "total_cents" | "currency" | "surge_bps" | "surge_cents">, pay: HomePay | null): [string, string][] {
  const c = home.currency ?? "PHP";
  const out: [string, string][] = [];
  if (home.total_cents != null) out.push(["Customer paid", money(home.total_cents, c)]);
  if ((home.surge_bps ?? 10000) > 10000) out.push(["High-demand surge", `×${((home.surge_bps ?? 10000) / 10000).toFixed(2)} · ${money(home.surge_cents ?? 0, c)} (to the lead)`]);
  if (pay) {
    out.push(["Lead's pay", money(pay.lead_payout_cents, c)]);
    out.push(["Platform commission", money(pay.commission_cents, c)]);
    if (pay.survey_payout_cents > 0) out.push(["Survey fee to lead", `${money(pay.survey_payout_cents, c)} (commission ${money(pay.survey_commission_cents, c)})`]);
  }
  return out;
}

function when(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

function floorLine(floor: number, lift: boolean): string {
  return `${FLOORS[floor] ?? floor} floor${lift ? ", lift" : floor > 0 ? ", no lift" : ""}`;
}

export function HomeMoveSection({ shipmentId }: { shipmentId: string }) {
  const [home, setHome] = useState<HomeMove | null>(null);
  const [lead, setLead] = useState<string | null>(null);
  const [pay, setPay] = useState<HomePay | null>(null);
  const [slot, setSlot] = useState<"support" | "emergency">("emergency");
  const [slotPay, setSlotPay] = useState("");
  const [offering, setOffering] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHome(null);
    setError(null);
    authFetch(`${API_BASE}/v1/shipments/${shipmentId}/home`)
      .then(async (r) => {
        if (!r.ok) throw new Error(r.status === 404 ? "No home-move detail recorded" : `${r.status} ${r.statusText}`);
        const j = await r.json();
        if (!cancelled) {
          setHome(j.data);
          setLead(j.lead_name ?? null);
          setPay(j.pay ?? null);
        }
      })
      .catch((e) => { if (!cancelled) setError(e instanceof Error ? e.message : "Could not load the home move"); });
    return () => { cancelled = true; };
  }, [shipmentId]);

  /**
   * Offer the move another truck. A captain's claim and a paid addendum do
   * this by themselves; this is for the slot nobody took.
   */
  async function offerTruck() {
    setOffering(true);
    try {
      const cents = slotPay.trim() ? Math.round(Number(slotPay) * 100) : null;
      if (cents != null && !Number.isFinite(cents)) throw new Error("That pay is not a number");
      const r = await authFetch(`${API_BASE}/v1/home-moves/${shipmentId}/slots`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ slot, count: 1, payout_cents: cents }),
      });
      const j = await r.json().catch(() => ({}));
      if (!r.ok) throw new Error(j?.error?.message ?? `${r.status} ${r.statusText}`);
      toast.success(slot === "support" ? "Support truck offered" : "Extra truck offered");
      setSlotPay("");
    } catch (e) {
      toast.error(e instanceof Error ? e.message : "Could not offer the truck");
    } finally {
      setOffering(false);
    }
  }

  if (error) return <GlassCard size="sm" className="mt-2"><p className="text-xs text-amber-300">{error}</p></GlassCard>;
  if (!home) return <GlassCard size="sm" className="mt-2"><p className="text-xs text-white/40">Loading…</p></GlassCard>;

  const rooms = new Map<string, HomeItem[]>();
  for (const i of home.items) rooms.set(i.room, [...(rooms.get(i.room) ?? []), i]);
  const volume = home.items.reduce((n, i) => n + i.volume_l * i.qty, 0);
  const weight = home.items.reduce((n, i) => n + i.weight_kg * i.qty, 0);
  const count = home.items.reduce((n, i) => n + i.qty, 0);
  const dismantle = home.items.filter((i) => i.dismantle).reduce((n, i) => n + i.qty, 0);
  const packing = home.items.filter((i) => i.packing).reduce((n, i) => n + i.qty, 0);
  const p = home.property;

  return (
    <GlassCard size="sm" className="mt-2 space-y-3">
      <div className="grid grid-cols-1 gap-2 text-xs sm:grid-cols-2">
        <div>
          <p className="text-white/35">Property</p>
          <p className="capitalize text-white/85">{p.type} · {p.size}</p>
        </div>
        <div>
          <p className="text-white/35">Plan</p>
          <p className="text-white/85">{home.plan === "trips" ? "Fewer trucks, more trips" : "One truck per load"} · {(home.distance_centikm / 100).toFixed(1)} km</p>
        </div>
        <div>
          <p className="text-white/35">Pick-up</p>
          <p className="text-white/85">{floorLine(p.pickup_floor, p.pickup_has_lift)}{p.long_carry ? " · long carry" : ""}</p>
        </div>
        <div>
          <p className="text-white/35">Drop-off</p>
          <p className="text-white/85">{floorLine(p.dropoff_floor, p.dropoff_has_lift)}</p>
        </div>
        {home.survey_at && (
          <div>
            <p className="text-white/35">Survey</p>
            <p className="font-mono text-cyan-300">{when(home.survey_at)}</p>
          </div>
        )}
        <div>
          <p className="text-white/35">Move</p>
          <p className="font-mono text-cyan-300">{when(home.move_at)}</p>
        </div>
      </div>

      <div className="rounded-lg border border-white/5 p-2 text-xs">
        <p className="text-white/35">Team</p>
        <p className="text-white/85">
          {lead ? `Team ${lead}` : "No lead yet"}
          {home.trucks ? ` · ${home.trucks} truck${home.trucks === 1 ? "" : "s"} & ${home.crew_total ?? "?"}-person crew` : ""}
        </p>
        <dl className="mt-2 grid grid-cols-1 gap-x-4 gap-y-1 sm:grid-cols-2">
          {payLines(home, pay).map(([k, v]) => (
            <div key={k} className="flex justify-between gap-2">
              <dt className="text-white/40">{k}</dt>
              <dd className="font-mono text-white/80">{v}</dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="rounded-lg border border-white/5 p-2">
        <p className="text-[11px] uppercase tracking-wide text-white/35">Another truck</p>
        <p className="mt-1 text-xs text-white/45">
          A joint move's second truck, or an extra one an addendum needs. Offered to leads free that day; the automatic paths do this themselves.
        </p>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <select
            value={slot}
            onChange={(e) => setSlot(e.target.value === "support" ? "support" : "emergency")}
            className="rounded-lg border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white"
          >
            <option value="emergency">Extra truck (today)</option>
            <option value="support">Support lead (2nd truck)</option>
          </select>
          <input
            value={slotPay}
            onChange={(e) => setSlotPay(e.target.value)}
            inputMode="decimal"
            placeholder={`Pay per truck (${home.currency ?? "PHP"})`}
            className="w-44 rounded-lg border border-white/10 bg-white/[0.03] px-2 py-1.5 text-xs text-white placeholder:text-white/30"
          />
          <button
            type="button"
            disabled={offering}
            onClick={() => void offerTruck()}
            className="rounded-lg bg-cyan-400/90 px-3 py-1.5 text-xs font-medium text-[#041a1f] hover:bg-cyan-300 disabled:opacity-40"
          >
            {offering ? "Offering…" : "Offer it"}
          </button>
        </div>
      </div>

      <p className="text-xs text-white/55">
        {count} items · {(volume / 1000).toFixed(1)} m³ · {weight.toLocaleString()} kg
        {dismantle > 0 ? ` · ${dismantle} to dismantle` : ""}
        {packing > 0 ? ` · ${packing} special packing` : ""}
      </p>

      <div className="divide-y divide-white/5 border-t border-white/5">
        {[...rooms.entries()].map(([room, items]) => (
          <div key={room} className="py-2">
            <p className="text-[11px] uppercase tracking-wide text-white/35">{room}</p>
            <ul className="mt-1 space-y-0.5">
              {items.map((i) => (
                <li key={`${room}-${i.name}`} className="flex justify-between gap-3 text-xs text-white/75">
                  <span className="min-w-0 truncate">
                    {i.qty} × {i.name}
                    {i.dismantle ? " · dismantle" : ""}
                    {i.packing ? " · pack" : ""}
                  </span>
                  <span className="shrink-0 font-mono text-white/40">{((i.volume_l * i.qty) / 1000).toFixed(1)} m³</span>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </GlassCard>
  );
}
