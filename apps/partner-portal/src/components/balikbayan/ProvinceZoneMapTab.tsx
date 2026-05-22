"use client";
import { useState } from "react";
import { Plus, Trash2, Download, Map } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import type { PhProvinceZoneEntry, PhDeliveryZone } from "@/lib/api/balikbayan-rates";

interface Props {
  entries: PhProvinceZoneEntry[];
  zones: PhDeliveryZone[];       // for zone_code dropdown options
  editing: boolean;
  onChange: (entries: PhProvinceZoneEntry[]) => void;
}

function exportCsv(entries: PhProvinceZoneEntry[]) {
  const header = "province,zone_code";
  const rows = entries.map((e) => [`"${e.province}"`, `"${e.zone_code}"`].join(","));
  const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "province-zone-map.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-green-signal/40 w-full";

export function ProvinceZoneMapTab({ entries, zones, editing, onChange }: Props) {
  const [search, setSearch] = useState("");

  function patch(idx: number, key: keyof PhProvinceZoneEntry, value: string) {
    const next = entries.slice();
    next[idx] = { ...next[idx], [key]: value };
    onChange(next);
  }
  function remove(idx: number) { onChange(entries.filter((_, i) => i !== idx)); }

  const zoneName = (code: string) => zones.find((z) => z.zone_code === code)?.zone_name ?? "";

  const filtered = search.trim()
    ? entries.filter((e) =>
        e.province.toLowerCase().includes(search.toLowerCase()) ||
        e.zone_code.toLowerCase().includes(search.toLowerCase()) ||
        zoneName(e.zone_code).toLowerCase().includes(search.toLowerCase())
      )
    : entries;

  // Read mode: group by zone_code
  const grouped = filtered.reduce<Record<string, PhProvinceZoneEntry[]>>((acc, e) => {
    (acc[e.zone_code] ??= []).push(e);
    return acc;
  }, {});
  const sortedZones = Object.keys(grouped).sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));

  // Edit mode: flat list sorted by zone_code, then province
  const sortedFlat = editing
    ? [...filtered].sort((a, b) => {
        const z = a.zone_code.localeCompare(b.zone_code, undefined, { numeric: true });
        return z !== 0 ? z : a.province.localeCompare(b.province);
      })
    : [];

  return (
    <GlassCard padding="none">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Province → Zone Map</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            Maps recipient province or city → PH delivery zone · used by Quote Calculator and public API · {entries.length} entries
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={search} onChange={(e) => setSearch(e.target.value)}
            placeholder="Search province / zone…"
            className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white placeholder-white/30 outline-none focus:border-green-signal/40 w-44"
          />
          {!editing && entries.length > 0 && (
            <button onClick={() => exportCsv(entries)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
              <Download size={12} /> CSV
            </button>
          )}
        </div>
      </div>

      {filtered.length === 0 && (
        <p className="px-5 py-10 text-center text-xs text-white/30 font-mono">No entries match your search.</p>
      )}

      {/* ── Edit mode — flat sortable list ───────────────────────────────────────── */}
      {editing && (
        <>
          {/* Column headers */}
          <div className="grid grid-cols-[1fr_200px_36px] gap-2 px-5 py-2.5 border-b border-glass-border">
            {["Province / City", "Delivery Zone", ""].map((h, i) => (
              <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>
          <div className="divide-y divide-glass-border/30">
            {sortedFlat.map((e) => {
              const globalIdx = entries.indexOf(e);
              return (
                <div key={globalIdx} className="grid grid-cols-[1fr_200px_36px] gap-2 items-center px-5 py-2 hover:bg-glass-100/40 transition-colors">
                  <input
                    value={e.province}
                    onChange={(ev) => patch(globalIdx, "province", ev.target.value)}
                    placeholder="Province or city name"
                    className={inputCls}
                  />
                  <select
                    value={e.zone_code}
                    onChange={(ev) => patch(globalIdx, "zone_code", ev.target.value)}
                    className={inputCls}
                  >
                    <option value="" style={{ background: "#0d1422" }}>— select zone —</option>
                    {zones.map((z) => (
                      <option key={z.zone_code} value={z.zone_code} style={{ background: "#0d1422" }}>
                        {z.zone_code} — {z.zone_name}
                      </option>
                    ))}
                  </select>
                  <button
                    onClick={() => remove(globalIdx)}
                    className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors"
                  >
                    <Trash2 size={11} />
                  </button>
                </div>
              );
            })}
          </div>
          <div className="px-5 py-3 border-t border-glass-border/40">
            <button
              onClick={() => onChange([...entries, { province: "", zone_code: zones[0]?.zone_code ?? "" }])}
              className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-signal/5 px-3 py-1.5 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors"
            >
              <Plus size={12} /> Add entry
            </button>
          </div>
        </>
      )}

      {/* ── Read mode — grouped by zone as pill chips ─────────────────────────── */}
      {!editing && filtered.length > 0 && (
        <div className="divide-y divide-glass-border/30">
          {sortedZones.map((zoneCode) => {
            const name = zoneName(zoneCode);
            return (
              <div key={zoneCode} className="px-5 py-3">
                <div className="flex items-center gap-2 mb-2">
                  <Map size={11} className="text-green-signal flex-shrink-0" />
                  <span className="text-xs font-bold font-mono text-green-signal">{zoneCode}</span>
                  {name && <span className="text-2xs font-mono text-white/40">— {name}</span>}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {grouped[zoneCode].map((e, i) => (
                    <span
                      key={i}
                      className="rounded-full border border-glass-border/60 bg-glass-100/40 px-2.5 py-0.5 text-2xs font-mono text-white/60 hover:text-white/80 transition-colors"
                    >
                      {e.province}
                    </span>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}

      <div className="px-5 py-2 border-t border-glass-border/40">
        <p className="text-2xs font-mono text-white/20">
          ★ Matching is case-insensitive. Add city names as separate entries to override their province zone.
          E.g., &quot;Cebu City&quot; → Zone 5A, &quot;Cebu&quot; → Zone 5B. City name matched before province name.
        </p>
      </div>
    </GlassCard>
  );
}
