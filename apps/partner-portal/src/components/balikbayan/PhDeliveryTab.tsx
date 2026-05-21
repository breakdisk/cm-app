"use client";
import { useState } from "react";
import { Plus, Trash2, Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import type { PhDeliveryZone, BoxSize } from "@/lib/api/balikbayan-rates";

interface Props {
  zones: PhDeliveryZone[];
  boxSizes: BoxSize[];
  editing: boolean;
  onChange: (zones: PhDeliveryZone[]) => void;
}

const REGION_PREFIXES: Record<string, string> = {
  "Zone 1": "NCR / Metro Manila",
  "Zone 2": "Nearby Luzon",
  "Zone 3": "Northern Luzon",
  "Zone 4": "Southern Luzon / Bicol",
  "Zone 5": "Visayas",
  "Zone 6": "Mindanao",
  "Zone 7": "Remote Islands",
};

function emptyZone(boxSizes: BoxSize[]): PhDeliveryZone {
  const prices: Record<string, number> = {};
  boxSizes.forEach((s) => { prices[s.id] = 0; });
  return { zone_code: "", zone_name: "", coverage: "", prices, transit_days: "" };
}

function regionOf(code: string): string {
  const prefix = Object.keys(REGION_PREFIXES).find((p) => code.startsWith(p));
  return prefix ? REGION_PREFIXES[prefix] : "Other";
}

function exportCsv(zones: PhDeliveryZone[], boxSizes: BoxSize[]) {
  const header = ["zone_code", "zone_name", "coverage", ...boxSizes.map((s) => s.id), "transit_days"].join(",");
  const rows   = zones.map((z) =>
    [`"${z.zone_code}"`, `"${z.zone_name}"`, `"${z.coverage}"`, ...boxSizes.map((s) => z.prices[s.id] ?? 0), `"${z.transit_days}"`].join(",")
  );
  const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-ph-delivery.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

export function PhDeliveryTab({ zones, boxSizes, editing, onChange }: Props) {
  const [search, setSearch] = useState("");

  const colTemplate = `80px 160px 1fr ${boxSizes.map(() => "90px").join(" ")} 90px 36px`;

  function patchMeta(idx: number, key: "zone_code" | "zone_name" | "coverage" | "transit_days", value: string) {
    const next = zones.slice(); next[idx] = { ...next[idx], [key]: value }; onChange(next);
  }
  function patchPrice(idx: number, sizeId: string, value: number) {
    const next = zones.slice();
    next[idx] = { ...next[idx], prices: { ...next[idx].prices, [sizeId]: value } };
    onChange(next);
  }
  function remove(idx: number) { onChange(zones.filter((_, i) => i !== idx)); }

  const filtered = search.trim()
    ? zones.filter((z) =>
        z.zone_name.toLowerCase().includes(search.toLowerCase()) ||
        z.coverage.toLowerCase().includes(search.toLowerCase()) ||
        z.zone_code.toLowerCase().includes(search.toLowerCase())
      )
    : zones;

  const grouped = filtered.reduce<Record<string, PhDeliveryZone[]>>((acc, z) => {
    const r = regionOf(z.zone_code);
    (acc[r] ??= []).push(z);
    return acc;
  }, {});

  const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-green-signal/40 w-full";

  return (
    <GlassCard padding="none">
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Philippine Local Delivery Zones</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            From port of Manila / Cebu → to recipient's province · always <span className="text-green-signal font-bold">₱ PHP</span> · added on top of sea/air freight
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={search} onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter zone / province…"
            className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white placeholder-white/30 outline-none focus:border-green-signal/40 w-44"
          />
          {!editing && (
            <button onClick={() => exportCsv(zones, boxSizes)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
              <Download size={12} /> CSV
            </button>
          )}
        </div>
      </div>

      <div className="overflow-x-auto">
        {/* Column headers */}
        <div
          className="grid gap-2 px-5 py-2.5 border-b border-glass-border min-w-max"
          style={{ gridTemplateColumns: colTemplate }}
        >
          {["Code", "Zone Name", "Coverage", ...boxSizes.map((s) => s.name), "Transit", ""].map((h, i) => (
            <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center first:text-left">{h}</span>
          ))}
        </div>

        {Object.entries(grouped).map(([region, regionZones]) => (
          <div key={region}>
            <div className="px-5 py-2 bg-green-signal/5 border-b border-glass-border/40">
              <span className="text-2xs font-mono font-bold text-green-signal uppercase tracking-widest">{region}</span>
            </div>
            {regionZones.map((z) => {
              const globalIdx = zones.indexOf(z);
              return (
                <div
                  key={globalIdx}
                  className="grid gap-2 items-center px-5 py-2.5 border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors min-w-max"
                  style={{ gridTemplateColumns: colTemplate }}
                >
                  {editing ? (
                    <input value={z.zone_code} onChange={(e) => patchMeta(globalIdx, "zone_code", e.target.value)} placeholder="Zone 1A" className={inputCls} />
                  ) : (
                    <span className="text-2xs font-mono font-bold text-white/60">{z.zone_code}</span>
                  )}
                  {editing ? (
                    <input value={z.zone_name} onChange={(e) => patchMeta(globalIdx, "zone_name", e.target.value)} placeholder="Metro Manila" className={inputCls} />
                  ) : (
                    <span className="text-xs font-medium text-white">{z.zone_name}</span>
                  )}
                  {editing ? (
                    <input value={z.coverage} onChange={(e) => patchMeta(globalIdx, "coverage", e.target.value)} placeholder="Provinces…" className={inputCls} />
                  ) : (
                    <span className="text-2xs font-mono text-white/40 truncate" title={z.coverage}>{z.coverage}</span>
                  )}
                  {boxSizes.map((s) =>
                    editing ? (
                      <input key={s.id} type="number" min={0} step={50} value={z.prices[s.id] ?? 0}
                        onChange={(e) => patchPrice(globalIdx, s.id, parseFloat(e.target.value || "0"))}
                        className={inputCls + " text-center"} />
                    ) : (
                      <span key={s.id} className="text-xs font-bold font-mono text-green-signal text-center">
                        ₱{(z.prices[s.id] ?? 0).toLocaleString()}
                      </span>
                    )
                  )}
                  {editing ? (
                    <input value={z.transit_days} onChange={(e) => patchMeta(globalIdx, "transit_days", e.target.value)} placeholder="1–2 days" className={inputCls + " text-center"} />
                  ) : (
                    <span className="text-2xs font-mono text-white/40 text-center">{z.transit_days}</span>
                  )}
                  <button onClick={() => editing && remove(globalIdx)} disabled={!editing}
                    className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors disabled:opacity-0">
                    <Trash2 size={11} />
                  </button>
                </div>
              );
            })}
          </div>
        ))}
      </div>

      {editing && !search.trim() && (
        <div className="px-5 py-3">
          <button onClick={() => onChange([...zones, emptyZone(boxSizes)])}
            className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-signal/5 px-3 py-1.5 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors">
            <Plus size={12} /> Add zone
          </button>
        </div>
      )}

      <div className="px-5 py-2 border-t border-glass-border/40">
        <p className="text-2xs font-mono text-white/20">
          ★ PH local delivery is always charged in Philippine Peso (₱). Transit days start from port arrival.
          BARMM zones may require additional security clearance. Island deliveries subject to vessel schedule.
        </p>
      </div>
    </GlassCard>
  );
}
