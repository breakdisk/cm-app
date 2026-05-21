"use client";
import { useState } from "react";
import { Plus, Trash2, ChevronDown, ChevronUp } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import type { BoxSize } from "@/lib/api/balikbayan-rates";

interface Props {
  sizes: BoxSize[];
  onSizesChange: (newSizes: BoxSize[], deletedIds: string[]) => void;
}

function emptySize(sortOrder: number): BoxSize {
  return {
    id: `size-${Date.now()}`,
    name: "",
    dimensions: "",
    max_weight_kg: 0,
    sort_order: sortOrder,
  };
}

const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-purple-plasma/40 w-full";

export function BoxSizesManager({ sizes, onSizesChange }: Props) {
  const [open, setOpen] = useState(true);

  function patch(idx: number, key: keyof BoxSize, value: string | number) {
    const next = sizes.slice();
    next[idx] = { ...next[idx], [key]: value } as BoxSize;
    onSizesChange(next, []);
  }

  function remove(idx: number) {
    const deletedId = sizes[idx].id;
    onSizesChange(sizes.filter((_, i) => i !== idx), [deletedId]);
  }

  function add() {
    onSizesChange([...sizes, emptySize(sizes.length + 1)], []);
  }

  function moveUp(idx: number) {
    if (idx === 0) return;
    const next = sizes.slice();
    [next[idx - 1], next[idx]] = [next[idx], next[idx - 1]];
    next.forEach((s, i) => { next[i] = { ...s, sort_order: i + 1 }; });
    onSizesChange(next, []);
  }

  function moveDown(idx: number) {
    if (idx === sizes.length - 1) return;
    const next = sizes.slice();
    [next[idx], next[idx + 1]] = [next[idx + 1], next[idx]];
    next.forEach((s, i) => { next[i] = { ...s, sort_order: i + 1 }; });
    onSizesChange(next, []);
  }

  return (
    <GlassCard padding="none">
      {/* Header — collapsible toggle */}
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center justify-between px-5 py-3 text-left"
      >
        <div className="flex items-center gap-2.5">
          <span className="text-xs font-semibold font-heading text-purple-plasma">Box Sizes Configuration</span>
          <span className="text-2xs font-mono text-white/30">{sizes.length} size{sizes.length !== 1 ? "s" : ""}</span>
        </div>
        {open
          ? <ChevronUp size={14} className="text-white/40 flex-shrink-0" />
          : <ChevronDown size={14} className="text-white/40 flex-shrink-0" />}
      </button>

      {open && (
        <>
          {/* Column headers */}
          <div className="grid grid-cols-[28px_1fr_140px_80px_36px_24px] gap-2 px-5 py-2 border-t border-glass-border bg-glass-100/20">
            {["", "Size Name", "Dimensions (L×W×H)", "Max kg", "", "#"].map((h, i) => (
              <span key={i} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {sizes.map((s, idx) => (
            <div
              key={s.id}
              className="grid grid-cols-[28px_1fr_140px_80px_36px_24px] gap-2 items-center px-5 py-2 border-t border-glass-border/40 hover:bg-glass-100/20 transition-colors"
            >
              {/* Reorder arrows */}
              <div className="flex flex-col">
                <button
                  onClick={() => moveUp(idx)}
                  disabled={idx === 0}
                  className="text-white/30 hover:text-white/70 disabled:opacity-20 transition-colors leading-none"
                >
                  <ChevronUp size={11} />
                </button>
                <button
                  onClick={() => moveDown(idx)}
                  disabled={idx === sizes.length - 1}
                  className="text-white/30 hover:text-white/70 disabled:opacity-20 transition-colors leading-none"
                >
                  <ChevronDown size={11} />
                </button>
              </div>

              <input
                value={s.name}
                onChange={(e) => patch(idx, "name", e.target.value)}
                placeholder="e.g. Jumbo"
                className={inputCls}
              />
              <input
                value={s.dimensions}
                onChange={(e) => patch(idx, "dimensions", e.target.value)}
                placeholder='24"×24"×24"'
                className={inputCls}
              />
              <input
                type="number"
                min={0}
                step={0.5}
                value={s.max_weight_kg}
                onChange={(e) => patch(idx, "max_weight_kg", parseFloat(e.target.value || "0"))}
                className={inputCls + " text-center"}
              />
              <button
                onClick={() => remove(idx)}
                disabled={sizes.length <= 1}
                className="flex h-7 w-7 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors disabled:opacity-20"
              >
                <Trash2 size={11} />
              </button>
              <span className="text-2xs font-mono text-white/20 text-center">{idx + 1}</span>
            </div>
          ))}

          <div className="px-5 py-3 border-t border-glass-border/40 flex items-center justify-between gap-3">
            <button
              onClick={add}
              className="flex items-center gap-1.5 rounded-lg border border-purple-plasma/30 bg-purple-plasma/5 px-3 py-1.5 text-xs font-medium text-purple-plasma hover:border-purple-plasma/60 transition-colors"
            >
              <Plus size={12} /> Add size
            </button>
            <p className="text-2xs font-mono text-white/20">
              Adding or removing sizes updates the Sea Cargo and PH Delivery pricing columns.
            </p>
          </div>
        </>
      )}
    </GlassCard>
  );
}
