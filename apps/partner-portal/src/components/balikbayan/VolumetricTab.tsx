"use client";
import { Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { CurrencySelect } from "@/components/balikbayan/CurrencySelect";
import { fmtCurrency } from "@/lib/data/currencies";
import type { VolumetricGroup, VolumetricGroupName } from "@/lib/api/balikbayan-rates";

interface Props {
  groups: VolumetricGroup[];
  editing: boolean;
  onChange: (groups: VolumetricGroup[]) => void;
}

const GROUP_ORDER: VolumetricGroupName[] = ["Manila", "Luzon", "Visayas", "Mindanao", "Islands"];

const GROUP_META: Record<VolumetricGroupName, { color: string; desc: string; coverage: string }> = {
  Manila:   { color: "text-cyan-neon",    desc: "Metro Manila / NCR",          coverage: "All NCR cities: Makati, QC, Manila, Pasig, Taguig, Parañaque, Caloocan…" },
  Luzon:    { color: "text-purple-plasma",desc: "Luzon island (outside NCR)",   coverage: "Ilocos, Cagayan, Central Luzon, CALABARZON, Bicol, MIMAROPA, Palawan" },
  Visayas:  { color: "text-green-signal", desc: "Visayas island group",         coverage: "Cebu, Iloilo, Bacolod, Leyte, Samar, Bohol, Aklan (Boracay), Siquijor" },
  Mindanao: { color: "text-amber-signal", desc: "Mindanao island group",        coverage: "Davao, CDO, GenSan, Zamboanga, Cotabato, Lanao, Caraga, BARMM" },
  Islands:  { color: "text-red-signal",   desc: "Remote island provinces",      coverage: "Sulu, Tawi-Tawi, Basilan, Batanes, Camiguin, Dinagat and other remote islands" },
};

function exportCsv(groups: VolumetricGroup[]) {
  const header = "group,currency,divisor,base_rate,rate_per_cbm,min_charge,surcharge_pct";
  const rows = groups.map((g) =>
    [g.group, g.currency ?? "USD", g.divisor, g.base_rate_usd, g.rate_per_cbm_usd, g.min_charge_usd, g.surcharge_pct].join(",")
  );
  const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-volumetric.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-sm text-white font-mono outline-none focus:border-cyan-neon/40 w-full";

export function VolumetricTab({ groups, editing, onChange }: Props) {
  function patch(groupName: VolumetricGroupName, key: keyof VolumetricGroup, value: string | number) {
    onChange(groups.map((g) => g.group === groupName ? { ...g, [key]: value } : g));
  }

  const orderedGroups = GROUP_ORDER.map((name) => groups.find((g) => g.group === name)).filter(Boolean) as VolumetricGroup[];

  return (
    <div className="flex flex-col gap-4">
      {/* Info banner */}
      <GlassCard glow="cyan" accent>
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-mono text-cyan-neon font-bold">VOLUMETRIC PRICING — HOW IT WORKS</p>
            <p className="text-2xs font-mono text-white/50 mt-1">
              Volumetric Weight (kg) = L(cm) × W(cm) × H(cm) ÷ Divisor &nbsp;|&nbsp;
              Chargeable Weight = MAX(actual kg, volumetric kg) &nbsp;|&nbsp;
              Charge = Base Rate + (Chargeable Weight × Rate/CBM × 0.001) &nbsp;|&nbsp;
              Minimum charge applies per box
            </p>
          </div>
          {!editing && (
            <button onClick={() => exportCsv(groups)}
              className="flex flex-shrink-0 items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
              <Download size={12} /> CSV
            </button>
          )}
        </div>
      </GlassCard>

      {/* One card per group */}
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {orderedGroups.map((g) => {
          const meta = GROUP_META[g.group];
          const cur  = g.currency ?? "USD";
          return (
            <GlassCard key={g.group} padding="none"
              glow={g.group === "Manila" ? "cyan" : g.group === "Luzon" ? "purple" : g.group === "Visayas" ? "green" : g.group === "Mindanao" ? "amber" : "red"}
              accent
            >
              {/* Group header */}
              <div className="px-5 pt-4 pb-3 border-b border-glass-border/60">
                <div className="flex items-center justify-between gap-2">
                  <h4 className={`font-heading text-base font-bold ${meta.color}`}>{g.group}</h4>
                  {editing ? (
                    <CurrencySelect
                      value={cur}
                      onChange={(code) => patch(g.group, "currency", code)}
                      className="rounded-md border border-glass-border bg-glass-100 px-2 py-1 text-xs text-white outline-none focus:border-cyan-neon/40 w-20"
                    />
                  ) : (
                    <span className="text-2xs font-mono font-bold text-white/40 border border-glass-border/40 rounded px-1.5 py-0.5">{cur}</span>
                  )}
                </div>
                <p className="text-2xs font-mono text-white/40 mt-0.5">{meta.desc}</p>
                <p className="text-2xs font-mono text-white/25 mt-1 leading-relaxed">{meta.coverage}</p>
              </div>

              {/* Fields */}
              <div className="divide-y divide-glass-border/40">
                {([
                  { label: "Volumetric Divisor",    key: "divisor",          suffix: "cm³/kg",  step: 100,   fmt: (v: number) => v.toLocaleString() + " cm³/kg"           },
                  { label: "Base Rate",              key: "base_rate_usd",    step: 1,            fmt: (v: number) => fmtCurrency(v, cur)                                    },
                  { label: "Rate per CBM",           key: "rate_per_cbm_usd", step: 1,            fmt: (v: number) => fmtCurrency(v, cur)                                    },
                  { label: "Minimum Charge",         key: "min_charge_usd",   step: 1,            fmt: (v: number) => fmtCurrency(v, cur)                                    },
                  { label: "Area Surcharge",         key: "surcharge_pct",    step: 0.01,         fmt: (v: number) => `${(v*100).toFixed(0)}%`                               },
                ] as const).map(({ label, key, fmt, step }) => (
                  <div key={key} className="flex items-center justify-between gap-3 px-5 py-3">
                    <span className="text-2xs font-mono text-white/40 uppercase tracking-wider">{label}</span>
                    {editing ? (
                      <input
                        type="number" min={0} step={step}
                        value={g[key]}
                        onChange={(e) => patch(g.group, key, parseFloat(e.target.value || "0"))}
                        className={inputCls + " w-28 text-right"}
                      />
                    ) : (
                      <span className={`text-sm font-bold font-mono ${meta.color}`}>{fmt(g[key] as number)}</span>
                    )}
                  </div>
                ))}
              </div>

              {/* Sample calculation */}
              {!editing && (
                <div className="px-5 py-3 bg-glass-100/40 border-t border-glass-border/40">
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-wider mb-1">Sample — 50×40×40cm box, 5 kg actual</p>
                  {(() => {
                    const vol      = (50 * 40 * 40) / g.divisor;
                    const charge   = Math.max(g.min_charge_usd, g.base_rate_usd + Math.max(5, vol) * g.rate_per_cbm_usd * 0.001);
                    const total    = charge * (1 + g.surcharge_pct);
                    return (
                      <p className={`text-xs font-mono font-bold ${meta.color}`}>
                        Vol={vol.toFixed(2)} kg · Est. {fmtCurrency(total, cur)}
                      </p>
                    );
                  })()}
                </div>
              )}
            </GlassCard>
          );
        })}
      </div>

      <div className="px-1">
        <p className="text-2xs font-mono text-white/20">
          ★ Volumetric pricing applies when a box is large but light. The higher of actual weight and volumetric weight
          is always charged. Area surcharge is applied on top of the computed charge. Each group can have its own billing currency.
        </p>
      </div>
    </div>
  );
}
