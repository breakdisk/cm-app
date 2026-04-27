"use client";
/**
 * Merchant Portal — Billing Page
 * Invoice history from `GET /v1/invoices` (payments service).
 * Wallet balance, COD remittance, and subscription tier.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import {
  Receipt, RefreshCw, Send, CheckCircle2, Clock, AlertCircle, FileText,
} from "lucide-react";
import { authFetch } from "@/lib/auth/auth-fetch";

const PAYMENTS_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// Backend `InvoiceSummary` shape (see services/payments/src/application/commands).
interface InvoiceSummary {
  invoice_id:     string;
  invoice_number: string;
  invoice_type:   string;   // "shipmentcharges" | "paymentreceipt" | ...
  status:         string;   // "draft" | "issued" | "paid" | "overdue" | "disputed" | "cancelled"
  awb_count:      number;
  subtotal_cents: number;
  vat_cents:      number;
  total_cents:    number;
  billing_period: string;   // "YYYY-MM"
  due_at:         string;   // ISO8601
  issued_at:      string;   // ISO8601
}

type StatusBadge = {
  label:   string;
  variant: "green" | "cyan" | "amber" | "red" | "purple";
  icon:    React.ReactNode;
};

function statusBadge(status: string): StatusBadge {
  switch (status) {
    case "paid":      return { label: "Paid",     variant: "green",  icon: <CheckCircle2 size={10} /> };
    case "issued":    return { label: "Unpaid",   variant: "amber",  icon: <Clock size={10} />        };
    case "overdue":   return { label: "Overdue",  variant: "red",    icon: <AlertCircle size={10} /> };
    case "draft":     return { label: "Draft",    variant: "cyan",   icon: <FileText size={10} />    };
    case "disputed":  return { label: "Disputed", variant: "purple", icon: <AlertCircle size={10} /> };
    case "cancelled": return { label: "Void",     variant: "red",    icon: <AlertCircle size={10} /> };
    default:          return { label: status,     variant: "cyan",   icon: <Clock size={10} />        };
  }
}

function formatPeso(cents: number): string {
  return `₱${Math.round(cents / 100).toLocaleString()}`;
}

function formatBillingPeriod(period: string): string {
  // "2026-04" → "April 2026"
  const [y, m] = period.split("-").map(Number);
  if (!y || !m) return period;
  const date = new Date(Date.UTC(y, m - 1, 1));
  return date.toLocaleString("en-US", { year: "numeric", month: "long", timeZone: "UTC" });
}

function formatDate(iso: string): string {
  try { return new Date(iso).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric" }); }
  catch { return iso; }
}

const PRICING_TIERS = [
  { label: "Base Rate",       value: "₱15.00 / shipment",  note: "Metro Manila"          },
  { label: "Provincial",      value: "₱22.00 / shipment",  note: "Luzon provinces"       },
  { label: "Island Shipping", value: "₱38.00 / shipment",  note: "Visayas / Mindanao"    },
  { label: "COD Fee",         value: "1.5%",               note: "of COD amount"         },
  { label: "Fuel Surcharge",  value: "₱2.50 / shipment",   note: "Current rate"          },
];

export default function BillingPage() {
  const [invoices,  setInvoices]  = useState<InvoiceSummary[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [resendingId, setResendingId] = useState<string | null>(null);
  const [resendMessage, setResendMessage] = useState<string | null>(null);

  const fetchInvoices = useCallback(async () => {
    setIsLoading(true);
    setLoadError(null);
    try {
      const res = await authFetch(`${PAYMENTS_URL}/v1/invoices`);
      if (!res.ok) {
        const body = await res.text().catch(() => "");
        setLoadError(`Failed to load invoices (HTTP ${res.status})${body ? `: ${body.slice(0, 200)}` : ""}`);
        setInvoices([]);
        return;
      }
      const json = await res.json();
      setInvoices(Array.isArray(json.data) ? json.data : []);
    } catch (e) {
      setLoadError(e instanceof Error ? e.message : "Network error");
      setInvoices([]);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => { fetchInvoices(); }, [fetchInvoices]);

  const resendInvoice = useCallback(async (invoiceId: string) => {
    setResendingId(invoiceId);
    setResendMessage(null);
    try {
      const res = await authFetch(`${PAYMENTS_URL}/v1/invoices/${invoiceId}/resend`, { method: "POST" });
      if (!res.ok) {
        const body = await res.text().catch(() => "");
        setResendMessage(`Resend failed (HTTP ${res.status})${body ? `: ${body.slice(0, 120)}` : ""}`);
      } else {
        setResendMessage("Receipt re-sent — check your inbox.");
      }
    } catch (e) {
      setResendMessage(e instanceof Error ? e.message : "Resend failed");
    } finally {
      setResendingId(null);
      setTimeout(() => setResendMessage(null), 4000);
    }
  }, []);

  // ── Derived KPIs from real invoices ───────────────────────────────────────
  const kpi = useMemo(() => {
    const now = new Date();
    const thisMonthKey = `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;

    const outstanding    = invoices.filter((i) => i.status === "issued" || i.status === "overdue");
    const paidThisMonth  = invoices.filter((i) => i.status === "paid" && i.billing_period === thisMonthKey);
    const balanceDueCents = outstanding.reduce((acc, i) => acc + i.total_cents, 0);
    const paidMtdCents    = paidThisMonth.reduce((acc, i) => acc + i.total_cents, 0);
    const shipmentsBilled = invoices.reduce((acc, i) => acc + (i.awb_count ?? 0), 0);

    return {
      balanceDue:    Math.round(balanceDueCents / 100),
      paidMtd:       Math.round(paidMtdCents / 100),
      shipmentsBilled,
      outstandingCount: outstanding.length,
    };
  }, [invoices]);

  const primaryOutstanding = useMemo(
    () => invoices.find((i) => i.status === "issued" || i.status === "overdue"),
    [invoices],
  );

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
        <button
          onClick={fetchInvoices}
          disabled={isLoading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={isLoading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {resendMessage && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm">
            <p className="text-xs font-mono text-cyan-neon">{resendMessage}</p>
          </GlassCard>
        </motion.div>
      )}

      {loadError && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard size="sm" glow="red">
            <p className="text-xs font-mono text-red-signal">{loadError}</p>
          </GlassCard>
        </motion.div>
      )}

      {/* KPI row */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        <GlassCard size="sm" glow="red" accent>
          <LiveMetric label="Balance Due"       value={kpi.balanceDue}       color="red"   format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="amber" accent>
          <LiveMetric label="Outstanding"       value={kpi.outstandingCount} color="amber" format="number" />
        </GlassCard>
        <GlassCard size="sm" glow="green" accent>
          <LiveMetric label="Paid MTD"          value={kpi.paidMtd}          color="green" format="currency" />
        </GlassCard>
        <GlassCard size="sm" glow="cyan" accent>
          <LiveMetric label="Shipments Billed"  value={kpi.shipmentsBilled}  color="cyan"  format="number" />
        </GlassCard>
      </motion.div>

      {/* Current balance + plan */}
      <div className="grid grid-cols-1 gap-4 lg:grid-cols-3">
        <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
          <GlassCard glow={primaryOutstanding ? "amber" : "green"} className="h-full">
            <div className="flex items-start justify-between mb-4">
              <div>
                <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">
                  {primaryOutstanding ? "Outstanding Balance" : "All Invoices Settled"}
                </p>
                <p className={`font-heading text-4xl font-bold ${primaryOutstanding ? "text-amber-signal" : "text-green-signal"}`}>
                  {primaryOutstanding ? formatPeso(primaryOutstanding.total_cents) : "₱0"}
                </p>
                {primaryOutstanding ? (
                  <p className="text-xs font-mono text-white/40 mt-1">
                    Due {formatDate(primaryOutstanding.due_at)} · Invoice {primaryOutstanding.invoice_number}
                  </p>
                ) : (
                  <p className="text-xs font-mono text-white/40 mt-1">
                    Nothing to pay right now.
                  </p>
                )}
              </div>
              {primaryOutstanding && <NeonBadge variant={statusBadge(primaryOutstanding.status).variant} dot>{statusBadge(primaryOutstanding.status).label}</NeonBadge>}
            </div>
            {primaryOutstanding && (
              <div className="grid grid-cols-3 gap-3">
                <div className="rounded-lg bg-glass-100 px-3 py-2.5">
                  <p className="text-2xs font-mono text-white/30">Subtotal</p>
                  <p className="text-sm font-bold font-mono mt-0.5 text-white">{formatPeso(primaryOutstanding.subtotal_cents)}</p>
                </div>
                <div className="rounded-lg bg-glass-100 px-3 py-2.5">
                  <p className="text-2xs font-mono text-white/30">VAT 12%</p>
                  <p className="text-sm font-bold font-mono mt-0.5 text-amber-signal">{formatPeso(primaryOutstanding.vat_cents)}</p>
                </div>
                <div className="rounded-lg bg-glass-100 px-3 py-2.5">
                  <p className="text-2xs font-mono text-white/30">AWBs</p>
                  <p className="text-sm font-bold font-mono mt-0.5 text-cyan-neon">{primaryOutstanding.awb_count.toLocaleString()}</p>
                </div>
              </div>
            )}
          </GlassCard>
        </motion.div>

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
          </GlassCard>
        </motion.div>
      </div>

      {/* Invoice history */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white">Invoice History</h2>
            <span className="text-2xs font-mono text-white/30">{invoices.length} invoice{invoices.length === 1 ? "" : "s"}</span>
          </div>

          <div className="grid grid-cols-[1.5fr_90px_100px_90px_100px_100px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
            {["Invoice", "AWBs", "Subtotal", "VAT", "Total", "Due Date", "Status"].map((h) => (
              <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
            ))}
          </div>

          {isLoading && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">Loading invoices…</div>
          )}

          {!isLoading && !loadError && invoices.length === 0 && (
            <div className="px-5 py-12 text-center text-xs font-mono text-white/40">
              No invoices yet. Your first billing run will appear here.
            </div>
          )}

          {!isLoading && invoices.map((inv) => {
            const badge = statusBadge(inv.status);
            const canResend = inv.status !== "draft" && inv.status !== "cancelled";
            return (
              <div
                key={inv.invoice_id}
                className="grid grid-cols-[1.5fr_90px_100px_90px_100px_100px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors"
              >
                <div>
                  <p className="text-xs font-mono text-cyan-neon">{inv.invoice_number}</p>
                  <p className="text-2xs font-mono text-white/30 mt-0.5">{formatBillingPeriod(inv.billing_period)}</p>
                </div>
                <span className="text-xs font-mono text-white/60">{inv.awb_count.toLocaleString()}</span>
                <span className="text-xs font-mono text-white/60">{formatPeso(inv.subtotal_cents)}</span>
                <span className="text-xs font-mono text-amber-signal">{formatPeso(inv.vat_cents)}</span>
                <span className="text-sm font-bold font-heading text-white">{formatPeso(inv.total_cents)}</span>
                <span className="text-xs font-mono text-white/50">{formatDate(inv.due_at)}</span>
                <div className="flex items-center gap-2">
                  <NeonBadge variant={badge.variant}>
                    <span className="flex items-center gap-1">{badge.icon}{badge.label}</span>
                  </NeonBadge>
                  {canResend && (
                    <button
                      onClick={() => resendInvoice(inv.invoice_id)}
                      disabled={resendingId === inv.invoice_id}
                      title="Re-send invoice email"
                      className="p-1 rounded text-white/40 hover:text-cyan-neon disabled:opacity-50"
                    >
                      <Send size={12} className={resendingId === inv.invoice_id ? "animate-pulse" : ""} />
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </GlassCard>
      </motion.div>
    </motion.div>
  );
}
