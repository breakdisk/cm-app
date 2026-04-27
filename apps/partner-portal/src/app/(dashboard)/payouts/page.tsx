"use client";
/**
 * Partner Portal — Payouts Page
 *
 * Surfaces the partner tenant's payments-service state:
 *   - Wallet balance (`GET /v1/wallet`) = cash available to withdraw
 *   - Invoices (`GET /v1/invoices`)     = shipment-charge billing runs
 *
 * A true carrier-settlement entity (distinct from merchant invoicing) is
 * blocked on ADR-0013 partner scoping — until then this page shows the
 * real tenant-scoped billing data rather than mock payout rows.
 */
import { useState, useEffect, useCallback, useMemo } from "react";
import { useRosterEvents } from "@/hooks/useRosterEvents";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { CreditCard, RefreshCw, CheckCircle2, Clock, AlertCircle, FileText } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

const PAYMENTS_URL = process.env.NEXT_PUBLIC_PAYMENTS_URL ?? "http://localhost:8008";

// ── Backend shapes ────────────────────────────────────────────────────────────

interface InvoiceSummary {
  invoice_id:     string;
  invoice_number: string;
  invoice_type:   string;
  status:         string;   // "draft" | "issued" | "paid" | "overdue" | "disputed" | "cancelled"
  awb_count:      number;
  subtotal_cents: number;
  vat_cents:      number;
  total_cents:    number;
  billing_period: string;   // "YYYY-MM"
  due_at:         string;
  issued_at:      string;
}

