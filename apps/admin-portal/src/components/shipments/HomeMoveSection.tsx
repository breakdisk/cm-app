"use client";
/**
 * The whole-home detail on a home-move shipment: the property, the survey and
 * move times, and the inventory as it was priced — what ops need to assign a
 * crew. Read from `GET /v1/shipments/:id/home`, which admits tenant-wide staff.
 */
import { useEffect, useState } from "react";
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
}

function when(iso: string): string {
  return new Date(iso).toLocaleString(undefined, { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
}

function floorLine(floor: number, lift: boolean): string {
  return `${FLOORS[floor] ?? floor} floor${lift ? ", lift" : floor > 0 ? ", no lift" : ""}`;
}

export function HomeMoveSection({ shipmentId }: { shipmentId: string }) {
  const [home, setHome] = useState<HomeMove | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setHome(null);
    setError(null);
    authFetch(`${API_BASE}/v1/shipments/${shipmentId}/home`)
      .then(async (r) => {
        if (!r.ok) throw new Error(r.status === 404 ? "No home-move detail recorded" : `${r.status} ${r.statusText}`);
        const j = await r.json();
        if (!cancelled) setHome(j.data);
      })
      .catch((e) => { if (!cancelled) setError(e instanceof Error ? e.message : "Could not load the home move"); });
    return () => { cancelled = true; };
  }, [shipmentId]);

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
