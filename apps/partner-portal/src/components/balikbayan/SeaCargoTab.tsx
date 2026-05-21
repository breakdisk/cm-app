"use client";
import { useState } from "react";
import { Plus, Trash2, Download } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import type { SeaCargoRate } from "@/lib/api/balikbayan-rates";

interface Props {
  rows: SeaCargoRate[];
  editing: boolean;
  onChange: (rows: SeaCargoRate[]) => void;
}

const SIZE_COLS = ["jumbo_usd", "xl_usd", "large_usd", "small_usd"] as const;
const SIZE_LABELS = ['Jumbo 24"×24"×24"', 'XL 24"×18"×18"', 'Large 20"×16"×16"', 'Small 18"×14"×14"'];

function emptyRow(): SeaCargoRate {
  return { origin: "", transit_days: "", jumbo_usd: 0, xl_usd: 0, large_usd: 0, small_usd: 0 };
}

function exportCsv(rows: SeaCargoRate[]) {
  const header = "origin,transit_days,jumbo_usd,xl_usd,large_usd,small_usd";
  const body = rows.map((r) =>
    [`"${r.origin}"`, r.transit_days, r.jumbo_usd, r.xl_usd, r.large_usd, r.small_usd].join(",")
  );
  const blob = new Blob([[header, ...body].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-sea-cargo.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

export function SeaCargoTab({ rows, editing, onChange }: Props) {
  const [search, setSearch] = useState("");

  function patch(idx: number, key: keyof SeaCargoRate, value: string | number) {
    const next = rows.slice();
    next[idx] = { ...next[idx], [key]: value };
    onChange(next);
  }

  function remove(idx: number) { onChange(rows.filter((_, i) => i !== idx)); }
  function add()               { onChange([...rows, emptyRow()]); }

  const visible = search.trim()
    ? rows.filter((r) => r.origin.toLowerCase().includes(search.toLowerCase()))
    : rows;

  return (
    <GlassCard padding="none">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Sea Cargo Rates</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            FROM origin country → TO Philippines (Port of Manila / Cebu) · per box · USD
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
              onClick={() => exportCsv(rows)}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors"
            >
              <Download size={12} /> CSV
            </button>
          )}
        </div>
      </div>

      {/* Column headers */}
      <div className="grid grid-cols-[2fr_110px_repeat(4,90px)_36px] gap-2 px-5 py-2.5 border-b border-glass-border">
        <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Origin Country / City</span>
        <span className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center">Transit</span>
        {SIZE_LABELS.map((l) => (
          <span key={l} className="text-2xs font-mono text-white/30 uppercase tracking-wider text-center">{l.split(" ")[0]}</span>
        ))}
        <span />
      </div>

      {/* Size sub-header */}
      <div className="grid grid-cols-[2fr_110px_repeat(4,90px)_36px] gap-2 px-5 py-1 border-b border-glass-border/40 bg-glass-100/30">
        <span />
        <span />
        {SIZE_LABELS.map((l) => (
          <span key={l} className="text-2xs font-mono text-white/20 text-center">{l.split(" ").slice(1).join(" ")}</span>
        ))}
        <span />
      </div>

      {visible.length === 0 && (
        <p className="px-5 py-10 text-center text-xs text-white/30 font-mono">No origins match filter.</p>
      )}

      {(search.trim() ? visible : rows).map((r, idx) => (
        <div
          key={idx}
          className="grid grid-cols-[2fr_110px_repeat(4,90px)_36px] gap-2 items-center px-5 py-2.5 border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors"
        >
          {editing ? (
            <input
              value={r.origin}
              onChange={(e) => patch(idx, "origin", e.target.value)}
              placeholder="Country / city"
              className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-cyan-neon/40 w-full"
            />
          ) : (
            <span className="text-xs text-white font-medium truncate" title={r.origin}>{r.origin}</span>
          )}

          {editing ? (
            <input
              value={r.transit_days}
              onChange={(e) => patch(idx, "transit_days", e.target.value)}
              placeholder="30–45"
              className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 text-center"
            />
          ) : (
            <span className="text-xs font-mono text-white/50 text-center">{r.transit_days}</span>
          )}

          {SIZE_COLS.map((col) =>
            editing ? (
              <input
                key={col}
                type="number"
                min={0}
                step={1}
                value={r[col]}
                onChange={(e) => patch(idx, col, parseFloat(e.target.value || "0"))}
                className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 text-center"
              />
            ) : (
              <span key={col} className="text-xs font-bold font-mono text-cyan-neon text-center">
                ${r[col]}
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
          ★ Rates are door-to-port. Philippine local delivery charged separately (PH Delivery Zones tab).
          Peak season surcharge (Oct–Jan) of 15–20% applies. Max box weight: Jumbo 30 kg · XL 25 kg · Large 20 kg · Small 15 kg.
        </p>
      </div>
    </GlassCard>
  );
}
