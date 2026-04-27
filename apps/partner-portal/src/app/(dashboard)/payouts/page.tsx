"use client";
/**
 * Partner Portal — Payouts Page
 * Live wallet balance, withdrawal modal, transaction history, and invoice table.
 * Backed by paymentsApi (wallet, transactions, invoices).
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  BarChart, Bar, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from "recharts";
import { CreditCard, Download, X } from "lucide-react";
import {
  paymentsApi,
  type Wallet,
  type WalletTransaction,
  type Invoice,
  type InvoiceStatus,
} from "@/lib/api/payments";
import { useRosterEvents } from "@/hooks/useRosterEvents";

// ── Static chart data (no backend endpoint for monthly breakdown) ─────────────
const MONTHLY_PAYOUTS = [
  { month: "Oct", base: 310000, cod: 190000, bonus: 12000 },
  { month: "Nov", base: 328000, cod: 212000, bonus: 18000 },
  { month: "Dec", base: 401000, cod: 268000, bonus: 42000 },
  { month: "Jan", base: 335000, cod: 218000, bonus: 8000  },
  { month: "Feb", base: 362000, cod: 241000, bonus: 14000 },
  { month: "Mar", base: 421000, cod: 284000, bonus: 22000 },
];

const STATUS_VARIANT: Record<InvoiceStatus, { label: string; variant: "green" | "amber" | "red" | "cyan" | "muted" }> = {
  paid:      { label: "Paid",      variant: "green" },
  issued:    { label: "Pending",   variant: "amber" },
  overdue:   { label: "Overdue",   variant: "red"   },
  draft:     { label: "Draft",     variant: "cyan"  },
  cancelled: { label: "Cancelled", variant: "muted" },
};

// ── Withdrawal modal ──────────────────────────────────────────────────────────
function WithdrawModal({
  wallet,
  onClose,
  onSuccess,
}: {
  wallet: Wallet;
  onClose: () => void;
  onSuccess: (updated: Wallet) => void;
}) {
  const [amount, setAmount]   = useState("");
  const [saving, setSaving]   = useState(false);
  const [error,  setError]    = useState<string | null>(null);

  async function handleSubmit() {
    const php = parseFloat(amount);
    if (Number.isNaN(php) || php <= 0) {
      setError("Enter a valid amount");
      return;
    }
    if (php > wallet.available_php) {
      setError(`Exceeds available balance of ₱${wallet.available_php.toLocaleString()}`);
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const updated = await paymentsApi.withdraw({ amount_php: php });
      onSuccess(updated);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Withdrawal failed");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-sm rounded-xl border border-glass-border bg-[#0A0E1A] p-6 shadow-xl">
        <div className="flex items-center justify-between mb-4">
          <h3 className="font-heading text-sm font-semibold text-white">Request Withdrawal</h3>
          <button onClick={onClose} className="text-white/40 hover:text-white transition-colors">
            <X size={16} />
          </button>
        </div>
        <p className="text-xs font-mono text-white/40 mb-1">
          Available: ₱{wallet.available_php.toLocaleString()}
        </p>
        <input
          type="number"
          min={1}
          max={wallet.available_php}
          placeholder="Amount in ₱"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-2 text-sm text-white font-mono placeholder-white/20 focus:border-green-signal/50 focus:outline-none mb-3"
        />
        {error && (
          <p className="text-xs text-red-signal font-mono mb-3">{error}</p>
        )}
        <div className="flex gap-2">
          <button
            onClick={onClose}
            className="flex-1 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={saving}
            className="flex-1 rounded-lg bg-green-surface border border-green-signal/30 px-3 py-2 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {saving ? "Submitting…" : "Confirm"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────
export default function PayoutsPage() {
  const [wallet,       setWallet]       = useState<Wallet | null>(null);
  const [transactions, setTransactions] = useState<WalletTransaction[]>([]);
  const [invoices,     setInvoices]     = useState<Invoice[]>([]);
  const [loading,      setLoading]      = useState(true);
  const [error,        setError]        = useState<string | null>(null);
  const [showWithdraw, setShowWithdraw] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [w, txs, invs] = await Promise.all([
        paymentsApi.getWallet(),
        paymentsApi.getTransactions(),
        paymentsApi.getInvoices(),
      ]);
      setWallet(w);
      setTransactions(txs);
      setInvoices(invs);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load payout data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // Refresh when driver roster flips — commission accrual moves wallet balance.
  useRosterEvents((event) => {
    if (event.type === "status_changed") load();
  });

  const pendingTotal = invoices
    .filter(i => i.status === "issued" || i.status === "overdue")
    .reduce((s, i) => s + i.total_php, 0);

  const paidMtd = invoices
    .filter(i => {
      if (i.status !== "paid" || !i.paid_at) return false;
      const d = new Date(i.paid_at);
      const now = new Date();
      return d.getMonth() === now.getMonth() && d.getFullYear() === now.getFullYear();
    })
    .reduce((s, i) => s + i.total_php, 0);

  // Derive monthly bar chart from live invoices; fall back to static seed data.
  const monthlyChart = (() => {
    const paid = invoices.filter(i => i.status === "paid" && i.paid_at);
    if (paid.length === 0) return MONTHLY_PAYOUTS;
    const byMonth: Record<string, number> = {};
    paid.forEach(inv => {
      const key = new Date(inv.paid_at!).toLocaleString("en", { month: "short" });
      byMonth[key] = (byMonth[key] ?? 0) + inv.total_php;
    });
    return Object.entries(byMonth).slice(-6).map(([month, base]) => ({ month, base, cod: 0, bonus: 0 }));
  })();

  return (
    <>
      {showWithdraw && wallet && (
        <WithdrawModal
          wallet={wallet}
          onClose={() => setShowWithdraw(false)}
          onSuccess={(updated) => {
            setWallet(updated);
            setShowWithdraw(false);
          }}
        />
      )}

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
            <p className="text-sm text-white/40 font-mono mt-0.5">Payout schedule: 5th of each month</p>
          </div>
          <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={12} /> Export CSV
          </button>
        </motion.div>

        {error && (
          <motion.div variants={variants.fadeInUp}>
            <GlassCard padding="sm">
              <p className="text-xs text-red-signal font-mono">{error}</p>
            </GlassCard>
          </motion.div>
        )}

        {/* KPI row */}
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <GlassCard size="sm" glow="green" accent>
            <LiveMetric label="Wallet Balance" value={wallet?.balance_php ?? 0} trend={0} color="green" format="currency" />
          </GlassCard>
          <GlassCard size="sm" glow="amber" accent>
            <LiveMetric label="Pending Invoices" value={pendingTotal} trend={0} color="amber" format="currency" />
          </GlassCard>
          <GlassCard size="sm" glow="cyan" accent>
            <LiveMetric label="Paid MTD" value={paidMtd} trend={0} color="cyan" format="currency" />
          </GlassCard>
        </motion.div>

        {/* Wallet card */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="green">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Available Balance</p>
                <p className="font-heading text-4xl font-bold text-green-signal">
                  ₱{(wallet?.available_php ?? 0).toLocaleString()}
                </p>
                {wallet && wallet.reserved_php > 0 && (
                  <p className="text-xs font-mono text-white/30 mt-1">
                    ₱{wallet.reserved_php.toLocaleString()} reserved
                  </p>
                )}
              </div>
              <button
                onClick={() => setShowWithdraw(true)}
                disabled={!wallet || wallet.available_php <= 0}
                className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-4 py-2 text-xs font-medium text-green-signal hover:border-green-signal/60 hover:shadow-[0_0_10px_rgba(0,255,136,0.2)] transition-all disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Request Withdrawal
              </button>
            </div>
          </GlassCard>
        </motion.div>

        {/* Payout trend chart (live data from invoices, fallback to seed) */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="green">
            <div className="flex items-center justify-between mb-5">
              <div>
                <h2 className="font-heading text-sm font-semibold text-white">Monthly Payout Breakdown</h2>
                <p className="text-2xs font-mono text-white/30">Base · COD Remittance · Bonus</p>
              </div>
            </div>
            <ResponsiveContainer width="100%" height={200}>
              <BarChart data={monthlyChart} margin={{ top: 0, right: 0, bottom: 0, left: -24 }}>
                <CartesianGrid stroke="rgba(255,255,255,0.04)" strokeDasharray="4 4" vertical={false} />
                <XAxis dataKey="month" tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 11, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <YAxis tick={{ fill: "rgba(255,255,255,0.3)", fontSize: 10, fontFamily: "JetBrains Mono" }} axisLine={false} tickLine={false} />
                <Tooltip
                  contentStyle={{ background: "rgba(13,20,34,0.95)", border: "1px solid rgba(255,255,255,0.08)", borderRadius: 8, fontFamily: "JetBrains Mono", fontSize: 11 }}
                  formatter={(v) => [`₱${Number(v).toLocaleString()}`, ""]}
                />
                <Bar dataKey="base"  fill="#00FF88" radius={[0,0,0,0]} fillOpacity={0.85} stackId="a" />
                <Bar dataKey="cod"   fill="#00E5FF" radius={[0,0,0,0]} fillOpacity={0.7}  stackId="a" />
                <Bar dataKey="bonus" fill="#A855F7" radius={[4,4,0,0]} fillOpacity={0.8}  stackId="a" />
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

        {/* Transactions */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none">
            <div className="px-5 py-4 border-b border-glass-border">
              <h2 className="font-heading text-sm font-semibold text-white">Recent Transactions</h2>
            </div>
            {loading ? (
              <div className="py-6 text-center">
                <p className="text-xs text-white/30 font-mono">loading…</p>
              </div>
            ) : transactions.length === 0 ? (
              <div className="py-6 text-center">
                <p className="text-xs text-white/30 font-mono">No transactions yet.</p>
              </div>
            ) : (
              transactions.map((tx) => (
                <div
                  key={tx.id}
                  className="flex items-center justify-between px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
                >
                  <div>
                    <p className="text-xs text-white font-mono">{tx.description}</p>
                    <p className="text-2xs text-white/30 font-mono mt-0.5">
                      {new Date(tx.created_at).toLocaleDateString()}
                    </p>
                  </div>
                  <span className={`text-sm font-bold font-mono ${tx.type === "credit" ? "text-green-signal" : "text-red-signal"}`}>
                    {tx.type === "credit" ? "+" : "-"}₱{tx.amount_php.toLocaleString()}
                  </span>
                </div>
              ))
            )}
          </GlassCard>
        </motion.div>

        {/* Invoice history */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none">
            <div className="px-5 py-4 border-b border-glass-border">
              <h2 className="font-heading text-sm font-semibold text-white">Invoice History</h2>
            </div>
            {loading ? (
              <div className="py-10 text-center">
                <p className="text-xs text-white/30 font-mono">loading…</p>
              </div>
            ) : invoices.length === 0 ? (
              <div className="py-10 text-center">
                <p className="text-xs text-white/30 font-mono">No invoices yet.</p>
              </div>
            ) : (
              <>
                <div className="grid grid-cols-[1fr_120px_100px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
                  {["Invoice", "Period", "Total", "Status"].map((h) => (
                    <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
                  ))}
                </div>
                {invoices.map((inv) => {
                  const cfg = STATUS_VARIANT[inv.status] ?? { label: inv.status, variant: "muted" as const };
                  return (
                    <div key={inv.id} className="grid grid-cols-[1fr_120px_100px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                      <div>
                        <p className="text-xs font-mono text-cyan-neon">{inv.invoice_number}</p>
                        <p className="text-2xs font-mono text-white/30 mt-0.5">
                          Due {new Date(inv.due_date).toLocaleDateString()}
                        </p>
                      </div>
                      <span className="text-xs font-mono text-white/60">
                        {new Date(inv.period_from).toLocaleDateString()} – {new Date(inv.period_to).toLocaleDateString()}
                      </span>
                      <span className="text-sm font-bold font-heading text-green-signal">
                        ₱{inv.total_php.toLocaleString()}
                      </span>
                      <NeonBadge variant={cfg.variant}>{cfg.label}</NeonBadge>
                    </div>
                  );
                })}
              </>
            )}
          </GlassCard>
        </motion.div>
      </motion.div>
    </>
  );
}
