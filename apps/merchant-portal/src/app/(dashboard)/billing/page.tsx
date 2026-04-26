"use client";
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Receipt, Download, CreditCard, RefreshCw } from "lucide-react";
import { billingApi, type Invoice, type Wallet } from "@/lib/api/billing";

type FilterTab = "all" | "issued" | "paid" | "overdue";

const STATUS_VARIANT: Record<string, "green" | "amber" | "red" | "cyan" | "muted"> = {
  paid:      "green",
  issued:    "amber",
  overdue:   "red",
  draft:     "cyan",
  cancelled: "muted",
};

const PRICING_TIERS = [
  { label: "Base Rate",       value: "₱15.00 / shipment", note: "Metro Manila"          },
  { label: "Provincial",      value: "₱22.00 / shipment", note: "Luzon provinces"       },
  { label: "Island Shipping", value: "₱38.00 / shipment", note: "Visayas / Mindanao"    },
  { label: "COD Fee",         value: "1.5%",               note: "of COD amount"         },
  { label: "Fuel Surcharge",  value: "₱2.50 / shipment",  note: "Current rate Apr 2026" },
];

export default function BillingPage() {
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [wallet,   setWallet]   = useState<Wallet | null>(null);
  const [loading,  setLoading]  = useState(true);
  const [error,    setError]    = useState<string | null>(null);
  const [tab,      setTab]      = useState<FilterTab>("all");

  const load = useCallback(async () => {
    setError(null);
    try {
      const [invRes, walletData] = await Promise.all([
        billingApi.listInvoices({}, ""),
        billingApi.getWallet(""),
      ]);
      setInvoices(invRes.data ?? []);
      setWallet(walletData);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load billing data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const outstanding = invoices
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

  const displayed = tab === "all"
    ? invoices
    : invoices.filter(i => i.status === tab);

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
            <Receipt size={22} className="text-amber-signal" />
            Billing
          </h1>
          <p className="text-sm text-white/40 font-mono mt-0.5">Plan: Business · Billing cycle: Monthly</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
          >
            <RefreshCw size={12} /> Refresh
          </button>
          <button className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-amber-signal to-red-signal px-4 py-2 text-xs font-semibold text-canvas">
            <CreditCard size={12} /> Pay Now
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <GlassCard size="sm" glow="red" accent>
          <LiveMetric
            label="Outstanding Balance"
            value={outstanding}
            trend={0}
            color="red"
            format="currency"
          />
        </GlassCard>
        <GlassCard size="sm" glow="green" accent>
          <LiveMetric
            label="Wallet Balance"
            value={wallet?.balance_php ?? 0}
            trend={0}
            color="green"
            format="currency"
          />
        </GlassCard>
        <GlassCard size="sm" glow="cyan" accent>
          <LiveMetric
            label="Paid MTD"
            value={paidMtd}
            trend={0}
            color="cyan"
            format="currency"
          />
        </GlassCard>
      </motion.div>

      {/* Plan card */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard glow="purple">
          <div className="flex items-center justify-between mb-4">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">Current Plan</p>
            <NeonBadge variant="purple">Business</NeonBadge>
          </div>
          <div className="grid grid-cols-2 gap-x-8 gap-y-2 sm:grid-cols-5">
            {PRICING_TIERS.map((t) => (
              <div key={t.label} className="flex flex-col">
                <p className="text-xs text-white/70">{t.label}</p>
                <p className="text-2xs font-mono text-white/30">{t.note}</p>
                <span className="text-xs font-mono font-bold text-cyan-neon mt-0.5">{t.value}</span>
              </div>
            ))}
          </div>
        </GlassCard>
      </motion.div>

      {/* Invoice history */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Invoice History</h2>
            <div className="flex gap-1">
              {(["all", "issued", "paid", "overdue"] as FilterTab[]).map((t) => (
                <button
                  key={t}
                  onClick={() => setTab(t)}
                  className={`px-2.5 py-1 rounded text-2xs font-mono capitalize transition-colors ${
                    tab === t
                      ? "bg-cyan-neon/10 text-cyan-neon border border-cyan-neon/30"
                      : "text-white/40 hover:text-white/60"
                  }`}
                >
                  {t}
                </button>
              ))}
            </div>
          </div>

          {loading ? (
            <div className="py-10 text-center">
              <p className="text-xs text-white/30 font-mono">loading invoices…</p>
            </div>
          ) : displayed.length === 0 ? (
            <div className="py-10 text-center">
              <p className="text-xs text-white/30 font-mono">No invoices found.</p>
            </div>
          ) : (
            <>
              <div className="grid grid-cols-[1fr_120px_100px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
                {["Invoice", "Period", "Total", "Status"].map((h) => (
                  <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
                ))}
              </div>
              {displayed.map((inv) => (
                <div
                  key={inv.id}
                  className="grid grid-cols-[1fr_120px_100px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
                >
                  <div>
                    <p className="text-xs font-mono text-cyan-neon">{inv.invoice_number}</p>
                    <p className="text-2xs font-mono text-white/30 mt-0.5">
                      Due {new Date(inv.due_date).toLocaleDateString()}
                    </p>
                  </div>
                  <span className="text-xs font-mono text-white/60">
                    {new Date(inv.period_from).toLocaleDateString()} –{" "}
                    {new Date(inv.period_to).toLocaleDateString()}
                  </span>
                  <span className="text-sm font-bold font-heading text-white">
                    ₱{inv.total_php.toLocaleString()}
                  </span>
                  <NeonBadge variant={STATUS_VARIANT[inv.status] ?? "muted"}>
                    {inv.status}
                  </NeonBadge>
                </div>
              ))}
            </>
          )}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
