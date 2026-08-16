"use client";
/**
 * Items nobody has vouched for yet.
 *
 * The storefront's whole mechanic is "confirm what you have; anything
 * unconfirmed gets substituted". The count of never-confirmed items existed
 * before this — but only as a clause inside the *stale* banner, so it appeared
 * only when something was already stale, listed nothing, and offered no way to
 * act on a single row. A merchant who imported forty items from Shopify was
 * told a number and left to find them.
 *
 * This is that queue. It stands on its own because arriving unconfirmed is not
 * the same problem as going stale: one is a new item awaiting its first
 * statement, the other is a statement that has aged out.
 *
 * Both buttons *are* the confirmation. Declaring "out of stock" is as much a
 * fact as declaring availability, and offering only "Confirm" would push a
 * merchant clearing a backlog into asserting they have things they do not.
 */
import { useState } from "react";
import { Check, PackageX, ShieldQuestion } from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import type { Availability, CatalogSource, Item } from "@/lib/api/storefront";

const SOURCE_LABEL: Record<CatalogSource, string> = {
  manual: "typed here",
  shopify: "Shopify",
  woocommerce: "WooCommerce",
  csv: "CSV import",
  pos: "POS",
};

const peso = (cents: number) =>
  `₱${(cents / 100).toLocaleString("en-PH", { minimumFractionDigits: 2 })}`;

export function UnconfirmedQueue({
  items,
  busyKey,
  onDeclare,
  onConfirmAll,
}: {
  /** Items with `confirmed_at === null` — never attested to by anyone. */
  items: Item[];
  busyKey: string | null;
  onDeclare: (itemId: string, state: Availability) => void;
  onConfirmAll: () => void;
}) {
  const [collapsed, setCollapsed] = useState(false);

  if (items.length === 0) return null;

  // Imported rows are the reason this queue gets long, so say where they came
  // from rather than making the merchant infer it row by row.
  const bySource = items.reduce<Record<string, number>>((acc, i) => {
    acc[i.source] = (acc[i.source] ?? 0) + 1;
    return acc;
  }, {});
  const imported = Object.entries(bySource)
    .filter(([s]) => s !== "manual")
    .map(([s, n]) => `${n} from ${SOURCE_LABEL[s as CatalogSource] ?? s}`)
    .join(", ");

  return (
    <GlassCard padding="none" className="border-l-2 border-l-[#00E5FF]">
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-white/5 p-4">
        <div className="flex items-start gap-3">
          <ShieldQuestion className="mt-0.5 h-5 w-5 shrink-0 text-[#00E5FF]" />
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-[#00E5FF]">
              {items.length} item{items.length === 1 ? "" : "s"} awaiting your say-so
            </h2>
            <p className="mt-1 text-xs text-white/60">
              Nobody has ever confirmed {items.length === 1 ? "this one" : "these"}
              {imported ? ` — ${imported}` : ""}. Until you do, the assistant treats
              {items.length === 1 ? " it" : " them"} as uncertain and offers substitutes
              instead.
            </p>
          </div>
        </div>
        <div className="flex shrink-0 gap-2">
          <button
            type="button"
            onClick={onConfirmAll}
            disabled={busyKey === "confirm-all"}
            className="rounded-lg bg-[#00E5FF]/90 px-3 py-1.5 text-xs font-medium text-[#04121a] hover:bg-[#00E5FF] disabled:opacity-40"
          >
            {busyKey === "confirm-all" ? "Confirming…" : "All available"}
          </button>
          <button
            type="button"
            onClick={() => setCollapsed((c) => !c)}
            className="rounded-lg border border-white/10 px-3 py-1.5 text-xs text-white/60 hover:bg-white/5"
          >
            {collapsed ? "Show" : "Hide"}
          </button>
        </div>
      </div>

      {!collapsed && (
        <div className="max-h-80 divide-y divide-white/5 overflow-y-auto">
          {items.map((item) => (
            <div
              key={item.id}
              className="flex flex-col gap-2 p-3 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0">
                <p className="truncate text-sm text-white">{item.name}</p>
                <p className="mt-0.5 text-xs text-white/40">
                  <span className="font-mono">{item.sku}</span> · {peso(item.price_cents)}
                  {item.source !== "manual" && ` · ${SOURCE_LABEL[item.source]}`}
                </p>
              </div>
              <div className="flex shrink-0 gap-2">
                <button
                  type="button"
                  onClick={() => onDeclare(item.id, "available")}
                  disabled={busyKey === item.id}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-[#00FF88]/30 px-2.5 py-1.5 text-xs text-[#00FF88] hover:bg-[#00FF88]/10 disabled:opacity-40"
                >
                  <Check className="h-3.5 w-3.5" />
                  Available
                </button>
                <button
                  type="button"
                  onClick={() => onDeclare(item.id, "out_of_stock")}
                  disabled={busyKey === item.id}
                  className="inline-flex items-center gap-1.5 rounded-lg border border-white/10 px-2.5 py-1.5 text-xs text-white/50 hover:bg-white/5 disabled:opacity-40"
                >
                  <PackageX className="h-3.5 w-3.5" />
                  Out of stock
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </GlassCard>
  );
}
