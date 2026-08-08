"use client";
/**
 * Merchant Portal — Storefront (OmniDeliv vendor console)
 *
 * Why this page exists, beyond "vendors need a screen":
 *
 * OmniDeliv's agent decides whether to line up a substitute from the freshness
 * of an availability declaration, not just its value. An item marked available
 * but not confirmed inside the freshness window (STOCK_FRESHNESS_MINS, 30 by
 * default) reads as *uncertain*, and the Nutritionist proposes a substitute for
 * it. With no console, nobody ever confirms anything — so half an hour after a
 * deploy every item in every store is being quietly swapped out, and the mesh
 * looks like it is behaving badly when it is doing exactly what it was told.
 *
 * So the primary control here is not "edit my menu". It is "confirm this is
 * true", and the page leads with what has gone stale.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { AlertTriangle, Check, PackageX, RefreshCw, Store } from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";

const OMNIDELIV_URL =
  process.env.NEXT_PUBLIC_OMNIDELIV_URL ?? "http://localhost:8091";

type Availability = "available" | "limited" | "out_of_stock";

interface Item {
  id: string;
  name: string;
  sku: string;
  price_cents: number;
  allergens: string[];
  is_listed: boolean;
  availability: Availability;
  confirmed_at: string;
  warrants_substitute: boolean;
}

interface Catalog {
  vendor_id: string;
  vendor_name: string;
  items: Item[];
}

interface Earnings {
  period: string;
  balance_cents: number;
}

const peso = (cents: number) =>
  `₱${(cents / 100).toLocaleString("en-PH", { minimumFractionDigits: 2 })}`;

/** "12m ago" — the number that decides whether the agent trusts this row. */
function since(iso: string): string {
  const mins = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export default function Storefront() {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [earnings, setEarnings] = useState<Earnings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [noStore, setNoStore] = useState(false);

  const load = useCallback(async () => {
    try {
      const res = await authFetch(`${OMNIDELIV_URL}/v1/omnideliv/catalog/mine`);
      if (res.status === 404) {
        // Distinct from "no items": this login runs no store at all.
        setNoStore(true);
        return;
      }
      if (!res.ok) throw new Error(`catalog: ${res.status}`);
      setCatalog(await res.json());
      setNoStore(false);

      const e = await authFetch(`${OMNIDELIV_URL}/v1/omnideliv/vendors/me/earnings`);
      if (e.ok) setEarnings(await e.json());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "could not load the storefront");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const setAvailability = useCallback(
    async (itemId: string, state: Availability) => {
      setBusy(itemId);
      try {
        const res = await authFetch(
          `${OMNIDELIV_URL}/v1/omnideliv/catalog/items/${itemId}/availability`,
          {
            method: "PATCH",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ state }),
          },
        );
        if (!res.ok) throw new Error(`update failed: ${res.status}`);
        // Reload rather than patching local state: the freshness stamp is set
        // server-side, so guessing it here would show a confirmation that had
        // not actually landed.
        await load();
      } catch (err) {
        setError(err instanceof Error ? err.message : "could not update that item");
      } finally {
        setBusy(null);
      }
    },
    [load],
  );

  if (noStore) {
    return (
      <div className="p-6">
        <GlassCard className="p-8 text-center">
          <Store className="mx-auto mb-3 h-8 w-8 text-white/40" />
          <p className="text-white/70">
            This login is not linked to an OmniDeliv store.
          </p>
          <p className="mt-2 text-xs text-white/40">
            A store is linked by setting its <code>user_id</code>. Ask an
            operator to connect this account to your storefront.
          </p>
        </GlassCard>
      </div>
    );
  }

  const stale = catalog?.items.filter((i) => i.warrants_substitute) ?? [];

  return (
    <motion.div {...variants.fadeIn} className="space-y-5 p-6">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white">
            {catalog?.vendor_name ?? "Storefront"}
          </h1>
          <p className="text-sm text-white/50">
            Confirm what you have. Anything unconfirmed gets substituted.
          </p>
        </div>
        <button
          onClick={() => void load()}
          className="flex items-center gap-2 rounded-lg border border-white/10 px-3 py-2 text-sm text-white/70 hover:bg-white/5"
        >
          <RefreshCw className="h-4 w-4" /> Refresh
        </button>
      </div>

      {error && (
        <GlassCard className="border-l-2 border-l-[#FF3B5C] p-4">
          <p role="alert" className="text-sm text-[#FF3B5C]">{error}</p>
        </GlassCard>
      )}

      {/* Lead with the consequence, not the inventory. */}
      {stale.length > 0 && (
        <GlassCard className="border-l-2 border-l-[#FFAB00] p-4">
          <div className="flex items-start gap-3">
            <AlertTriangle className="mt-0.5 h-5 w-5 shrink-0 text-[#FFAB00]" />
            <div>
              <p className="text-sm font-semibold text-[#FFAB00]">
                {stale.length} item{stale.length === 1 ? "" : "s"} will be
                substituted
              </p>
              <p className="mt-1 text-xs text-white/60">
                Either they are out of stock or limited, or nobody has confirmed
                them recently enough for the assistant to rely on. Confirming an
                item as available resets that clock.
              </p>
            </div>
          </div>
        </GlassCard>
      )}

      {earnings && (
        <GlassCard className="p-4">
          <div className="flex items-baseline justify-between">
            <span className="text-xs uppercase tracking-wider text-white/40">
              Payouts · {earnings.period}
            </span>
            <span className="text-2xl font-bold text-[#00FF88]">
              {peso(earnings.balance_cents)}
            </span>
          </div>
        </GlassCard>
      )}

      <GlassCard className="overflow-hidden p-0">
        <div className="divide-y divide-white/5">
          {catalog?.items.length === 0 && (
            <p className="p-6 text-sm text-white/50">
              This store has no items yet.
            </p>
          )}

          {catalog?.items.map((item) => (
            <div
              key={item.id}
              className="flex flex-col gap-3 p-4 sm:flex-row sm:items-center sm:justify-between"
            >
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="truncate font-semibold text-white">
                    {item.name}
                  </span>
                  {item.warrants_substitute && (
                    <span className="rounded bg-[#FFAB00]/10 px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-[#FFAB00]">
                      substituting
                    </span>
                  )}
                </div>
                <p className="mt-0.5 text-xs text-white/40">
                  {peso(item.price_cents)} · confirmed {since(item.confirmed_at)}
                  {item.allergens.length > 0 && ` · ${item.allergens.join(", ")}`}
                </p>
              </div>

              <div className="flex shrink-0 gap-2">
                {(
                  [
                    ["available", Check, "#00FF88"],
                    ["limited", AlertTriangle, "#FFAB00"],
                    ["out_of_stock", PackageX, "#FF3B5C"],
                  ] as const
                ).map(([state, Icon, colour]) => {
                  const active = item.availability === state;
                  return (
                    <button
                      key={state}
                      disabled={busy === item.id}
                      onClick={() => void setAvailability(item.id, state)}
                      aria-label={`Mark ${item.name} ${state.replace("_", " ")}`}
                      className="flex items-center gap-1.5 rounded-lg border px-2.5 py-1.5 text-xs transition disabled:opacity-40"
                      style={{
                        borderColor: active ? colour : "rgba(255,255,255,0.10)",
                        backgroundColor: active ? `${colour}1A` : "transparent",
                        color: active ? colour : "rgba(255,255,255,0.55)",
                      }}
                    >
                      <Icon className="h-3.5 w-3.5" />
                      <span className="hidden sm:inline">
                        {state === "out_of_stock" ? "Out" : state === "limited" ? "Low" : "In stock"}
                      </span>
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </div>
      </GlassCard>

      <p className="text-xs text-white/30">
        Re-confirming an item as available resets its freshness clock, even if
        nothing changed — that is the point of the control.
      </p>
    </motion.div>
  );
}
