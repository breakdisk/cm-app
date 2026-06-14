"use client";
/**
 * Merchant Portal — Billing Page
 * Invoice history, COD remittance panel, wallet balance, and pricing plan.
 * Backed by live billingApi.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { LiveMetric } from "@/components/ui/live-metric";
import { Receipt, CreditCard, RefreshCw, X, Building2, Package, Download, Send } from "lucide-react";
import {
  billingApi,
  type Invoice,
  type Wallet,
  type CodBalance,
  type CodRemittance,
  type InvoiceLineItem,
} from "@/lib/api/billing";

// ── Types ─────────────────────────────────────────────────────────────────────

type FilterTab = "all" | "issued" | "paid" | "overdue";

const STATUS_VARIANT: Record<string, "green" | "amber" | "red" | "cyan" | "muted"> = {
  paid:      "green",
  issued:    "amber",
  overdue:   "red",
  draft:     "cyan",
  cancelled: "muted",
};

const COD_STATUS_VARIANT: Record<CodRemittance["status"], "green" | "amber" | "red" | "cyan" | "muted"> = {
  completed:  "green",
  pending:    "amber",
  processing: "cyan",
  failed:     "red",
};

const PRICING_TIERS = [
  { label: "Base Rate",       value: "₱15.00 / shipment", note: "Metro Manila"          },
  { label: "Provincial",      value: "₱22.00 / shipment", note: "Luzon provinces"       },
  { label: "Island Shipping", value: "₱38.00 / shipment", note: "Visayas / Mindanao"    },
  { label: "COD Fee",         value: "1.5%",               note: "of COD amount"         },
  { label: "Fuel Surcharge",  value: "₱2.50 / shipment",  note: "Current rate Apr 2026" },
];

// ── Pay-by-bank modal ─────────────────────────────────────────────────────────

function BankTransferModal({ outstanding, onClose }: { outstanding: number; onClose: () => void }) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="w-full max-w-md rounded-xl border border-glass-border bg-[#0A0E1A] p-6 shadow-xl">
        <div className="flex items-center justify-between mb-5">
          <div className="flex items-center gap-2">
            <Building2 size={16} className="text-amber-signal" />
            <h3 className="font-heading text-sm font-semibold text-white">Pay via Bank Transfer</h3>
          </div>
          <button onClick={onClose} className="text-white/40 hover:text-white transition-colors">
            <X size={16} />
          </button>
        </div>

        <div className="rounded-lg border border-glass-border bg-glass-100 p-4 mb-4 space-y-2">
          {[
            ["Bank",           "BDO Unibank"],
            ["Account Name",   "LogisticOS Inc."],
            ["Account Number", "0044 8888 7777"],
            ["Branch",         "Makati City Main"],
          ].map(([label, value]) => (
            <div key={label} className="flex justify-between">
              <span className="text-2xs font-mono text-white/40">{label}</span>
              <span className="text-xs font-mono text-white">{value}</span>
            </div>
          ))}
        </div>

        <p className="text-xs font-mono text-white/50 mb-3">
          Outstanding balance: <span className="text-amber-signal font-bold">₱{outstanding.toLocaleString()}</span>
        </p>

        <p className="text-2xs text-white/30 font-mono leading-relaxed">
          After transfer, email your deposit slip to{" "}
          <span className="text-cyan-neon">billing@logisticos.ph</span> with your invoice number(s) in the subject line.
          Payment is applied within 1–2 business days.
        </p>

        <button
          onClick={onClose}
          className="mt-5 w-full rounded-lg border border-glass-border px-4 py-2 text-xs text-white/60 hover:text-white transition-colors"
        >
          Close
        </button>
      </div>
    </div>
  );
}

// ── Invoice detail modal ──────────────────────────────────────────────────────

function InvoiceDetailModal({ invoice, onClose }: { invoice: Invoice; onClose: () => void }) {
  const [detail,      setDetail]      = useState<Invoice | null>(null);
  const [loading,     setLoading]     = useState(true);
  const [downloading, setDownloading] = useState(false);
  const [resending,   setResending]   = useState(false);
  const [resendDone,  setResendDone]  = useState(false);

  useEffect(() => {
    billingApi.getInvoice(invoice.id, "")
      .then(setDetail)
      .catch(() => setDetail(invoice))  // fallback to summary
      .finally(() => setLoading(false));
  }, [invoice]);

  const handleDownload = async () => {
    if (downloading) return;
    setDownloading(true);
    try {
      const blob = await billingApi.downloadInvoicePdf(invoice.id, "");
      const url  = URL.createObjectURL(blob);
      const a    = document.createElement("a");
      a.href     = url;
      a.download = `invoice-${invoice.invoice_number}.pdf`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      // non-fatal; PDF may not be generated yet
    } finally {
      setDownloading(false);
    }
  };

  const handleResend = async () => {
    if (resending || resendDone) return;
    setResending(true);
    try {
      await billingApi.resendInvoice(invoice.id, "");
      setResendDone(true);
    } catch {
      // non-fatal
    } finally {
      setResending(false);
    }
  };

  const inv = detail ?? invoice;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-lg rounded-xl border border-glass-border bg-[#0A0E1A] shadow-xl max-h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border shrink-0">
          <div>
            <p className="text-xs font-mono text-cyan-neon">{inv.invoice_number}</p>
            <p className="text-2xs font-mono text-white/30 mt-0.5">
              {new Date(inv.period_from).toLocaleDateString()} – {new Date(inv.period_to).toLocaleDateString()}
            </p>
          </div>
          <div className="flex items-center gap-3">
            <NeonBadge variant={STATUS_VARIANT[inv.status] ?? "muted"}>{inv.status}</NeonBadge>
            <button onClick={onClose} className="text-white/40 hover:text-white transition-colors">
              <X size={16} />
            </button>
          </div>
        </div>

        {/* Body */}
        <div className="overflow-y-auto flex-1 px-5 py-4 space-y-4">
          {loading ? (
            <p className="text-xs text-white/30 font-mono text-center py-6">Loading line items…</p>
          ) : (inv.line_items ?? []).length === 0 ? (
            <p className="text-xs text-white/30 font-mono text-center py-6">No line items available.</p>
          ) : (
            <div className="space-y-1">
              <div className="grid grid-cols-[1fr_60px_80px] gap-2 pb-2 border-b border-glass-border">
                {["Description", "Qty", "Amount"].map(h => (
                  <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
                ))}
              </div>
              {(inv.line_items ?? []).map((li: InvoiceLineItem, i: number) => (
                <div key={i} className="grid grid-cols-[1fr_60px_80px] gap-2 items-center py-1.5">
                  <span className="text-xs font-mono text-white/70">{li.description}</span>
                  <span className="text-xs font-mono text-white/40 text-right">{li.quantity}</span>
                  <span className="text-xs font-mono text-white text-right">
                    ₱{li.total_php.toLocaleString()}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer totals */}
        <div className="border-t border-glass-border px-5 py-4 shrink-0 space-y-1.5">
          {[
            ["Subtotal",  `₱${(inv.subtotal_php ?? inv.total_php * 0.88).toLocaleString()}`],
            ["VAT (12%)", `₱${(inv.tax_php ?? inv.total_php * 0.12).toLocaleString()}`],
          ].map(([label, value]) => (
            <div key={label} className="flex justify-between">
              <span className="text-2xs font-mono text-white/40">{label}</span>
              <span className="text-xs font-mono text-white/60">{value}</span>
            </div>
          ))}
          <div className="flex justify-between pt-1 border-t border-glass-border/50">
            <span className="text-xs font-mono font-semibold text-white">Total</span>
            <span className="text-sm font-bold font-heading text-amber-signal">
              ₱{inv.total_php.toLocaleString()}
            </span>
          </div>
          {inv.paid_at && (
            <p className="text-2xs font-mono text-green-signal mt-1">
              Paid {new Date(inv.paid_at).toLocaleDateString()}
            </p>
          )}
          {inv.due_date && inv.status !== "paid" && (
            <p className="text-2xs font-mono text-white/30 mt-1">
              Due {new Date(inv.due_date).toLocaleDateString()}
            </p>
          )}
          <div className="flex gap-2 pt-2">
            <button
              onClick={handleDownload}
              disabled={downloading}
              className="flex flex-1 items-center justify-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-2xs font-mono text-white/60 hover:text-white transition-colors disabled:opacity-40"
            >
              <Download size={11} />
              {downloading ? "Downloading…" : "Download PDF"}
            </button>
            <button
              onClick={handleResend}
              disabled={resending || resendDone}
              className="flex flex-1 items-center justify-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-2xs font-mono text-white/60 hover:text-white transition-colors disabled:opacity-40"
            >
              <Send size={11} />
              {resendDone ? "Sent!" : resending ? "Sending…" : "Resend"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────────

export default function BillingPage() {
  const [invoices,     setInvoices]     = useState<Invoice[]>([]);
  const [wallet,       setWallet]       = useState<Wallet | null>(null);
  const [codBalance,   setCodBalance]   = useState<CodBalance | null>(null);
  const [remittances,  setRemittances]  = useState<CodRemittance[]>([]);
  const [loading,      setLoading]      = useState(true);
  const [error,        setError]        = useState<string | null>(null);
  const [tab,          setTab]          = useState<FilterTab>("all");
  const [selectedInv,  setSelectedInv]  = useState<Invoice | null>(null);
  const [showPayNow,   setShowPayNow]   = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [invRes, walletData] = await Promise.all([
        billingApi.listInvoices({}, ""),
        billingApi.getWallet(""),
      ]);
      const invList = invRes.data ?? [];
      setInvoices(invList);
      setWallet(walletData);

      // Derive merchant_id from wallet tenant (1:1 for merchant portal).
      const merchantId = walletData?.tenant_id;
      if (merchantId) {
        // Fetch COD balance + remittance history in parallel; failures are non-fatal.
        const [codBal, codRem] = await Promise.allSettled([
          billingApi.getCodBalance(merchantId, ""),
          billingApi.listCodRemittances({ merchant_id: merchantId, limit: 10 }, ""),
        ]);
        if (codBal.status === "fulfilled") setCodBalance(codBal.value);
        if (codRem.status === "fulfilled") setRemittances(codRem.value.data ?? []);
      }
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
    <>
      {selectedInv && (
        <InvoiceDetailModal invoice={selectedInv} onClose={() => setSelectedInv(null)} />
      )}
      {showPayNow && (
        <BankTransferModal outstanding={outstanding} onClose={() => setShowPayNow(false)} />
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
            <button
              onClick={() => setShowPayNow(true)}
              disabled={outstanding <= 0}
              className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-amber-signal to-red-signal px-4 py-2 text-xs font-semibold text-canvas disabled:opacity-40 disabled:cursor-not-allowed"
            >
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
            <LiveMetric label="Outstanding Balance" value={outstanding} trend={0} color="red" format="currency" />
          </GlassCard>
          <GlassCard size="sm" glow="green" accent>
            <LiveMetric label="Wallet Balance" value={wallet?.balance_php ?? 0} trend={0} color="green" format="currency" />
          </GlassCard>
          <GlassCard size="sm" glow="cyan" accent>
            <LiveMetric label="Paid MTD" value={paidMtd} trend={0} color="cyan" format="currency" />
          </GlassCard>
        </motion.div>

        {/* COD Balance panel */}
        {(codBalance || loading) && (
          <motion.div variants={variants.fadeInUp}>
            <GlassCard glow="amber">
              <div className="flex items-center justify-between mb-4">
                <div className="flex items-center gap-2">
                  <Package size={14} className="text-amber-signal" />
                  <h2 className="font-heading text-sm font-semibold text-white">COD Balance</h2>
                </div>
                {codBalance && (
                  <span className="text-2xs font-mono text-white/30">
                    {codBalance.shipments_pending_cod} shipments pending
                  </span>
                )}
              </div>

              {loading && !codBalance ? (
                <p className="text-xs text-white/30 font-mono">loading…</p>
              ) : codBalance ? (
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Pending Remittance</p>
                    <p className="font-heading text-2xl font-bold text-amber-signal">
                      ₱{codBalance.pending_php.toLocaleString()}
                    </p>
                  </div>
                  <div>
                    <p className="text-2xs font-mono text-white/40 uppercase tracking-wider mb-1">Total Remitted</p>
                    <p className="font-heading text-2xl font-bold text-green-signal">
                      ₱{codBalance.remitted_php.toLocaleString()}
                    </p>
                  </div>
                  {codBalance.last_remittance_at && (
                    <div className="col-span-2">
                      <p className="text-2xs font-mono text-white/30">
                        Last remittance: {new Date(codBalance.last_remittance_at).toLocaleDateString()}
                        {codBalance.next_remittance_date && (
                          <> · Next: {new Date(codBalance.next_remittance_date).toLocaleDateString()}</>
                        )}
                      </p>
                    </div>
                  )}
                </div>
              ) : null}
            </GlassCard>
          </motion.div>
        )}

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
                  <button
                    key={inv.id}
                    onClick={() => setSelectedInv(inv)}
                    className="grid grid-cols-[1fr_120px_100px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50 hover:bg-glass-100 transition-colors w-full text-left"
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
                  </button>
                ))}
              </>
            )}
          </GlassCard>
        </motion.div>

        {/* COD Remittance history */}
        {remittances.length > 0 && (
          <motion.div variants={variants.fadeInUp}>
            <GlassCard padding="none">
              <div className="px-5 py-4 border-b border-glass-border">
                <h2 className="font-heading text-sm font-semibold text-white">COD Remittance History</h2>
              </div>
              <div className="grid grid-cols-[1fr_80px_80px_100px] gap-3 px-5 py-2.5 border-b border-glass-border">
                {["Date", "Shipments", "Amount", "Status"].map((h) => (
                  <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
                ))}
              </div>
              {remittances.map((r) => (
                <div
                  key={r.id}
                  className="grid grid-cols-[1fr_80px_80px_100px] gap-3 items-center px-5 py-3.5 border-b border-glass-border/50"
                >
                  <div>
                    <p className="text-xs font-mono text-white/70">
                      {new Date(r.created_at).toLocaleDateString()}
                    </p>
                    {r.bank_reference && (
                      <p className="text-2xs font-mono text-white/30 mt-0.5">Ref: {r.bank_reference}</p>
                    )}
                  </div>
                  <span className="text-xs font-mono text-white/60 text-center">
                    {r.shipment_count}
                  </span>
                  <span className="text-sm font-bold font-heading text-green-signal">
                    ₱{r.amount_php.toLocaleString()}
                  </span>
                  <NeonBadge variant={COD_STATUS_VARIANT[r.status]}>
                    {r.status}
                  </NeonBadge>
                </div>
              ))}
            </GlassCard>
          </motion.div>
        )}
      </motion.div>
    </>
  );
}
