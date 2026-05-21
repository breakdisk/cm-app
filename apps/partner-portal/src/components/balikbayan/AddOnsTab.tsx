"use client";
import { useState } from "react";
import { Plus, Trash2, Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { CurrencySelect } from "@/components/balikbayan/CurrencySelect";
import { fmtCurrency } from "@/lib/data/currencies";
import type { AddOnService, AddOnCategory, AddOnRateType } from "@/lib/api/balikbayan-rates";

interface Props {
  addons: AddOnService[];
  editing: boolean;
  onChange: (addons: AddOnService[]) => void;
}

const CATEGORIES: AddOnCategory[]  = ["insurance", "crate", "packing", "surcharge"];
const RATE_TYPES: AddOnRateType[]  = ["fixed", "percent", "per_kg", "per_day"];

const CAT_BADGE: Record<AddOnCategory, "cyan" | "purple" | "green" | "amber"> = {
  insurance: "cyan",
  crate:     "purple",
  packing:   "green",
  surcharge: "amber",
};

const CAT_LABELS: Record<AddOnCategory, string> = {
  insurance: "Insurance",
  crate:     "Wooden Crate",
  packing:   "Packing",
  surcharge: "Surcharge",
};

function emptyAddon(): AddOnService {
  return { id: `addon-${Date.now()}`, name: "", category: "packing", currency: "USD", rate: 0, rate_type: "fixed", min_charge: 0, description: "" };
}

function formatRate(addon: AddOnService): string {
  const cur = addon.currency ?? "USD";
  if (addon.rate_type === "percent") return `${(addon.rate * 100).toFixed(addon.rate < 0.01 ? 2 : 1)}%`;
  if (addon.rate_type === "per_kg")  return `${fmtCurrency(addon.rate, cur)}/kg`;
  if (addon.rate_type === "per_day") return `${fmtCurrency(addon.rate, cur)}/day`;
  return fmtCurrency(addon.rate, cur);
}

function exportCsv(addons: AddOnService[]) {
  const header = "id,name,category,currency,rate,rate_type,min_charge,description";
  const rows = addons.map((a) =>
    [a.id, `"${a.name}"`, a.category, a.currency ?? "USD", a.rate, a.rate_type, a.min_charge, `"${a.description}"`].join(",")
  );
  const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-addons.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

// grid: Name | Category | Currency | Rate | Rate Type | Min Charge | Description | Delete
const GRID = "1fr 90px 70px 80px 90px 90px 1fr 36px";
const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-amber-signal/40 w-full";

export function AddOnsTab({ addons, editing, onChange }: Props) {
  const [activeCategory, setActiveCategory] = useState<AddOnCategory | "all">("all");

  function patch(idx: number, key: keyof AddOnService, value: string | number) {
    const next = addons.slice(); next[idx] = { ...next[idx], [key]: value } as AddOnService; onChange(next);
  }
  function remove(idx: number) { onChange(addons.filter((_, i) => i !== idx)); }

  const visible = activeCategory === "all"
    ? addons
    : addons.filter((a) => a.category === activeCategory);

  return (
    <GlassCard padding="none">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Add-On Services</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            Optional charges added on top of sea or air freight rate · per box unless stated · per-service currency
          </p>
        </div>
        {!editing && (
          <button onClick={() => exportCsv(addons)}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={12} /> CSV
          </button>
        )}
      </div>

      {/* Category filter tabs */}
      <div className="flex gap-2 px-5 py-3 border-b border-glass-border overflow-x-auto">
        {(["all", ...CATEGORIES] as const).map((cat) => (
          <button
            key={cat}
            onClick={() => setActiveCategory(cat)}
            className={[
              "rounded-full border px-3 py-1 text-2xs font-mono uppercase tracking-wider whitespace-nowrap transition-colors",
              activeCategory === cat
                ? "border-amber-signal/60 bg-amber-signal/10 text-amber-signal"
                : "border-glass-border text-white/40 hover:text-white/70",
            ].join(" ")}
          >
            {cat === "all" ? `All (${addons.length})` : `${CAT_LABELS[cat]} (${addons.filter((a) => a.category === cat).length})`}
          </button>
        ))}
      </div>

      {/* Column headers */}
      <div className={`grid grid-cols-[${GRID}] gap-2 px-5 py-2.5 border-b border-glass-border`}>
        {["Name", "Category", "Currency", "Rate", "Rate Type", "Min Charge", "Description", ""].map((h, i) => (
          <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
        ))}
      </div>

      {visible.length === 0 && (
        <p className="px-5 py-10 text-center text-xs text-white/30 font-mono">No add-ons in this category.</p>
      )}

      {visible.map((a) => {
        const globalIdx = addons.indexOf(a);
        const cur = a.currency ?? "USD";
        return (
          <div key={a.id}
            className={`grid grid-cols-[${GRID}] gap-2 items-center px-5 py-2.5 border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors`}>

            {editing ? (
              <input value={a.name} onChange={(e) => patch(globalIdx, "name", e.target.value)} placeholder="Service name" className={inputCls} />
            ) : (
              <span className="text-xs font-medium text-white">{a.name}</span>
            )}

            {editing ? (
              <select value={a.category} onChange={(e) => patch(globalIdx, "category", e.target.value)} className={inputCls}>
                {CATEGORIES.map((c) => <option key={c} value={c} style={{ background: "#0d1422" }}>{CAT_LABELS[c]}</option>)}
              </select>
            ) : (
              <NeonBadge variant={CAT_BADGE[a.category]}>{CAT_LABELS[a.category]}</NeonBadge>
            )}

            {editing ? (
              <CurrencySelect value={cur} onChange={(code) => patch(globalIdx, "currency", code)} className={inputCls} />
            ) : (
              <span className="text-2xs font-mono font-bold text-white/50 border border-glass-border/40 rounded px-1 py-0.5">{cur}</span>
            )}

            {editing ? (
              <input type="number" min={0} step={0.001} value={a.rate}
                onChange={(e) => patch(globalIdx, "rate", parseFloat(e.target.value || "0"))}
                className={inputCls} />
            ) : (
              <span className="text-xs font-bold font-mono text-amber-signal">{formatRate(a)}</span>
            )}

            {editing ? (
              <select value={a.rate_type} onChange={(e) => patch(globalIdx, "rate_type", e.target.value)} className={inputCls}>
                {RATE_TYPES.map((t) => <option key={t} value={t} style={{ background: "#0d1422" }}>{t}</option>)}
              </select>
            ) : (
              <span className="text-2xs font-mono text-white/40">{a.rate_type}</span>
            )}

            {editing ? (
              <input type="number" min={0} step={1} value={a.min_charge}
                onChange={(e) => patch(globalIdx, "min_charge", parseFloat(e.target.value || "0"))}
                className={inputCls} />
            ) : (
              <span className="text-xs font-mono text-white/40">
                {a.min_charge > 0 ? `${fmtCurrency(a.min_charge, cur)} min` : "—"}
              </span>
            )}

            {editing ? (
              <input value={a.description} onChange={(e) => patch(globalIdx, "description", e.target.value)} placeholder="Description…" className={inputCls} />
            ) : (
              <span className="text-2xs font-mono text-white/40 truncate" title={a.description}>{a.description}</span>
            )}

            <button onClick={() => editing && remove(globalIdx)} disabled={!editing}
              className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors disabled:opacity-0">
              <Trash2 size={11} />
            </button>
          </div>
        );
      })}

      {editing && (
        <div className="px-5 py-3">
          <button onClick={() => onChange([...addons, emptyAddon()])}
            className="flex items-center gap-1.5 rounded-lg border border-amber-signal/30 bg-amber-signal/5 px-3 py-1.5 text-xs font-medium text-amber-signal hover:border-amber-signal/60 transition-colors">
            <Plus size={12} /> Add service
          </button>
        </div>
      )}
    </GlassCard>
  );
}
