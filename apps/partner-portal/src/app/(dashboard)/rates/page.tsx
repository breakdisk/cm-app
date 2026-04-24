"use client";
/**
 * Partner Portal — Rate Cards + Rate Shop
 * Surfaces the carrier service for the acting partner:
 *   GET /v1/carriers/:id             → partner's own rate_cards (read-only)
 *   GET /v1/carriers/rate-shop?…     → calculator showing all carriers' quotes
 *
 * Per-partner rate editing is intentionally omitted — rate cards are
 * ops-managed in the current commercial model. When product enables
 * self-serve rate edits, add `PUT /v1/carriers/:id/rate-cards` on the
 * backend and wire an edit modal here.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { GitBranch, RefreshCw, Calculator, Download } from "lucide-react";
import {
  carriersApi, fmtPhp,
  type Carrier, type RateCard, type RateQuote,
} from "@/lib/api/carriers";
import { getCurrentPartnerId } from "@/lib/api/partner-identity";

const SERVICE_TYPES = ["standard", "next_day", "same_day"] as const;
type ServiceType = typeof SERVICE_TYPES[number];

export default function RateCardsPage() {
  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const id = getCurrentPartnerId();
      const c = await carriersApi.get(id);
      setCarrier(c);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load rate card");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const rateCards: RateCard[] = carrier?.rate_cards ?? [];

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <GitBranch size={20} className="text-cyan-neon" />
            Rate Cards
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {carrier ? `${carrier.name} (${carrier.code})` : "Partner rate card"}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {carrier && <NeonBadge variant={carrier.status === "active" ? "green" : "amber"} dot>{carrier.status}</NeonBadge>}
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* SLA summary */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {loading && !carrier ? (
          Array.from({ length: 4 }).map((_, i) => (
            <GlassCard key={i} size="sm">
              <div className="h-10 animate-pulse rounded bg-glass-200" />
            </GlassCard>
          ))
        ) : carrier ? [
          { label: "Service Types", value: String(rateCards.length),                         color: "text-cyan-neon"    },
          { label: "On-Time Target", value: `${carrier.sla.on_time_target_pct.toFixed(0)}%`, color: "text-green-signal" },
          { label: "Max Days",       value: `${carrier.sla.max_delivery_days}d`,             color: "text-white"        },
          { label: "Breach Penalty", value: carrier.sla.penalty_per_breach > 0 ? fmtPhp(carrier.sla.penalty_per_breach) : "—", color: "text-amber-signal" },
        ].map((m) => (
          <GlassCard key={m.label} size="sm">
            <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{m.label}</p>
            <p className={`text-sm font-bold font-mono mt-1 ${m.color}`}>{m.value}</p>
          </GlassCard>
        )) : null}
      </motion.div>

      {/* Rate card table — read-only; backend has no edit endpoint yet */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Rate Cards</h2>
              <p className="text-2xs font-mono text-white/30 mt-0.5">Ops-managed pricing. Contact your account manager to update.</p>
            </div>
            <button
              disabled
              title="PDF export coming soon"
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/30 cursor-not-allowed"
            >
              <Download size={12} /> Export
            </button>
          </div>

          <div className="grid grid-cols-[2fr_100px_120px_80px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Service", "Base", "Per kg", "Max kg", "Coverage Zones"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : rateCards.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                No rate cards configured. Contact ops to set up pricing before accepting dispatches.
              </p>
            </div>
          ) : (
            rateCards.map((r) => (
              <div key={r.service_type} className="grid grid-cols-[2fr_100px_120px_80px_1fr] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <span className="text-sm font-medium text-white capitalize">{r.service_type.replace(/_/g, " ")}</span>
                <span className="text-sm font-bold font-mono text-green-signal">{fmtPhp(r.base_rate_cents)}</span>
                <span className="text-xs font-mono text-white/60">{fmtPhp(r.per_kg_cents)} / kg</span>
                <span className="text-xs font-mono text-white/60">{r.max_weight_kg} kg</span>
                <span className="text-xs text-white/40 font-mono truncate" title={r.coverage_zones.join(", ")}>
                  {r.coverage_zones.length === 0 ? "—" : r.coverage_zones.join(", ")}
                </span>
              </div>
            ))
          )}
        </GlassCard>
      </motion.div>

      {/* Rate-shop calculator */}
      <motion.div variants={variants.fadeInUp}>
        <RateShopCalculator />
      </motion.div>
    </motion.div>
  );
}