interface WalletSummary {
  wallet_id:     string;
  balance_cents: number;
  currency:      string;
  updated_at:    string;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatPeso(cents: number): string {
  return `₱${Math.round(cents / 100).toLocaleString()}`;
}

function formatBillingPeriod(period: string): string {
  const [y, m] = period.split("-").map(Number);
  if (!y || !m) return period;
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleString("en-US", { year: "numeric", month: "long", timeZone: "UTC" });
}

function formatShortMonth(period: string): string {
  const [y, m] = period.split("-").map(Number);
  if (!y || !m) return period;
  return new Date(Date.UTC(y, m - 1, 1)).toLocaleString("en-US", { month: "short", timeZone: "UTC" });
}

type StatusBadge = { label: string; variant: "green" | "cyan" | "amber" | "red" | "purple"; icon: React.ReactNode };

function statusBadge(status: string): StatusBadge {
  switch (status) {
    case "paid":      return { label: "Paid",     variant: "green",  icon: <CheckCircle2 size={11} /> };
    case "issued":    return { label: "Issued",   variant: "amber",  icon: <Clock size={11} />        };
    case "overdue":   return { label: "Overdue",  variant: "red",    icon: <AlertCircle size={11} /> };
    case "draft":     return { label: "Draft",    variant: "cyan",   icon: <FileText size={11} />    };
    case "disputed":  return { label: "Disputed", variant: "purple", icon: <AlertCircle size={11} /> };
    case "cancelled": return { label: "Void",     variant: "red",    icon: <AlertCircle size={11} /> };
    default:          return { label: status,     variant: "cyan",   icon: <Clock size={11} />        };
  }
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function PayoutsPage() {
  const [invoices,  setInvoices]  = useState<InvoiceSummary[]>([]);
  const [wallet,    setWallet]    = useState<WalletSummary | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    setIsLoading(true);
    setLoadError(null);
    try {
      const [invRes, walletRes] = await Promise.all([
        authFetch(`${PAYMENTS_URL}/v1/invoices`),
        authFetch(`${PAYMENTS_URL}/v1/wallet`),
      ]);

      if (invRes.ok) {
        const json = await invRes.json();
        setInvoices(Array.isArray(json.data) ? json.data : []);
      } else {
        setLoadError(`Invoices: HTTP ${invRes.status}`);
        setInvoices([]);
      }

      if (walletRes.ok) {
        const json = await walletRes.json();
        setWallet(json.data ?? null);
      } else {
        setWallet(null);
      }
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : "Network error");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { loadData(); }, [loadData]);

  // Refresh on driver status flips — commission accrual moves wallet balance.
  // 60s poll backstop for anything the roster channel doesn't catch.
  useRosterEvents((event) => {
    if (event.type === "status_changed") loadData();
  });
  useEffect(() => {
    const id = setInterval(loadData, 60_000);
    return () => clearInterval(id);
  }, [loadData]);

  // ── Derived metrics ─────────────────────────────────────────────────────────

  const kpi = useMemo(() => {
    const now = new Date();
    const thisMonthKey = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;

    const thisMonth   = invoices.filter((i) => i.billing_period === thisMonthKey);
    const outstanding = invoices.filter((i) => i.status === "issued" || i.status === "overdue");
    const paid        = invoices.filter((i) => i.status === "paid");

    return {
      billedMtd:      Math.round(thisMonth.reduce((a, i) => a + i.total_cents, 0) / 100),
      paidAllTime:    Math.round(paid.reduce((a, i) => a + i.total_cents, 0) / 100),
      outstanding:    Math.round(outstanding.reduce((a, i) => a + i.total_cents, 0) / 100),
      shipmentsBilled: invoices.reduce((a, i) => a + (i.awb_count ?? 0), 0),
    };
  }, [invoices]);

  // Group by billing_period for the 6-month chart. Sorted oldest → newest.
  const monthlyBreakdown = useMemo(() => {
    const byPeriod = new Map<string, { paid: number; issued: number; overdue: number }>();
    for (const inv of invoices) {
      const entry = byPeriod.get(inv.billing_period) ?? { paid: 0, issued: 0, overdue: 0 };
      if (inv.status === "paid")    entry.paid    += inv.total_cents;
      if (inv.status === "issued")  entry.issued  += inv.total_cents;
      if (inv.status === "overdue") entry.overdue += inv.total_cents;
      byPeriod.set(inv.billing_period, entry);
    }
    return Array.from(byPeriod.entries())
      .sort(([a], [b]) => a.localeCompare(b))
      .slice(-6)
      .map(([period, sums]) => ({
        month:   formatShortMonth(period),
        paid:    Math.round(sums.paid    / 100),
        issued:  Math.round(sums.issued  / 100),
        overdue: Math.round(sums.overdue / 100),
      }));
  }, [invoices]);

  const walletBalance = wallet ? Math.round(wallet.balance_cents / 100) : 0;

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
            <CreditCard size={20} className="text-green-signal" />
            Payouts
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Wallet &amp; billing · Payout schedule: 5th of each month</p>
        </div>
        <button
          onClick={loadData}
          disabled={isLoading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={isLoading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {loadError && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm" glow="red">
            <p className="text-xs font-mono text-red-signal">{loadError}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <GlassCard size="sm" glow="green" accent>
          <LiveMetric label="Wallet Balance"    value={walletBalance}     color="green"  format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="cyan" accent>
          <LiveMetric label="Billed MTD"        value={kpi.billedMtd}     color="cyan"   format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="amber" accent>
          <LiveMetric label="Outstanding"       value={kpi.outstanding}   color="amber"  format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="purple" accent>
          <LiveMetric label="Shipments Billed"  value={kpi.shipmentsBilled} color="purple" format="number" />
        </GlassCard>
      </motion.div>

      {/* Payout trend chart */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Monthly Billing Breakdown</h2>
              <p className="text-2xs font-mono text-white/30">Paid · Issued · Overdue</p>
            </div>
            <NeonBadge variant="green">{formatPeso(kpi.billedMtd * 100)} MTD</NeonBadge>
          </div>

          {monthlyBreakdown.length === 0 ? (
            <div className="py-12 text-center text-xs font-mono text-white/40">
              No billing history yet.
            </div>
          ) : (
            <>
              <ResponsiveContainer width="100%" height={200}>
                <BarChart data={monthlyBreakdown} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
                  <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
                  <XAxis dataKey="month" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                  <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                  <Tooltip
                    contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                    formatter={(v) => [`₱${Number(v).toLocaleString()}`, ""]}
                  />
                  <Bar dataKey="paid"    fill="#00FF88" stackId="a" fillOpacity={0.85} />
                  <Bar dataKey="issued"  fill="#FFAB00" stackId="a" fillOpacity={0.8}  />
                  <Bar dataKey="overdue" fill="#FF4D4F" stackId="a" fillOpacity={0.8} radius={[4,4,0,0]} />
                </BarChart>
              </ResponsiveContainer>
              <div className="flex items-center gap-4 mt-3">
                {[["Paid", "#00FF88"], ["Issued", "#FFAB00"], ["Overdue", "#FF4D4F"]].map(([label, color]) => (
                  <div key={label} className="flex items-center gap-1.5">
                    <div className="h-2 w-2 rounded-full" style={{ background: color }} />
                    <span className="text-2xs font-mono text-white/40">{label}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </GlassCard>
      </motion.div>

      {/* Invoice history */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Invoice History</h2>
            <span className="text-2xs font-mono text-white/30">{invoices.length} invoice{invoices.length === 1 ? "" : "s"}</span>
          </div>

          <div className="grid grid-cols-[2fr_90px_110px_90px_110px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Period", "AWBs", "Subtotal", "VAT", "Total", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {isLoading && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">Loading…</div>
          )}

          {!isLoading && invoices.length === 0 && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">
              No invoices yet. Your first billing run will appear here.
            </div>
          )}

          {!isLoading && invoices.map((inv) => {
            const badge = statusBadge(inv.status);
            return (
              <div
                key={inv.invoice_id}
                className="grid grid-cols-[2fr_90px_110px_90px_110px_100px] gap-3 items-center px-5 py-4 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
              >
                <div>
                  <p className="text-xs font-medium text-white">{formatBillingPeriod(inv.billing_period)}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{inv.invoice_number}</p>
                </div>
                <span className="text-xs font-mono text-white/60">{inv.awb_count.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">{formatPeso(inv.subtotal_cents)}</span>
                <span className="text-xs font-mono text-amber-signal">{formatPeso(inv.vat_cents)}</span>
                <span className="text-sm font-bold font-heading text-green-signal">{formatPeso(inv.total_cents)}</span>
                <NeonBadge variant={badge.variant}>
                  <span className="flex items-center gap-1">{badge.icon}{badge.label}</span>
                </NeonBadge>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
