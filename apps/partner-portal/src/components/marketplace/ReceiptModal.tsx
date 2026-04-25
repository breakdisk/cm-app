"use client";

/**
 * Shipment Receipt Modal — renders a BusReceipt or an issue form for a booking
 * that has no receipt yet. Shared structure across portals (partner, merchant,
 * admin) so the customer-facing artifact is identical regardless of where it's
 * viewed. Partner owns the issue mutation; merchant and admin are read-only.
 *
 * Production: modal fetches `GET /v1/marketplace/receipts/by-booking/{id}` and,
 * on issue, POSTs `/v1/marketplace/receipts`. The engagement engine then fans
 * it out to the consumer's preferred channel (ADR-0013 §Booking flow).
 */

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Printer, Share2, CheckCircle2, FileText, Loader2 } from "lucide-react";
import { cn } from "@/lib/design-system/cn";
import type { BusReceipt, BusBookingStatus } from "@/lib/api/marketplace-bus";

export interface ReceiptModalBooking {
  id:                   string;
  awb:                  string;
  partner_display_name: string;
  merchant_display?:    string;
  consumer_display?:    string;
  pickup_label:         string;
  dropoff_label:        string;
  pickup_at:            string;
  cargo_weight_kg:      number;
  quoted_price_cents:   number;
  status:               BusBookingStatus;
}

interface Props {
  open:      boolean;
  onClose:   () => void;
  booking?:  ReceiptModalBooking | null;    // required when receipt is null (for issue form)
  receipt:   BusReceipt | null;
  /** Provide to show the issue form when receipt is null. Partner-side only. */
  onIssue?:  (input: { signed_by: string; notes: string }) => Promise<void>;
}

