"use client";
/**
 * Partner Portal — Payouts Page
 * Carrier payout history, pending remittances, COD reconciliation.
 */
import { useState, useEffect } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { CreditCard, Download, CheckCircle2, Clock, AlertCircle } from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

// ── API helpers ────────────────────────────────────────────────────────────────

const PAYMENTS_URL = process.env.NEXT_PUBLIC_PAYMENTS_URL ?? "http://localhost:8008";

async function fetchInvoices() {
  try {
    const res = await authFetch(`${PAYMENTS_URL}/v1/invoices`);
    if (!res.ok) return null;
    const json = await res.json();
    return json.data ?? json.invoices ?? null;
  } catch {
    return null;
  }
}

async function fetchWallet() {
  try {
    const res = await authFetch(`${PAYMENTS_URL}/v1/wallet`);
    if (!res.ok) return null;
    const json = await res.json();
    return json.data ?? null;
  } catch {
    return null;
  }
}

// ── Mock data ──────────────────────────────────────────────────────────────────

const MONTHLY_PAYOUTS = [
  { month: "Oct", base: 310000, cod: 190000, bonus: 12000 },
  { month: "Nov", base: 328000, cod: 212000, bonus: 18000 },
  { month: "Dec", base: 401000, cod: 268000, bonus: 42000 },
  { month: "Jan", base: 335000, cod: 218000, bonus: 8000  },
  { month: "Feb", base: 362000, cod: 241000, bonus: 14000 },
  { month: "Mar", base: 421000, cod: 284000, bonus: 22000 },
];

type PayoutStatus = "paid" | "processing" | "pending" | "disputed";

interface PayoutRecord {
  id: string;
  period: string;
  deliveries: number;
  base_rate: number;
  cod_remittance: number;
  bonus: number;
  total: number;
  status: PayoutStatus;
  paid_date?: string;
}

const PAYOUT_HISTORY_DEFAULT: PayoutRecord[] = [
  { id: "P-2026-03", period: "March 2026 (MTD)", deliveries: 8420, base_rate: 280000, cod_remittance: 284000, bonus: 22000, total: 421000, status: "processing" },
  { id: "P-2026-02", period: "February 2026",     deliveries: 7840, base_rate: 241000, cod_remittance: 241000, bonus: 14000, total: 362000, status: "paid",       paid_date: "Mar 5, 2026" },
  { id: "P-2026-01", period: "January 2026",       deliveries: 7120, base_rate: 218000, cod_remittance: 218000, bonus: 8000,  total: 335000, status: "paid",       paid_date: "Feb 5, 2026" },
  { id: "P-2025-12", period: "December 2025",      deliveries: 9840, base_rate: 268000, cod_remittance: 268000, bonus: 42000, total: 401000, status: "paid",       paid_date: "Jan 5, 2026" },
  { id: "P-2025-11", period: "November 2025",      deliveries: 7320, base_rate: 212000, cod_remittance: 212000, bonus: 18000, total: 328000, status: "paid",       paid_date: "Dec 5, 2025" },
];

const STATUS_CONFIG: Record<PayoutStatus, { label: string; variant: "green" | "cyan" | "amber" | "red"; icon: React.ReactNode }> = {
  paid:       { label: "Paid",        variant: "green", icon: <CheckCircle2 size={11} /> },
  processing: { label: "Processing",  variant: "amber", icon: <Clock size={11} />        },
  pending:    { label: "Pending",     variant: "cyan",  icon: <Clock size={11} />        },
  disputed:   { label: "Disputed",    variant: "red",   icon: <AlertCircle size={11} /> },
};

