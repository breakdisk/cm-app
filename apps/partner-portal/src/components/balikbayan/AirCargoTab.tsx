"use client";
import { Plus, Trash2, Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import type { AirCargoFixed, AirCargoZone } from "@/lib/api/balikbayan-rates";

interface Props {
  zones: AirCargoZone[];
  fixed: AirCargoFixed;
  editing: boolean;
  onZonesChange: (zones: AirCargoZone[]) => void;
  onFixedChange: (fixed: AirCargoFixed) => void;
}

function emptyZone(): AirCargoZone {
  return { zone_name: "", origins: "", rate_per_kg_usd: 0, distance_surcharge_per_km: 0, transit_days: "" };
}

function exportCsv(zones: AirCargoZone[], fixed: AirCargoFixed) {
  const zoneHeader = "zone_name,origins,rate_per_kg_usd,distance_surcharge_per_km,transit_days";
  const zoneRows   = zones.map((z) =>
    [`"${z.zone_name}"`, `"${z.origins}"`, z.rate_per_kg_usd, z.distance_surcharge_per_km, z.transit_days].join(",")
  );
  const fixedBlock = [
    "",
    "FIXED CHARGES",
    `AWB Fee,$${fixed.awb_fee_usd}`,
    `Fuel Surcharge,${(fixed.fuel_surcharge_pct * 100).toFixed(0)}%`,
    `THC,$${fixed.thc_usd}`,
    `PH Customs,$${fixed.customs_clearance_usd}`,
    `Min Weight,${fixed.min_weight_kg} kg`,
    `Volumetric Divisor,${fixed.volumetric_divisor}`,
  ];
  const blob = new Blob([[zoneHeader, ...zoneRows, ...fixedBlock].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-air-cargo.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-purple-plasma/60 w-full";

export function AirCargoTab({ zones, fixed, editing, onZonesChange, onFixedChange }: Props) {
  function patchZone(idx: number, key: keyof AirCargoZone, value: string | number) {
    const next = zones.slice(); next[idx] = { ...next[idx], [key]: value }; onZonesChange(next);
  }
  function removeZone(idx: number) { onZonesChange(zones.filter((_, i) => i !== idx)); }
  function patchFixed(key: keyof AirCargoFixed, value: number) {
    onFixedChange({ ...fixed, [key]: value });
  }

  return (
    <div className="flex flex-col gap-4">
      {/* Formula banner */}
      <GlassCard glow="purple" accent>
        <p className="text-xs font-mono text-purple-plasma">
          FORMULA: Total Air Freight = (Chargeable Weight × Zone Rate/kg) + (Distance km × Dist. Surcharge) + AWB $
          {fixed.awb_fee_usd} + FSC {(fixed.fuel_surcharge_pct * 100).toFixed(0)}% + THC ${fixed.thc_usd}
        </p>
        <p className="text-2xs font-mono text-white/30 mt-1">
          Chargeable Weight = MAX(actual kg, volumetric kg) · Volumetric = L×W×H (cm) ÷ {fixed.volumetric_divisor}
        </p>
      </GlassCard>

      {/* Zone rate table */}
      <GlassCard padding="none">
        <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
          <div>
            <h3 className="font-heading text-sm font-semibold text-white">Zone Rates per kg + Distance Surcharge</h3>
            <p className="text-2xs font-mono text-white/30 mt-0.5">FROM origin country → TO NAIA / Mactan / Clark</p>
          </div>
          {!editing && (
            <button onClick={() => exportCsv(zones, fixed)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
              <Download size={12} /> CSV
            </button>
          )}
        </div>

        <div className="grid grid-cols-[160px_1fr_110px_130px_90px_36px] gap-2 px-5 py-2.5 border-b border-glass-border">
          {["Zone", "Origin Countries / Regions", "Rate / kg (USD)", "Dist. Surcharge / km", "Transit", ""].map((h, i) => (
            <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
          ))}
        </div>

        {zones.map((z, idx) => (
          <div key={idx} className="grid grid-cols-[160px_1fr_110px_130px_90px_36px] gap-2 items-center px-5 py-2.5 border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors">
            {editing ? (
              <input value={z.zone_name} onChange={(e) => patchZone(idx, "zone_name", e.target.value)} placeholder="Zone name" className={inputCls} />
            ) : (
              <span className="text-xs font-bold text-purple-plasma">{z.zone_name}</span>
            )}

            {editing ? (
              <input value={z.origins} onChange={(e) => patchZone(idx, "origins", e.target.value)} placeholder="Countries…" className={inputCls} />
            ) : (
              <span className="text-2xs font-mono text-white/50 truncate" title={z.origins}>{z.origins}</span>
            )}

            {editing ? (
              <input type="number" min={0} step={0.5} value={z.rate_per_kg_usd}
                onChange={(e) => patchZone(idx, "rate_per_kg_usd", parseFloat(e.target.value || "0"))}
                className={inputCls} />
            ) : (
              <span className="text-xs font-bold font-mono text-cyan-neon">${z.rate_per_kg_usd.toFixed(2)}</span>
            )}

            {editing ? (
              <input type="number" min={0} step={0.001} value={z.distance_surcharge_per_km}
                onChange={(e) => patchZone(idx, "distance_surcharge_per_km", parseFloat(e.target.value || "0"))}
                className={inputCls} />
            ) : (
              <span className="text-xs font-mono text-white/60">${z.distance_surcharge_per_km.toFixed(3)}</span>
            )}

            {editing ? (
              <input value={z.transit_days} onChange={(e) => patchZone(idx, "transit_days", e.target.value)} placeholder="7–12" className={inputCls} />
            ) : (
              <span className="text-xs font-mono text-white/50 text-center">{z.transit_days}</span>
            )}

            <button onClick={() => editing && removeZone(idx)} disabled={!editing}
              className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors disabled:opacity-0">
              <Trash2 size={11} />
            </button>
          </div>
        ))}

        {editing && (
          <div className="px-5 py-3">
            <button onClick={() => onZonesChange([...zones, emptyZone()])}
              className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/30 bg-purple-plasma/5 px-3 py-1.5 text-xs font-medium text-purple-plasma hover:border-purple-plasma/60 transition-colors">
              <Plus size={12} /> Add zone
            </button>
          </div>
        )}
      </GlassCard>

      {/* Fixed charges */}
      <GlassCard padding="none">
        <div className="px-5 py-4 border-b border-glass-border">
          <h3 className="font-heading text-sm font-semibold text-white">Fixed Charges — Applied to Every Shipment</h3>
        </div>

        <div className="grid grid-cols-2 gap-0 divide-y divide-glass-border/40 sm:grid-cols-3">
          {([
            { label: "AWB Fee",             key: "awb_fee_usd",           fmt: (v: number) => `$${v}`,                 step: 1 },
            { label: "Fuel Surcharge (FSC)",key: "fuel_surcharge_pct",    fmt: (v: number) => `${(v*100).toFixed(0)}%`,step: 0.01 },
            { label: "THC (Origin Airport)",key: "thc_usd",               fmt: (v: number) => `$${v}`,                 step: 1 },
            { label: "PH Customs Clearance",key: "customs_clearance_usd", fmt: (v: number) => `$${v}`,                 step: 1 },
            { label: "Min Chargeable Wt",   key: "min_weight_kg",         fmt: (v: number) => `${v} kg`,               step: 1 },
            { label: "Volumetric Divisor",  key: "volumetric_divisor",    fmt: (v: number) => `${v.toLocaleString()} cm³/kg`, step: 100 },
          ] as const).map(({ label, key, fmt, step }) => (
            <div key={key} className="flex flex-col gap-1 px-5 py-4">
              <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">{label}</span>
              {editing ? (
                <input
                  type="number" min={0} step={step}
                  value={fixed[key]}
                  onChange={(e) => patchFixed(key, parseFloat(e.target.value || "0"))}
                  className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-purple-plasma/60 w-28"
                />
              ) : (
                <span className="text-sm font-bold font-mono text-cyan-neon">{fmt(fixed[key] as number)}</span>
              )}
            </div>
          ))}
        </div>
      </GlassCard>
    </div>
  );
}
