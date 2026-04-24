"use client";
/**
 * Merchant Portal — Customers Page
 * Surfaces the Customer Data Platform (cdp service) for the logged-in tenant:
 *   GET /v1/customers           → full list (with search filters)
 *   GET /v1/customers/top-clv   → top-N customers by CLV score
 *
 * Clicking a row navigates to /customers/:external_id (detail page — future work).
 * For now we show the list + top-CLV leaderboard; the detail view is a
 * placeholder navigation target.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Users, Search, RefreshCw, TrendingUp, Package, Mail, Phone } from "lucide-react";
import {
  createCdpApi,
  deliverySuccessRate,
  type CustomerProfile,
} from "@/lib/api/cdp";

function clvTier(score: number): { label: string; variant: "green" | "cyan" | "purple" | "amber" | "muted" } {
  if (score >= 80) return { label: "Platinum", variant: "green" };
  if (score >= 60) return { label: "Gold",     variant: "amber" };
  if (score >= 40) return { label: "Silver",   variant: "cyan" };
  if (score >= 20) return { label: "Bronze",   variant: "purple" };
  return { label: "New", variant: "muted" };
}

function fmtPhp(cents: number): string {
  return `₱${(cents / 100).toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 0 })}`;
}

export default function CustomersPage() {
  const api = useMemo(() => createCdpApi(), []);

  const [profiles, setProfiles] = useState<CustomerProfile[]>([]);
  const [top, setTop]           = useState<CustomerProfile[]>([]);
  const [search, setSearch]     = useState("");
  const [loading, setLoading]   = useState(true);
  const [error, setError]       = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const [list, top] = await Promise.all([
        api.list({ name: search || undefined, limit: 100 }),
        api.topByClv(5),
      ]);
      setProfiles(list.profiles ?? []);
      setTop(top.profiles ?? []);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load customers");
    } finally {
      setLoading(false);
    }
  }, [api, search]);

  useEffect(() => { load(); }, [load]);

  // Derived KPIs — server doesn't return aggregate counts at tenant scope yet,
  // so we compute from the fetched list. For tenants with > 100 customers this
  // undercounts; a follow-up ticket adds a /v1/customers/stats endpoint.
  const kpis = useMemo(() => {
    const totalShipments = profiles.reduce((n, p) => n + p.total_shipments, 0);
    const avgClv = profiles.length === 0 ? 0
      : profiles.reduce((n, p) => n + p.clv_score, 0) / profiles.length;
    const successRate = totalShipments === 0 ? 0
      : (profiles.reduce((n, p) => n + p.successful_deliveries, 0) / totalShipments) * 100;
    return [
      { label: "Total Customers", value: profiles.length, trend: 0, color: "cyan"   as const, format: "number"  as const },
      { label: "Avg CLV Score",   value: avgClv,          trend: 0, color: "purple" as const, format: "number"  as const },
      { label: "Total Shipments", value: totalShipments,  trend: 0, color: "amber"  as const, format: "number"  as const },
      { label: "Success Rate",    value: successRate,     trend: 0, color: "green"  as const, format: "percent" as const },
    ];
  }, [profiles]);

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
            <Users size={22} className="text-cyan-neon" />
            Customers
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            Customer Data Platform · {profiles.length} profiles loaded
          </p>
        </div>
        <button
          onClick={load}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          title="Refresh"
        >
          <RefreshCw size={13} />
        </button>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="sm">
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPIs */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpis.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Top CLV leaderboard + search */}
      <motion.div variants={variants.fadeInUp} className="grid gap-4 lg:grid-cols-[1fr_320px]">
        {/* Search + list */}
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border gap-3">
            <h2 className="font-heading text-sm font-semibold text-white whitespace-nowrap">All Customers</h2>
            <div className="relative flex-1 max-w-md">
              <Search size={14} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search by name…"
                className="w-full rounded-lg border border-glass-border bg-glass-100 pl-9 pr-3 py-2 text-xs text-white placeholder-white/25 outline-none focus:border-cyan-neon/40 transition-colors"
              />
            </div>
          </div>

          <div className="grid grid-cols-[2fr_90px_90px_90px_1fr] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Customer", "CLV", "Shipments", "Success", "Recent Address"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {loading ? (
            <div className="px-5 py-10 text-center text-xs text-white/40 font-mono">loading…</div>
          ) : profiles.length === 0 ? (
            <div className="px-5 py-10 text-center">
              <p className="text-xs text-white/40 font-mono">
                {search ? `No customers match "${search}"` : "No customers yet"}
              </p>
            </div>
          ) : (
            profiles.map((p) => {
              const tier = clvTier(p.clv_score);
              const successRate = deliverySuccessRate(p);
              const topAddr = p.address_history[0]?.address ?? "—";
              return (
                <div
                  key={p.external_customer_id}
                  className="grid grid-cols-[2fr_90px_90px_90px_1fr] gap-3 items-center px-5 py-3 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
                >
                  <div className="min-w-0">
                    <p className="text-xs font-medium text-white truncate">{p.name ?? "Unnamed customer"}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 flex items-center gap-2 truncate">
                      {p.email && (<span className="flex items-center gap-1"><Mail size={10} />{p.email}</span>)}
                      {p.phone && (<span className="flex items-center gap-1"><Phone size={10} />{p.phone}</span>)}
                      {!p.email && !p.phone && <span>{p.external_customer_id.slice(0, 8)}…</span>}
                    </p>
                  </div>
                  <NeonBadge variant={tier.variant} dot>{tier.label}</NeonBadge>
                  <span className="text-xs font-mono text-white/60">{p.total_shipments}</span>
                  <span className={`text-xs font-mono font-semibold ${
                    successRate > 90 ? "text-green-signal" :
                    successRate > 70 ? "text-cyan-neon" :
                    p.total_shipments === 0 ? "text-white/30" : "text-amber-signal"
                  }`}>
                    {p.total_shipments === 0 ? "—" : `${successRate.toFixed(0)}%`}
                  </span>
                  <span className="text-xs text-white/40 font-mono truncate" title={topAddr}>{topAddr}</span>
                </div>
              );
            })
          )}
        </GlassCard>

        {/* Top CLV */}
        <GlassCard>
          <div className="flex items-center justify-between mb-4">
            <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
              <TrendingUp size={14} className="text-green-signal" />
              Top Value Customers
            </h2>
          </div>

          <div className="flex flex-col gap-3">
            {top.length === 0 ? (
              <p className="text-xs text-white/30 font-mono">No data yet.</p>
            ) : top.map((p, i) => {
              const tier = clvTier(p.clv_score);
              return (
                <div key={p.external_customer_id} className="flex items-center gap-3">
                  <div className="flex h-7 w-7 items-center justify-center rounded-full bg-glass-100 border border-glass-border text-xs font-mono text-white/60">
                    {i + 1}
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="text-xs font-medium text-white truncate">{p.name ?? p.external_customer_id.slice(0, 8)}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5 flex items-center gap-2">
                      <span>CLV {p.clv_score.toFixed(0)}</span>
                      <span className="flex items-center gap-0.5"><Package size={9} />{p.total_shipments}</span>
                      {p.total_cod_collected_cents > 0 && (
                        <span className="text-green-signal">{fmtPhp(p.total_cod_collected_cents)}</span>
                      )}
                    </p>
                  </div>
                  <NeonBadge variant={tier.variant}>{tier.label}</NeonBadge>
                </div>
              );
            })}
          </div>
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