function fmtPhp(cents: number): string {
  return "₱" + (cents / 100).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

function fmtDateTime(iso: string): string {
  return new Date(iso).toLocaleString("en-PH", {
    year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit",
  });
}

export function ReceiptModal({ open, onClose, booking, receipt, onIssue }: Props) {
  const [signedBy, setSignedBy] = useState("");
  const [notes, setNotes]       = useState("");
  const [busy, setBusy]         = useState(false);
  const [copied, setCopied]     = useState(false);

  const canIssue = open && !receipt && !!booking && !!onIssue;
  const showPrint = open && !!receipt;

  async function handleIssue() {
    if (!onIssue) return;
    setBusy(true);
    try {
      await onIssue({ signed_by: signedBy.trim(), notes: notes.trim() });
      setSignedBy("");
      setNotes("");
    } finally {
      setBusy(false);
    }
  }

  async function handleCopyShareLink() {
    if (!receipt) return;
    const shareUrl = `https://cargomarket.app/t/${receipt.awb}`;
    try {
      await navigator.clipboard.writeText(shareUrl);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard permission may be denied — silent no-op, user can still print.
    }
  }

  function handlePrint() {
    if (typeof window !== "undefined") window.print();
  }

  return (
    <AnimatePresence>
      {open && (
        <>
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm print:hidden"
            onClick={onClose}
          />
          <motion.div
            key="modal"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: 10 }}
            transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
            className="fixed inset-0 z-50 flex items-center justify-center p-4 print:static print:p-0"
            role="dialog"
            aria-modal="true"
          >
            <div
              className={cn(
                "relative w-full max-w-2xl overflow-hidden rounded-xl border border-glass-border bg-canvas shadow-2xl",
                "print:border-0 print:bg-white print:text-black print:shadow-none"
              )}
            >
              {/* Header */}
              <div className="flex items-center justify-between border-b border-glass-border px-6 py-4 print:border-gray-300">
                <div className="flex items-center gap-2">
                  <FileText className="h-4 w-4 text-green-signal print:text-gray-700" />
                  <div>
                    <p className="text-2xs font-mono uppercase tracking-wider text-white/40 print:text-gray-500">
                      Shipment Receipt
                    </p>
                    <h2 className="mt-0.5 font-heading text-base font-semibold text-white print:text-black">
                      {receipt ? receipt.receipt_no : "Not yet issued"}
                    </h2>
                  </div>
                </div>
                <button
                  onClick={onClose}
                  className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/60 transition-colors hover:bg-glass-200 hover:text-white print:hidden"
                  aria-label="Close"
                >
                  <X className="h-4 w-4" />
                </button>
              </div>

              {/* Body */}
              <div className="px-6 py-5">
                {receipt ? (
                  <ReceiptBody receipt={receipt} />
                ) : booking ? (
                  <IssueForm
                    booking={booking}
                    signedBy={signedBy}
                    setSignedBy={setSignedBy}
                    notes={notes}
                    setNotes={setNotes}
                    disabled={!canIssue}
                  />
                ) : (
                  <p className="py-6 text-center text-sm text-white/50">No receipt available.</p>
                )}
              </div>

              {/* Footer */}
              <div className="flex items-center justify-end gap-2 border-t border-glass-border px-6 py-4 print:hidden">
                {showPrint && (
                  <>
                    <button
                      onClick={handleCopyShareLink}
                      className={cn(
                        "flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-surface px-3 py-2",
                        "text-xs font-medium text-cyan-neon transition-all hover:shadow-[0_0_10px_rgba(0,229,255,0.35)]"
                      )}
                      title="Copy a shareable tracking + receipt link for the customer"
                    >
                      {copied ? <CheckCircle2 className="h-3.5 w-3.5" /> : <Share2 className="h-3.5 w-3.5" />}
                      {copied ? "Copied" : "Copy customer link"}
                    </button>
                    <button
                      onClick={handlePrint}
                      className="flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/80 transition-colors hover:bg-glass-200 hover:text-white"
                    >
                      <Printer className="h-3.5 w-3.5" />
                      Print
                    </button>
                  </>
                )}
                {canIssue && (
                  <button
                    onClick={handleIssue}
                    disabled={busy}
                    className={cn(
                      "flex items-center gap-2 rounded-lg border border-green-signal/40 bg-green-surface px-4 py-2",
                      "text-sm font-medium text-green-signal transition-all hover:shadow-[0_0_12px_rgba(0,255,136,0.4)]",
                      busy && "opacity-50"
                    )}
                  >
                    {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FileText className="h-3.5 w-3.5" />}
                    {busy ? "Issuing…" : "Issue & send to customer"}
                  </button>
                )}
                <button
                  onClick={onClose}
                  className="rounded-lg border border-glass-border bg-glass-100 px-4 py-2 text-sm text-white/70 transition-colors hover:bg-glass-200 hover:text-white"
                >
                  Close
                </button>
              </div>
            </div>
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

// ── Receipt body (issued) ────────────────────────────────────────────────────

function ReceiptBody({ receipt }: { receipt: BusReceipt }) {
  return (
    <div className="space-y-5 text-sm print:text-black">
      {/* Status strip */}
      <div className="flex items-center gap-2 rounded-lg border border-green-signal/30 bg-green-signal/5 px-3 py-2 text-green-signal print:border-gray-300 print:bg-white print:text-gray-700">
        <CheckCircle2 className="h-4 w-4" />
        <span className="text-xs">
          Issued {fmtDateTime(receipt.issued_at)} — delivered to customer via preferred channel.
        </span>
      </div>

      <Row label="AWB"         mono>{receipt.awb}</Row>
      <Row label="Carrier">{receipt.partner_display_name}</Row>
      <Row label="Issued by">{receipt.issued_by_name}</Row>
      <Row label="Shipper / Merchant">{receipt.merchant_display}</Row>
      <Row label="Consignee / Consumer">{receipt.consumer_display}</Row>

      <div className="h-px bg-glass-border print:bg-gray-300" />

      <Row label="Pickup">{receipt.pickup_label}</Row>
      <Row label="Drop-off">{receipt.dropoff_label}</Row>
      <Row label="Pickup time">{fmtDateTime(receipt.pickup_at)}</Row>

      <div className="h-px bg-glass-border print:bg-gray-300" />

      <Row label="Cargo">
        {receipt.cargo_weight_kg.toLocaleString()} kg · {receipt.size_class.replace("_", " ")}
      </Row>
      <Row label="Quoted price" mono>{fmtPhp(receipt.quoted_price_cents)}</Row>

      {(receipt.signed_by || receipt.notes) && (
        <>
          <div className="h-px bg-glass-border print:bg-gray-300" />
          {receipt.signed_by && <Row label="Received by">{receipt.signed_by}</Row>}
          {receipt.notes     && <Row label="Notes">{receipt.notes}</Row>}
        </>
      )}

      <p className="pt-2 text-2xs text-white/40 print:text-gray-500">
        Track at{" "}
        <span className="font-mono">cargomarket.app/t/{receipt.awb}</span>. Retain this receipt
        for proof of handover.
      </p>
    </div>
  );
}

function Row({ label, children, mono }: { label: string; children: React.ReactNode; mono?: boolean }) {
  return (
    <div className="flex items-start justify-between gap-4">
      <span className="min-w-[9rem] text-2xs font-mono uppercase tracking-wider text-white/40 print:text-gray-500">
        {label}
      </span>
      <span className={cn("text-right text-white/85 print:text-black", mono && "font-mono")}>
        {children}
      </span>
    </div>
  );
}

// ── Issue form (pre-issuance) ────────────────────────────────────────────────

function IssueForm({
  booking,
  signedBy,
  setSignedBy,
  notes,
  setNotes,
  disabled,
}: {
  booking: ReceiptModalBooking;
  signedBy: string;
  setSignedBy: (v: string) => void;
  notes: string;
  setNotes: (v: string) => void;
  disabled: boolean;
}) {
  return (
    <div className="space-y-4 text-sm">
      <div className="rounded-lg border border-amber-signal/30 bg-amber-surface px-3 py-2 text-xs text-amber-signal">
        Receipt not yet issued. Once issued, the customer receives it via their preferred channel
        and it can&apos;t be edited — only superseded by an amendment.
      </div>

      <Row label="AWB" mono>{booking.awb}</Row>
      <Row label="Carrier">{booking.partner_display_name}</Row>
      <Row label="Pickup">{booking.pickup_label}</Row>
      <Row label="Drop-off">{booking.dropoff_label}</Row>
      <Row label="Pickup time">{fmtDateTime(booking.pickup_at)}</Row>
      <Row label="Cargo">{booking.cargo_weight_kg.toLocaleString()} kg</Row>
      <Row label="Quoted price" mono>{fmtPhp(booking.quoted_price_cents)}</Row>

      <div className="h-px bg-glass-border" />

      <label className="block">
        <span className="mb-1.5 block text-2xs font-mono uppercase tracking-wider text-white/50">
          Received by (signer)
        </span>
        <input
          value={signedBy}
          onChange={(e) => setSignedBy(e.target.value)}
          placeholder="e.g. J. Dela Cruz"
          disabled={disabled}
          className="w-full rounded-lg border border-glass-border bg-white/5 px-3 py-2 text-sm text-white/90 outline-none focus:border-green-signal/50 focus:ring-2 focus:ring-green-signal/20"
        />
      </label>
      <label className="block">
        <span className="mb-1.5 block text-2xs font-mono uppercase tracking-wider text-white/50">
          Notes (optional)
        </span>
        <textarea
          value={notes}
          onChange={(e) => setNotes(e.target.value)}
          placeholder="Cargo condition, box count, special handling…"
          rows={3}
          disabled={disabled}
          className="w-full rounded-lg border border-glass-border bg-white/5 px-3 py-2 text-sm text-white/90 outline-none focus:border-green-signal/50 focus:ring-2 focus:ring-green-signal/20"
        />
      </label>
    </div>
  );
}
