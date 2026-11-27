"use client";

/**
 * Booking receipt — a read-only summary of a marketplace booking, driven
 * straight from the `MarketplaceBooking` the carrier service returned. It shows
 * only what the booking holds: there is no separate receipt record.
 */

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Printer, Share2, CheckCircle2, FileText } from "lucide-react";
import { cn } from "@/lib/design-system/cn";
import { useDialogA11y } from "@/lib/hooks/use-dialog-a11y";
import { formatCentsPhp, type MarketplaceBooking } from "@/lib/api/marketplace";

interface Props {
  open:    boolean;
  onClose: () => void;
  booking: MarketplaceBooking | null;
}

const TITLE_ID = "booking-receipt-title";

function fmtDateTime(iso: string): string {
  return new Date(iso).toLocaleString("en-PH", {
    year: "numeric", month: "short", day: "numeric",
    hour: "2-digit", minute: "2-digit",
  });
}

const PAYMENT_STATUS_LABEL: Record<MarketplaceBooking["payment_status"], string> = {
  pending:    "Pending",
  authorized: "Authorized",
  captured:   "Paid",
  voided:     "Voided",
  failed:     "Failed",
};

export function ReceiptModal({ open, onClose, booking }: Props) {
  const [copied, setCopied] = useState(false);
  const dialogRef = useDialogA11y<HTMLDivElement>(open && !!booking, onClose);

  async function handleCopyAwb() {
    if (!booking) return;
    try {
      await navigator.clipboard.writeText(booking.awb);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard permission may be denied: silent no-op, the AWB is on screen.
    }
  }

  return (
    <AnimatePresence>
      {open && booking && (
        <>
          <motion.div
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm print:hidden"
            onClick={onClose}
            aria-hidden="true"
          />
          <motion.div
            key="modal"
            initial={{ opacity: 0, scale: 0.96, y: 10 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: 10 }}
            transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
            className="fixed inset-0 z-50 flex items-center justify-center p-4 print:static print:p-0 pointer-events-none"
          >
            <div
              ref={dialogRef}
              role="dialog"
              aria-modal="true"
              aria-labelledby={TITLE_ID}
              tabIndex={-1}
              className={cn(
                "pointer-events-auto relative w-full max-w-2xl max-h-[92vh] overflow-y-auto rounded-xl border border-glass-border bg-canvas shadow-2xl outline-none",
                "print:border-0 print:bg-white print:text-black print:shadow-none"
              )}
            >
              <div className="flex items-center justify-between border-b border-glass-border px-4 sm:px-6 py-4 print:border-gray-300">
                <div className="flex items-center gap-2 min-w-0">
                  <FileText className="h-4 w-4 flex-shrink-0 text-green-signal print:text-gray-700" aria-hidden="true" />
                  <div className="min-w-0">
                    <p className="text-2xs font-mono uppercase tracking-wider text-white/40 print:text-gray-500">
                      Booking Receipt
                    </p>
                    <h2 id={TITLE_ID} className="mt-0.5 truncate font-heading text-base font-semibold text-white print:text-black">
                      {booking.awb}
                    </h2>
                  </div>
                </div>
                <button
                  onClick={onClose}
                  className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-lg border border-glass-border text-white/60 transition-colors hover:bg-glass-200 hover:text-white print:hidden"
                  aria-label="Close"
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                </button>
              </div>

              <div className="space-y-5 px-4 sm:px-6 py-5 text-sm print:text-black">
                <Row label="AWB" mono>{booking.awb}</Row>
                <Row label="Status">{booking.status.replace("_", " ")}</Row>
                <Row label="Consignee">{booking.consumer_name}</Row>
                {booking.consumer_phone && <Row label="Phone" mono>{booking.consumer_phone}</Row>}

                <div className="h-px bg-glass-border print:bg-gray-300" />

                <Row label="Pickup">{booking.pickup_label}</Row>
                <Row label="Drop-off">{booking.dropoff_label}</Row>
                <Row label="Pickup time">{fmtDateTime(booking.pickup_at)}</Row>
                {booking.picked_up_at && (
                  <Row label="Picked up">
                    {fmtDateTime(booking.picked_up_at)}
                    {booking.picked_up_by ? ` · ${booking.picked_up_by}` : ""}
                  </Row>
                )}

                <div className="h-px bg-glass-border print:bg-gray-300" />

                <Row label="Cargo">
                  {booking.cargo_weight_kg.toLocaleString()} kg
                  {booking.cargo_volume_m3 != null ? ` · ${booking.cargo_volume_m3} m³` : ""}
                </Row>
                <Row label="Distance">{booking.distance_km.toLocaleString()} km</Row>
                <Row label="Quoted price" mono>{formatCentsPhp(booking.quoted_price_cents)}</Row>
                <Row label="Payment">
                  {booking.payment_method === "online" ? "Online" : "Invoice"} · {PAYMENT_STATUS_LABEL[booking.payment_status]}
                </Row>
                {booking.pickup_notes && <Row label="Notes">{booking.pickup_notes}</Row>}

                <p className="pt-2 text-2xs text-white/40 print:text-gray-500">
                  Booked {fmtDateTime(booking.created_at)}. The carrier has {booking.response_window_mins} minutes to respond.
                </p>
              </div>

              <div className="flex flex-wrap items-center justify-end gap-2 border-t border-glass-border px-4 sm:px-6 py-4 print:hidden">
                <button
                  onClick={handleCopyAwb}
                  className="flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-surface px-3 py-2 text-xs font-medium text-cyan-neon transition-all hover:shadow-[0_0_10px_rgba(0,229,255,0.35)]"
                >
                  {copied ? <CheckCircle2 className="h-3.5 w-3.5" aria-hidden="true" /> : <Share2 className="h-3.5 w-3.5" aria-hidden="true" />}
                  {copied ? "Copied" : "Copy AWB"}
                </button>
                <button
                  onClick={() => window.print()}
                  className="flex items-center gap-2 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/80 transition-colors hover:bg-glass-200 hover:text-white"
                >
                  <Printer className="h-3.5 w-3.5" aria-hidden="true" />
                  Print
                </button>
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

function Row({ label, children, mono }: { label: string; children: React.ReactNode; mono?: boolean }) {
  return (
    <div className="flex flex-col gap-0.5 sm:flex-row sm:items-start sm:justify-between sm:gap-4">
      <span className="sm:min-w-[9rem] text-2xs font-mono uppercase tracking-wider text-white/40 print:text-gray-500">
        {label}
      </span>
      <span className={cn("break-words sm:text-right text-white/85 print:text-black", mono && "font-mono")}>
        {children}
      </span>
    </div>
  );
}
