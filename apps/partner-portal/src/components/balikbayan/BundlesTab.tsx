"use client";
import { Plus, Trash2, Download, Package } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { CurrencySelect } from "@/components/balikbayan/CurrencySelect";
import { fmtCurrency } from "@/lib/data/currencies";
import type { BoxBundle, BoxBundleItem, BoxSize, BundleScope } from "@/lib/api/balikbayan-rates";

interface Props {
  bundles: BoxBundle[];
  boxSizes: BoxSize[];
  editing: boolean;
  onChange: (bundles: BoxBundle[]) => void;
}

const SCOPE_BADGE: Record<BundleScope, "cyan" | "purple" | "green"> = {
  sea:  "cyan",
  air:  "purple",
  both: "green",
};

const SCOPE_LABELS: Record<BundleScope, string> = {
  sea:  "Sea Only",
  air:  "Air Only",
  both: "Sea & Air",
};

function emptyBundle(): BoxBundle {
  return {
    id: `bundle-${Date.now()}`,
    name: "",
    description: "",
    items: [],
    currency: "USD",
    price_usd: 0,
    valid_for: "sea",
    notes: "",
  };
}

function sizeNameOf(boxSizes: BoxSize[], sizeId: string): string {
  return boxSizes.find((s) => s.id === sizeId)?.name ?? sizeId;
}

function exportCsv(bundles: BoxBundle[]) {
  const header = "id,name,description,items,currency,price,valid_for,notes";
  const rows = bundles.map((b) =>
    [
      b.id, `"${b.name}"`, `"${b.description}"`,
      `"${b.items.map((i) => `${i.size_id}×${i.quantity}`).join("; ")}"`,
      b.currency ?? "USD", b.price_usd, b.valid_for, `"${b.notes}"`,
    ].join(",")
  );
  const blob = new Blob([[header, ...rows].join("\n")], { type: "text/csv;charset=utf-8" });
  const url  = URL.createObjectURL(blob);
  const a    = Object.assign(document.createElement("a"), { href: url, download: "balikbayan-bundles.csv" });
  document.body.appendChild(a); a.click(); document.body.removeChild(a); URL.revokeObjectURL(url);
}

const inputCls = "rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-cyan-neon/40 w-full";

