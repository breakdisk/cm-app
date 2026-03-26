"use client";
/**
 * Merchant Portal — Billing Page
 * Invoice history, COD remittance, wallet balance, subscription tier.
 */
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Receipt, Download, CreditCard, Wallet, CheckCircle2, Clock, AlertCircle } from "lucide-react";

// ── Mock data ──────────────────────────────────────────────────────────────────

const KPI = [
  { label: "Balance Due",     value: 28400,  trend: 0,    color: "red"    as const, format: "currency" as const },
  { label: "COD Pending",     value: 142000, trend: -8.4, color: "amber"  as const, format: "currency" as const },
  { label: "Paid MTD",        value: 84200,  trend: +14.2, color: "green" as const, format: "currency" as const },
  { label: "Shipments Billed", value: 1284,  trend: +8.1, color: "cyan"   as const, format: "number"  as const },
];

type InvoiceStatus = "paid" | "unpaid" | "overdue" | "processing";

interface Invoice {
  id: string;
  period: string;
  shipments: number;
  base_charges: number;
  cod_fee: number;
  total: number;
  status: InvoiceStatus;
  due_date: string;
  paid_date?: string;
}

const INVOICES: Invoice[] = [
  { id: "INV-2026-0317", period: "March 2026 (MTD)",  shipments: 1284, base_charges: 19260, cod_fee: 9120,  total: 28380, status: "unpaid",    due_date: "Apr 5, 2026" },
  { id: "INV-2026-0228", period: "February 2026",      shipments: 1142, base_charges: 17130, cod_fee: 8210,  total: 25340, status: "paid",      due_date: "Mar 5, 2026", paid_date: "Mar 3, 2026" },
  { id: "INV-2026-0131", period: "January 2026",       shipments: 984,  base_charges: 14760, cod_fee: 6840,  total: 21600, status: "paid",      due_date: "Feb 5, 2026", paid_date: "Feb 4, 2026" },
  { id: "INV-2025-1231", period: "December 2025",      shipments: 1840, base_charges: 27600, cod_fee: 14200, total: 41800, status: "paid",      due_date: "Jan 5, 2026", paid_date: "Jan 5, 2026" },
  { id: "INV-2025-1130", period: "November 2025",      shipments: 920,  base_charges: 13800, cod_fee: 6420,  total: 20220, status: "paid",      due_date: "Dec 5, 2025", paid_date: "Dec 4, 2025" },
];

const STATUS_CONFIG: Record<InvoiceStatus, { label: string; variant: "green" | "cyan" | "amber" | "red"; icon: React.ReactNode }> = {
  paid:       { label: "Paid",       variant: "green", icon: <CheckCircle2 size={10} /> },
  unpaid:     { label: "Unpaid",     variant: "amber", icon: <Clock size={10} />        },
  overdue:    { label: "Overdue",    variant: "red",   icon: <AlertCircle size={10} /> },
  processing: { label: "Processing", variant: "cyan",  icon: <Clock size={10} />        },
};

const PRICING_TIERS = [
  { label: "Base Rate",      value: "₱15.00 / shipment", note: "Metro Manila"          },
  { label: "Provincial",     value: "₱22.00 / shipment", note: "Luzon provinces"       },
  { label: "Island Shipping", value: "₱38.00 / shipment", note: "Visayas / Mindanao"   },
  { label: "COD Fee",        value: "1.5%",               note: "of COD amount"         },
  { label: "Fuel Surcharge", value: "₱2.50 / shipment",  note: "Current rate Mar 2026" },
];

export default function BillingPage() {
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
          <button className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors">
            <Download size={12} /> Download All
          </button>
          <button className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-amber-signal to-red-signal px-4 py-2 text-xs font-semibold text-canvas">
            <CreditCard size={12} /> Pay Now
          </button>
        </div>
      </motion.div>

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {KPI.map((m) => (
          <GlassCard key={m.label} size="sm" glow={m.color} accent>
            <LiveMetric label={m.label} value={m.value} trend={m.trend} color={m.color} format={m.format} />
          </GlassCard>
        ))}
      </motion.div>

      {/* Current balance + plan */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        {/* Balance card */}
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard glow="amber" className="h-full">
            <div className="flex items-start justify-between mb-4">
              <div>
                <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Outstanding Balance</p>
                <p className="font-heading text-4xl font-bold text-amber-signal">₱28,380</p>
                <p className="text-xs font-mono text-white/40 mt-1">Due: April 5, 2026 · Invoice INV-2026-0317</p>
              </div>
              <NeonBadge variant="amber" dot>Unpaid</NeonBadge>
            </div>
            <div className="grid grid-cols-3 gap-3">
              {[
                { label: "Base Charges", value: "₱19,260", color: "text-white"        },
                { label: "COD Fee",      value: "₱9,120",  color: "text-amber-signal" },
                { label: "Fuel Surcharge", value: "₱0",    color: "text-white/40"     },
              ].map((item) => (
                <div key={item.label} className="rounded-lg bg-glass-100 px-3 py-2.5">
                  <p className="text-2xs font-mono text-white/30">{item.label}</p>
                  <p className={`text-sm font-bold font-mono mt-0.5 ${item.color}`}>{item.value}</p>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* Plan card */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard glow="purple" className="h-full">
            <div className="flex items-center justify-between mb-4">
              <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">Current Plan</p>
              <NeonBadge variant="purple">Business</NeonBadge>
            </div>
            <div className="flex flex-col gap-2">
              {PRICING_TIERS.map((t) => (
                <div key={t.label} className="flex items-center justify-between">
                  <div>
                    <p className="text-xs text-white/70">{t.label}</p>
                    <p className="text-2xs font-mono text-white/30">{t.note}</p>
                  </div>
                  <span className="text-xs font-mono font-bold text-cyan-neon">{t.value}</span>
                </div>
              ))}
            </div>
            <button className="mt-4 w-full rounded-lg border border-purple-plasma/30 bg-purple-surface py-2 text-xs font-medium text-purple-plasma hover:bg-purple-plasma/10 transition-colors">
              Upgrade to Enterprise
            </button>
          </GlassCard>
        </motion.div>
      </div>

      {/* Invoice history */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Invoice History</h2>
          </div>

          <div className="grid grid-cols-[1fr_80px_100px_80px_100px_100px_80px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Invoice", "Shipments", "Base", "COD Fee", "Total", "Due Date", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {INVOICES.map((inv) => {
            const { label, variant, icon } = STATUS_CONFIG[inv.status];
            return (
              <div key={inv.id} className="grid grid-cols-[1fr_80px_100px_80px_100px_100px_80px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors">
                <div>
                  <p className="text-xs font-mono text-cyan-neon">{inv.id}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{inv.period}</p>
                </div>
                <span className="text-xs font-mono text-white/60">{inv.shipments.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">₱{inv.base_charges.toLocaleString()}</span>
                <span className="text-xs font-mono text-amber-signal">₱{inv.cod_fee.toLocaleString()}</span>
                <span className="text-sm font-bold font-heading text-white">₱{inv.total.toLocaleString()}</span>
                <div>
                  <p className="text-xs font-mono text-white/50">{inv.due_date}</p>
                  {inv.paid_date && <p className="text-2xs font-mono text-white/25 mt-0.5">Paid {inv.paid_date}</p>}
                </div>
                <div className="flex items-center gap-1">
                  <NeonBadge variant={variant}>
                    <span className="flex items-center gap-1">{icon}{label}</span>
                  </NeonBadge>
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
