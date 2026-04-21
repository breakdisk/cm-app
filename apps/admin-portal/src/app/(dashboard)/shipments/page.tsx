"use client";
/**
 * Admin Portal — Shipments Page
 *
 * Authoritative list of every shipment across the tenant, regardless of
 * dispatch state. Customer-booked shipments (booked_by_customer=true) and
 * merchant-booked shipments appear here together; the dispatch console
 * only shows the pending subset, so this page is the place to verify an
 * ingest actually landed when dispatch is down or slow.
 *
 * Data source: GET /v1/shipments (api-gateway → order-intake).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { Search, RefreshCw, Package, User } from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { variants } from "@/lib/design-system/tokens";
import { authFetch } from "@/lib/auth/auth-fetch";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// Raw shape emitted by order-intake's `Shipment` entity. Newtype wrappers
// (ShipmentId, Awb, Money, etc.) flatten to plain primitives via serde.
interface ApiAddress {
  line1: string;
  line2?: string | null;
  city: string;
  province: string;
  postal_code: string;
  country_code: string;
  coordinates?: { lat: number; lng: number } | null;
}

interface ApiMoney {
  amount: number;   // in minor units (cents)
  currency: string; // "PHP", "USD", ...
}

interface ApiShipment {
  id: string;
  tenant_id: string;
  merchant_id: string;
  customer_id: string;
  customer_name: string;
  customer_phone: string;
  customer_email?: string | null;
  booked_by_customer: boolean;
  awb: string;
  piece_count: number;
  status: string;
  service_type: string;
  origin: ApiAddress;
  destination: ApiAddress;
  weight: { grams: number };
  declared_value?: ApiMoney | null;
  cod_amount?: ApiMoney | null;
  created_at: string;
  updated_at: string;
}

const STATUS_VARIANT: Record<string, "green" | "cyan" | "amber" | "red" | "purple"> = {
  pending:            "amber",
  confirmed:          "cyan",
  pickup_assigned:    "cyan",
  picked_up:          "cyan",
  in_transit:         "purple",
  at_hub:             "purple",
  out_for_delivery:   "cyan",
  delivery_attempted: "amber",
  delivered:          "green",
  partial_delivery:   "amber",
  piece_exception:    "amber",
  customs_hold:       "amber",
  failed:             "red",
  cancelled:          "red",
  returned:           "red",
};

const STATUS_FILTERS = [
  "all",
  "pending",
  "picked_up",
  "in_transit",
  "out_for_delivery",
  "delivered",
  "failed",
] as const;
type StatusFilter = (typeof STATUS_FILTERS)[number];

function prettyStatus(s: string): string {
  return s.replace(/_/g, " ");
}

function formatMoney(m: ApiMoney | null | undefined): string | null {
  if (!m) return null;
  const symbol = m.currency === "PHP" ? "₱" : `${m.currency} `;
  return `${symbol}${(m.amount / 100).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

function formatRelative(iso: string): string {
  const then = new Date(iso).getTime();
  const now  = Date.now();
  const sec  = Math.max(0, Math.round((now - then) / 1000));
  if (sec < 60)   return `${sec}s ago`;
  if (sec < 3600) return `${Math.floor(sec / 60)}m ago`;
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ago`;
  return `${Math.floor(sec / 86400)}d ago`;
}

export default function ShipmentsPage() {
  const [shipments,  setShipments]  = useState<ApiShipment[]>([]);
  const [loading,    setLoading]    = useState(false);
  const [error,      setError]      = useState<string | null>(null);
  const [search,     setSearch]     = useState("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");

  const fetchShipments = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await authFetch(`${API_BASE}/v1/shipments?per_page=100`);
      if (!res.ok) {
        throw new Error(`${res.status} ${res.statusText}`);
      }
      const json = await res.json();
      setShipments(json.shipments ?? []);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load shipments");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { fetchShipments(); }, [fetchShipments]);

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    return shipments.filter((s) => {
      if (statusFilter !== "all" && s.status !== statusFilter) return false;
      if (!q) return true;
      return (
        s.awb.toLowerCase().includes(q) ||
        s.customer_name.toLowerCase().includes(q) ||
        s.destination.city.toLowerCase().includes(q)
      );
    });
  }, [shipments, search, statusFilter]);

  const kpi = useMemo(() => {
    const total         = shipments.length;
    const pending       = shipments.filter((s) => s.status === "pending").length;
    const inTransit     = shipments.filter((s) => ["picked_up", "in_transit", "at_hub", "out_for_delivery"].includes(s.status)).length;
    const deliveredToday = shipments.filter((s) => {
      if (s.status !== "delivered") return false;
      const updated = new Date(s.updated_at);
      const today   = new Date();
      return updated.toDateString() === today.toDateString();
    }).length;
    return [
      { label: "Total Shipments", value: total,          trend: 0, color: "cyan"   as const, format: "number" as const },
      { label: "Pending Ingest",  value: pending,        trend: 0, color: "amber"  as const, format: "number" as const },
      { label: "In Transit",      value: inTransit,      trend: 0, color: "purple" as const, format: "number" as const },
      { label: "Delivered Today", value: deliveredToday, trend: 0, color: "green"  as const, format: "number" as const },
    ];
  }, [shipments]);

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white">Shipments</h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">
            {filtered.length} of {shipments.length} shown · all origins
          </p>
        </div>
        <button
          onClick={fetchShipments}
          disabled={loading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {error && (
        <div className="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-2 text-sm font-mono text-red-400">
          {error}
        </div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {kpi.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Filters */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex flex-wrap items-center gap-1.5">
              {STATUS_FILTERS.map((s) => (
                <button
                  key={s}
                  onClick={() => setStatusFilter(s)}
                  className={`rounded-full px-3 py-1 text-xs font-medium capitalize transition-all ${
                    statusFilter === s
                      ? "bg-cyan-surface text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 border border-glass-border hover:text-white"
                  }`}
                >
                  {s === "all" ? "All" : prettyStatus(s)}
                </button>
              ))}
            </div>
            <div className="ml-auto flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2">
              <Search size={13} className="text-white/30" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="AWB · customer · city…"
                className="bg-transparent text-xs text-white placeholder:text-white/25 outline-none font-mono w-48"
              />
            </div>
          </div>
        </GlassCard>
      </motion.div>

      {/* Table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard>
          {filtered.length === 0 && !loading && (
            <div className="flex flex-col items-center justify-center py-10 text-center">
              <Package className="h-8 w-8 text-white/20 mb-2" />
              <p className="text-sm text-white/40">No shipments match your filters.</p>
            </div>
          )}

          {filtered.length > 0 && (
            <div className="overflow-x-auto">
              <table className="w-full text-left text-sm">
                <thead>
                  <tr className="border-b border-glass-border text-2xs font-mono uppercase tracking-widest text-white/30">
                    <th className="py-2 pr-4 font-normal">AWB</th>
                    <th className="py-2 pr-4 font-normal">Customer</th>
                    <th className="py-2 pr-4 font-normal">Route</th>
                    <th className="py-2 pr-4 font-normal">Service</th>
                    <th className="py-2 pr-4 font-normal">Status</th>
                    <th className="py-2 pr-4 font-normal">COD</th>
                    <th className="py-2 pr-4 font-normal">Source</th>
                    <th className="py-2 pr-0 font-normal text-right">Created</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((s) => {
                    const variant = STATUS_VARIANT[s.status] ?? "cyan";
                    const cod     = formatMoney(s.cod_amount);
                    return (
                      <tr
                        key={s.id}
                        className="border-b border-glass-border/40 hover:bg-glass-100/40 transition-colors"
                      >
                        <td className="py-2.5 pr-4">
                          <span className="font-mono text-xs text-cyan-neon">{s.awb}</span>
                          {s.piece_count > 1 && (
                            <span className="ml-1.5 text-2xs font-mono text-white/30">×{s.piece_count}</span>
                          )}
                        </td>
                        <td className="py-2.5 pr-4">
                          <p className="text-xs text-white truncate max-w-[180px]">{s.customer_name}</p>
                          <p className="text-2xs font-mono text-white/40 truncate max-w-[180px]">{s.customer_phone}</p>
                        </td>
                        <td className="py-2.5 pr-4">
                          <p className="text-2xs font-mono text-white/50 truncate max-w-[200px]">
                            {s.origin.city} → {s.destination.city}
                          </p>
                          <p className="text-2xs font-mono text-white/30 truncate max-w-[200px]">
                            {s.destination.line1}
                          </p>
                        </td>
                        <td className="py-2.5 pr-4">
                          <span className="text-xs capitalize text-white/70">{prettyStatus(s.service_type)}</span>
                        </td>
                        <td className="py-2.5 pr-4">
                          <NeonBadge variant={variant}>{prettyStatus(s.status)}</NeonBadge>
                        </td>
                        <td className="py-2.5 pr-4">
                          {cod ? (
                            <span className="text-xs font-mono text-amber-signal">{cod}</span>
                          ) : (
                            <span className="text-2xs font-mono text-white/20">—</span>
                          )}
                        </td>
                        <td className="py-2.5 pr-4">
                          {s.booked_by_customer ? (
                            <span className="inline-flex items-center gap-1 text-2xs font-mono text-cyan-neon/80">
                              <User size={10} /> Customer
                            </span>
                          ) : (
                            <span className="text-2xs font-mono text-white/40">Merchant</span>
                          )}
                        </td>
                        <td className="py-2.5 pr-0 text-right">
                          <span className="text-2xs font-mono text-white/40">{formatRelative(s.created_at)}</span>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