// ── Rate shop calculator ───────────────────────────────────────────────────────

function RateShopCalculator() {
  const [serviceType, setServiceType] = useState<ServiceType>("standard");
  const [weightKg, setWeightKg]       = useState<string>("5");
  const [quotes, setQuotes]           = useState<RateQuote[] | null>(null);
  const [loading, setLoading]         = useState(false);
  const [error, setError]             = useState<string | null>(null);

  const weightNumber = useMemo(() => {
    const w = parseFloat(weightKg);
    return Number.isFinite(w) && w > 0 ? w : null;
  }, [weightKg]);

  async function handleShop() {
    if (weightNumber === null) return;
    setLoading(true);
    setError(null);
    try {
      const q = await carriersApi.rateShop({ service_type: serviceType, weight_kg: weightNumber });
      setQuotes(q);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Rate shop failed");
      setQuotes(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <GlassCard padding="none">
      <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
        <div>
          <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
            <Calculator size={14} className="text-purple-plasma" />
            Rate Shop Calculator
          </h2>
          <p className="text-2xs font-mono text-white/30 mt-0.5">Compare carrier quotes for a hypothetical shipment.</p>
        </div>
      </div>

      <div className="grid grid-cols-[1fr_1fr_120px] gap-3 px-5 py-4 border-b border-glass-border">
        <div>
          <label className="mb-1 block text-2xs font-mono text-white/40 uppercase tracking-wider">Service</label>
          <select
            value={serviceType}
            onChange={(e) => setServiceType(e.target.value as ServiceType)}
            className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-sm text-white outline-none focus:border-cyan-neon/40"
          >
            {SERVICE_TYPES.map((t) => (
              <option key={t} value={t} style={{ background: "#0d1422" }}>{t.replace(/_/g, " ")}</option>
            ))}
          </select>
        </div>
        <div>
          <label className="mb-1 block text-2xs font-mono text-white/40 uppercase tracking-wider">Weight (kg)</label>
          <input
            type="number"
            inputMode="decimal"
            min={0.1}
            step={0.1}
            value={weightKg}
            onChange={(e) => setWeightKg(e.target.value)}
            className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-sm text-white outline-none focus:border-cyan-neon/40"
          />
        </div>
        <div className="flex items-end">
          <button
            onClick={handleShop}
            disabled={loading || weightNumber === null}
            className="w-full rounded-lg bg-gradient-to-r from-purple-plasma to-cyan-neon px-4 py-2 text-xs font-semibold text-white disabled:opacity-40 transition-opacity"
          >
            {loading ? "Shopping…" : "Get Quotes"}
          </button>
        </div>
      </div>

      {error && (
        <p className="px-5 py-3 text-xs text-red-signal font-mono">{error}</p>
      )}

      {quotes && (
        <>
          <div className="grid grid-cols-[2fr_100px_120px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Carrier", "Quote", "Eligible", "Reason"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {quotes.length === 0 ? (
            <p className="px-5 py-10 text-center text-xs text-white/40 font-mono">
              No carriers match this service + weight. Adjust inputs or onboard more carriers.
            </p>
          ) : (
            quotes.map((q) => (
              <div key={q.carrier_id} className="grid grid-cols-[2fr_100px_120px_1fr] gap-3 items-center px-5 py-3 border-b border-glass-border/50">
                <span className="text-xs font-medium text-white">{q.carrier_name}</span>
                <span className={`text-sm font-bold font-mono ${q.eligible ? "text-green-signal" : "text-white/40"}`}>
                  {fmtPhp(q.total_cost_cents)}
                </span>
                <NeonBadge variant={q.eligible ? "green" : "muted"} dot>
                  {q.eligible ? "eligible" : "ineligible"}
                </NeonBadge>
                <span className="text-2xs text-white/40 font-mono truncate" title={q.ineligibility_reason ?? ""}>
                  {q.eligible ? "—" : q.ineligibility_reason ?? "—"}
                </span>
              </div>
            ))
          )}
        </>
      )}
    </GlassCard>
  );
}
