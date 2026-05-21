"use client";
import { useState } from "react";
import { Plus, Trash2, Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { CurrencySelect } from "@/components/balikbayan/CurrencySelect";
import { fmtCurrency } from "@/lib/data/currencies";
import type { SeaCargoRate, BoxSize } from "@/lib/api/balikbayan-rates";

interface Props {
  rows: SeaCargoRate[];
  boxSizes: BoxSize[];
  editing: boolean;
  onChange: (rows: SeaCargoRate[]) => void;
}

function emptyRow(boxSizes: BoxSize[]): SeaCargoRate {
  const prices: Record<string, number> = {};
  boxSizes.forEach((s) => { prices[s.id] = 0; });
  return { origin: "", transit_days: "", currency: "USD", prices };
}

function exportCsv(rows: SeaCargoRate[], boxSizes: BoxSize[]) {
  const header = ["origin", "currency", "transit_days", ...boxSizes.map((s) => s.id)].join(",");
  const body   = rows.map((r) =>
    [`"${r.origin}"`, r.currency, r.transit_days, ...boxSizes.map((s) => r.prices[s.id] ?? 0)].join(",")
  );
  const blob = new Blob([[header, ...body].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-sea-cargo.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

const selectCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-cyan-neon/40 w-full";

export function SeaCargoTab({ rows, boxSizes, editing, onChange }: Props) {
  const [search, setSearch] = useState("");

  // grid: Origin | Currency | Transit | [size cols] | Delete
  const colTemplate = `2fr 70px 100px ${boxSizes.map(() => "80px").join(" ")} 36px`;

  function patchMeta(idx: number, key: "origin" | "transit_days", value: string) {
    const next = rows.slice();
    next[idx] = { ...next[idx], [key]: value };
    onChange(next);
  }
  function patchCurrency(idx: number, code: string) {
    const next = rows.slice();
    next[idx] = { ...next[idx], currency: code };
    onChange(next);
  }
  function patchPrice(idx: number, sizeId: string, value: number) {
    const next = rows.slice();
    next[idx] = { ...next[idx], prices: { ...next[idx].prices, [sizeId]: value } };
    onChange(next);
  }
  function remove(idx: number) { onChange(rows.filter((_, i) => i !== idx)); }
  function add()               { onChange([...rows, emptyRow(boxSizes)]); }

  const visible = search.trim()
    ? rows.filter((r) => r.origin.toLowerCase().includes(search.toLowerCase()))
    : rows;

  const cellCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 w-full text-center";

  return (
    <GlassCard padding="none">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Sea Cargo Rates</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            FROM origin country → TO Philippines · per box · native origin currency
          </p>
        </div>
        <div className="flex items-center gap-2">
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter origin…"
            className="rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white placeholder-white/30 outline-none focus:border-cyan-neon/40 w-40"
          />
          {!editing && (
            <button
              onClick={() => exportCsv(rows, boxSizes)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors"
            >
              <Download size={12} /> CSV
            </button>
          )}
        </div>
      </div>

      <div className="overflow-x-auto">
        {/* Column headers */}
        <div className="grid gap-2 px-5 py-2.5 border-b border-glass-border min-w-max" style={{ gridTemplateColumns: colTemplate }}>
          <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Origin Country / City</span>
          <span className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center">Currency</span>
          <span className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center">Transit</span>
          {boxSizes.map((s) => (
            <span key={s.id} className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center">{s.name}</span>
          ))}
          <span />
        </div>

        {/* Dimensions sub-header */}
        <div className="grid gap-2 px-5 py-1 border-b border-glass-border/40 bg-glass-100/30 min-w-max" style={{ gridTemplateColumns: colTemplate }}>
          <span /><span /><span />
          {boxSizes.map((s) => (
            <span key={s.id} className="text-2xs font-mono text-white/20 text-center truncate">{s.dimensions}</span>
          ))}
          <span />
        </div>

        {visible.length === 0 && (
          <p className="px-5 py-10 text-center text-xs text-white/30 font-mono">No origins match filter.</p>
        )}

        {(search.trim() ? visible : rows).map((r, idx) => (
          <div
            key={idx}
            className="grid gap-2 items-center px-5 py-2.5 border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors min-w-max"
            style={{ gridTemplateColumns: colTemplate }}
          >
            {editing ? (
              <input
                value={r.origin}
                onChange={(e) => patchMeta(idx, "origin", e.target.value)}
                placeholder="Country / city"
                className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-cyan-neon/40 w-full"
              />
            ) : (
              <span className="text-xs text-white font-medium truncate" title={r.origin}>{r.origin}</span>
            )}

            {editing ? (
              <CurrencySelect
                value={r.currency ?? "USD"}
                onChange={(code) => patchCurrency(idx, code)}
                className={selectCls}
              />
            ) : (
              <span className="text-2xs font-mono font-bold text-center text-white/50 border border-glass-border/40 rounded px-1 py-0.5 mx-auto">
                {r.currency ?? "USD"}
              </span>
            )}

            {editing ? (
              <input
                value={r.transit_days}
                onChange={(e) => patchMeta(idx, "transit_days", e.target.value)}
                placeholder="30–45"
                className={cellCls}
              />
            ) : (
              <span className="text-xs font-mono text-white/50 text-center">{r.transit_days}</span>
            )}

            {boxSizes.map((s) =>
              editing ? (
                <input
                  key={s.id}
                  type="number"
                  min={0}
                  step={1}
                  value={r.prices[s.id] ?? 0}
                  onChange={(e) => patchPrice(idx, s.id, parseFloat(e.target.value || "0"))}
                  className={cellCls}
                />
              ) : (
                <span key={s.id} className="text-xs font-bold font-mono text-cyan-neon text-center">
                  {fmtCurrency(r.prices[s.id] ?? 0, r.currency ?? "USD")}
                </span>
              )
            )}

            <button
              onClick={() => editing && remove(idx)}
              disabled={!editing}
              className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors disabled:opacity-0"
            >
              <Trash2 size={11} />
            </button>
          </div>
        ))}
      </div>

      {editing && !search.trim() && (
        <div className="px-5 py-3">
          <button
            onClick={add}
            className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-3 py-1.5 text-xs font-medium text-cyan-neon hover:border-cyan-neon/60 transition-colors"
          >
            <Plus size={12} /> Add origin
          </button>
        </div>
      )}

      <div className="px-5 py-2 border-t border-glass-border/40">
        <p className="text-2xs font-mono text-white/20">
          ★ Each origin row is priced in its native local currency. Rates are door-to-port.
          PH local delivery charged separately (PH Delivery Zones tab). Peak season surcharge (Oct–Jan) of 15–20% applies.
        </p>
      </div>
    </GlassCard>
  );
}