export function BundlesTab({ bundles, boxSizes, editing, onChange }: Props) {
  function patchBundle(idx: number, key: keyof BoxBundle, value: unknown) {
    const next = bundles.slice();
    next[idx] = { ...next[idx], [key]: value } as BoxBundle;
    onChange(next);
  }
  function removeBundle(idx: number) { onChange(bundles.filter((_, i) => i !== idx)); }

  function addItem(bIdx: number) {
    const first = boxSizes[0];
    if (!first) return;
    const items: BoxBundleItem[] = [...bundles[bIdx].items, { size_id: first.id, quantity: 1 }];
    patchBundle(bIdx, "items", items);
  }
  function patchItem(bIdx: number, iIdx: number, key: keyof BoxBundleItem, value: string | number) {
    const items = bundles[bIdx].items.slice();
    items[iIdx] = { ...items[iIdx], [key]: value };
    patchBundle(bIdx, "items", items);
  }
  function removeItem(bIdx: number, iIdx: number) {
    patchBundle(bIdx, "items", bundles[bIdx].items.filter((_, i) => i !== iIdx));
  }

  return (
    <GlassCard padding="none">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <h3 className="font-heading text-sm font-semibold text-white">Bundle Packages</h3>
          <p className="text-2xs font-mono text-white/30 mt-0.5">
            Pre-packaged multi-box deals · fixed price in carrier&apos;s chosen currency · sea, air, or both
          </p>
        </div>
        {!editing && bundles.length > 0 && (
          <button onClick={() => exportCsv(bundles)}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={12} /> CSV
          </button>
        )}
      </div>

      {bundles.length === 0 && !editing && (
        <p className="px-5 py-10 text-center text-xs text-white/30 font-mono">
          No bundles configured. Enter edit mode to add bundle packages.
        </p>
      )}

      <div className="divide-y divide-glass-border/40">
        {bundles.map((b, bIdx) => {
          const cur = b.currency ?? "USD";
          return (
            <div key={b.id} className="px-5 py-4 hover:bg-glass-100/20 transition-colors">
              {editing ? (
                /* ── Edit mode ─────────────────────────────── */
                <div className="flex flex-col gap-3">
                  {/* Row 1: name / description / currency / price / delete */}
                  <div className="grid grid-cols-[1fr_1fr_70px_110px_36px] gap-2 items-start">
                    <input value={b.name} onChange={(e) => patchBundle(bIdx, "name", e.target.value)}
                      placeholder="Bundle name" className={inputCls} />
                    <input value={b.description} onChange={(e) => patchBundle(bIdx, "description", e.target.value)}
                      placeholder="Short description" className={inputCls} />
                    <CurrencySelect value={cur} onChange={(code) => patchBundle(bIdx, "currency", code)} className={inputCls} />
                    <input type="number" min={0} step={1} value={b.price_usd}
                      onChange={(e) => patchBundle(bIdx, "price_usd", parseFloat(e.target.value || "0"))}
                      placeholder="Price" className={inputCls} />
                    <button onClick={() => removeBundle(bIdx)}
                      className="flex h-8 w-8 items-center justify-center rounded-md border border-red-signal/30 text-red-signal hover:bg-red-signal/10 transition-colors">
                      <Trash2 size={12} />
                    </button>
                  </div>

                  {/* Row 2: scope / notes */}
                  <div className="grid grid-cols-[120px_1fr] gap-2">
                    <select value={b.valid_for} onChange={(e) => patchBundle(bIdx, "valid_for", e.target.value as BundleScope)}
                      className={inputCls}>
                      {(["sea", "air", "both"] as BundleScope[]).map((v) => (
                        <option key={v} value={v} style={{ background: "#0d1422" }}>{SCOPE_LABELS[v]}</option>
                      ))}
                    </select>
                    <input value={b.notes} onChange={(e) => patchBundle(bIdx, "notes", e.target.value)}
                      placeholder="Additional notes (promo, expiry, eligibility…)" className={inputCls} />
                  </div>

                  {/* Bundle items */}
                  <div className="rounded-lg border border-glass-border/60 bg-glass-100/20 p-3">
                    <p className="text-2xs font-mono text-white/40 mb-2 uppercase tracking-wider">Items included</p>
                    <div className="flex flex-col gap-1.5">
                      {b.items.map((item, iIdx) => (
                        <div key={iIdx} className="flex items-center gap-2">
                          <select value={item.size_id} onChange={(e) => patchItem(bIdx, iIdx, "size_id", e.target.value)}
                            className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none focus:border-cyan-neon/40 flex-1">
                            {boxSizes.map((s) => (
                              <option key={s.id} value={s.id} style={{ background: "#0d1422" }}>{s.name}</option>
                            ))}
                          </select>
                          <span className="text-2xs font-mono text-white/40">×</span>
                          <input type="number" min={1} step={1} value={item.quantity}
                            onChange={(e) => patchItem(bIdx, iIdx, "quantity", parseInt(e.target.value || "1"))}
                            className="rounded-md border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white font-mono outline-none focus:border-cyan-neon/40 w-16 text-center" />
                          <button onClick={() => removeItem(bIdx, iIdx)}
                            className="flex h-6 w-6 items-center justify-center rounded text-red-signal/50 hover:text-red-signal transition-colors">
                            <Trash2 size={10} />
                          </button>
                        </div>
                      ))}
                      {b.items.length === 0 && (
                        <p className="text-2xs font-mono text-white/20">No items — add at least one box size.</p>
                      )}
                    </div>
                    <button onClick={() => addItem(bIdx)} disabled={boxSizes.length === 0}
                      className="mt-2 flex items-center gap-1 text-2xs font-mono text-cyan-neon/60 hover:text-cyan-neon transition-colors disabled:opacity-30">
                      <Plus size={10} /> add item
                    </button>
                  </div>
                </div>
              ) : (
                /* ── Read mode ──────────────────────────────── */
                <div className="flex items-start justify-between gap-4">
                  <div className="flex items-start gap-3 min-w-0">
                    <div className="mt-0.5 rounded-lg border border-glass-border bg-glass-100/40 p-2 flex-shrink-0">
                      <Package size={16} className="text-cyan-neon" />
                    </div>
                    <div className="min-w-0">
                      <div className="flex items-center gap-2 flex-wrap mb-1">
                        <span className="text-sm font-semibold text-white font-heading">{b.name}</span>
                        <NeonBadge variant={SCOPE_BADGE[b.valid_for]}>{SCOPE_LABELS[b.valid_for]}</NeonBadge>
                      </div>
                      {b.description && <p className="text-xs text-white/50 mb-2">{b.description}</p>}
                      <div className="flex flex-wrap gap-2">
                        {b.items.map((item, i) => (
                          <span key={i} className="rounded-full border border-glass-border px-2.5 py-0.5 text-2xs font-mono text-white/50">
                            {item.quantity}× {sizeNameOf(boxSizes, item.size_id)}
                          </span>
                        ))}
                        {b.items.length === 0 && <span className="text-2xs font-mono text-white/20">No items</span>}
                      </div>
                      {b.notes && <p className="text-2xs font-mono text-white/30 mt-1.5">{b.notes}</p>}
                    </div>
                  </div>
                  <div className="flex-shrink-0 text-right">
                    <p className="text-xl font-bold font-mono text-cyan-neon">{fmtCurrency(b.price_usd, cur)}</p>
                    <p className="text-2xs font-mono text-white/30">per bundle</p>
                  </div>
                </div>
              )}
            </div>
          );
        })}
      </div>

      {editing && (
        <div className="px-5 py-3 border-t border-glass-border/40">
          <button onClick={() => onChange([...bundles, emptyBundle()])}
            className="flex items-center gap-1.5 rounded-lg border border-cyan-neon/30 bg-cyan-neon/5 px-3 py-1.5 text-xs font-medium text-cyan-neon hover:border-cyan-neon/60 transition-colors">
            <Plus size={12} /> Add bundle
          </button>
        </div>
      )}
    </GlassCard>
  );
}