export default function PayoutsPage() {
  const [payoutHistory, setPayoutHistory] = useState<PayoutRecord[]>(PAYOUT_HISTORY_DEFAULT);
  const [pendingPayout, setPendingPayout] = useState<number>(62000);

  useEffect(() => {
    async function loadData() {
      const [invoices, wallet] = await Promise.all([fetchInvoices(), fetchWallet()]);

      if (invoices && Array.isArray(invoices) && invoices.length > 0) {
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        const mapped: PayoutRecord[] = invoices.map((inv: any) => ({
          id:             inv.id,
          period:         inv.period ?? inv.billing_period ?? "—",
          deliveries:     inv.deliveries_count ?? inv.deliveries ?? 0,
          base_rate:      inv.base_amount ?? inv.base_rate ?? 0,
          cod_remittance: inv.cod_amount ?? 0,
          bonus:          inv.bonus_amount ?? inv.bonus ?? 0,
          total:          inv.total_amount ?? inv.total ?? 0,
          status:         inv.status ?? "pending",
          paid_date:      inv.paid_date ?? inv.paid_at ?? undefined,
        }));
        setPayoutHistory(mapped);
      }

      if (wallet) {
        const balance = wallet.pending_balance ?? wallet.balance ?? wallet.pending_payout ?? null;
        if (balance !== null) {
          setPendingPayout(Number(balance));
        }
      }
    }

    loadData();
  }, []);

  const KPI = [
    { label: "Payout MTD",        value: 421000,        trend: +14.2, color: "green"  as const, format: "currency" as const },
    { label: "COD Remitted",      value: 284000,        trend: +9.8,  color: "amber"  as const, format: "currency" as const },
    { label: "Pending Payout",    value: pendingPayout, trend: -4.1,  color: "cyan"   as const, format: "currency" as const },
    { label: "Deliveries Billed", value: 8420,          trend: +14.2, color: "purple" as const, format: "number"  as const },
  ];

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
          <p className="text-sm text-white/40 font-mono mt-0.5">FastLine Couriers · Payout schedule: 5th of each month</p>
        </div>
        <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
          <Download size={12} /> Export CSV
        </button>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Payout trend chart */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="green">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white">Monthly Payout Breakdown</h2>
              <p className="text-2xs font-mono text-white/30">Base · COD Remittance · Bonus</p>
            </div>
            <NeonBadge variant="green">₱421K MTD</NeonBadge>
          </div>
          <ResponsiveContainer width="100%" height={200}>
            <BarChart data={MONTHLY_PAYOUTS} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
              <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
              <XAxis dataKey="month" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
              <Tooltip
                contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                formatter={(v) => [`₱${Number(v).toLocaleString()}`, ""]}
              />
              <Bar dataKey="base"          fill="#00FF88" radius={[0,0,0,0]} fillOpacity={0.85} stackId="a" />
              <Bar dataKey="cod"           fill="#00E5FF" radius={[0,0,0,0]} fillOpacity={0.7}  stackId="a" />
              <Bar dataKey="bonus"         fill="#A855F7" radius={[4,4,0,0]} fillOpacity={0.8}  stackId="a" />
            </BarChart>
          </ResponsiveContainer>
          <div className="flex items-center gap-4 mt-3">
            {[["Base Rate", "#00FF88"], ["COD Remittance", "#00E5FF"], ["Bonus", "#A855F7"]].map(([label, color]) => (
              <div key={label} className="flex items-center gap-1.5">
                <div className="h-2 w-2 rounded-full" style={{ background: color }} />
                <span className="text-2xs font-mono text-white/40">{label}</span>
              </div>
            ))}
          </div>
        </GlassCard>
      </motion.div>

      {/* Payout history table */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Payout History</h2>
          </div>

          {/* Header */}
          <div className="grid grid-cols-[2fr_80px_100px_100px_80px_100px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Period", "Deliveries", "Base Rate", "COD", "Bonus", "Total", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {payoutHistory.map((p) => {
            const cfg = STATUS_CONFIG[p.status] ?? STATUS_CONFIG["pending"];
            const { label, variant, icon } = cfg;
            return (
              <div key={p.id} className="grid grid-cols-[2fr_80px_100px_100px_80px_100px_100px] gap-3 items-center px-5 py-4 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <div>
                  <p className="text-xs font-medium text-white">{p.period}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{p.id}</p>
                </div>
                <span className="text-xs font-mono text-white/60">{p.deliveries.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">₱{p.base_rate.toLocaleString()}</span>
                <span className="text-xs font-mono text-amber-signal">₱{p.cod_remittance.toLocaleString()}</span>
                <span className="text-xs font-mono text-purple-plasma">₱{p.bonus.toLocaleString()}</span>
                <span className="text-sm font-bold font-heading text-green-signal">₱{p.total.toLocaleString()}</span>
                <div className="flex flex-col gap-0.5">
                  <NeonBadge variant={variant}>
                    <span className="flex items-center gap-1">{icon}{label}</span>
                  </NeonBadge>
                  {p.paid_date && <span className="text-2xs font-mono text-white/30">{p.paid_date}</span>}
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
