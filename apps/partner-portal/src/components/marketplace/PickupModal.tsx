"use client";

/**
 * Pickup Modal — carrier records cargo collected at pickup point. Transitions
 * booking from `accepted` to `in_transit` and stamps driver + timestamp +
 * optional notes. No printable artifact (shipment receipt is the customer
 * handover document); this modal only captures the state transition.
 *
 * Production: POST /v1/marketplace/bookings/{id}/pickup with
 * { picked_up_by, pickup_notes }; server emits `shipment.picked_up` on Kafka
 * and the engagement engine fires the customer's "driver has your package"
 * notification.
 */

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, Truck, Loader2 } from "lucide-react";
import { cn } from "@/lib/design-system/cn";

export interface PickupModalBooking {
  id:                  string;
  awb:                 string;
  pickup_label:        string;
  dropoff_label:       string;
  pickup_at:           string;
  cargo_weight_kg:     number;
}

interface Props {
  open:     boolean;
  onClose:  () => void;
  booking:  PickupModalBooking | null;
  onConfirm: (input: { picked_up_by: string; pickup_notes: string }) => Promise<void>;
}

export function PickupModal({ open, onClose, booking, onConfirm }: Props) {
  const [pickedUpBy, setPickedUpBy] = useState("");
  const [notes, setNotes]           = useState("");
  const [busy, setBusy]             = useState(false);

  async function handleConfirm() {
    if (!booking) return;
    setBusy(true);
    try {
      await onConfirm({
        picked_up_by: pickedUpBy.trim(),
        pickup_notes: notes.trim(),
      });
      setPickedUpBy("");
      setNotes("");
    } finally {
      setBusy(false);
    }
  }

  return (
    <AnimatePresence>
      {open && booking && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={onClose}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
        >
          <motion.div
            initial={{ opacity: 0, scale: 0.96, y: 8 }}
            animate={{ opacity: 1, scale: 1,    y: 0 }}
            exit={{    opacity: 0, scale: 0.96, y: 8 }}
            transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-md rounded-2xl border border-glass-border bg-canvas-100/95 shadow-2xl"
          >
            {/* Header */}
            <div className="flex items-center justify-between border-b border-glass-border px-5 py-3.5">
              <div className="flex items-center gap-2">
                <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-green-surface border border-green-signal/30">
                  <Truck className="h-4 w-4 text-green-signal" />
                </div>
                <div>
                  <p className="font-heading text-sm font-semibold text-white">Record Pickup</p>
                  <p className="font-mono text-2xs text-white/40">{booking.awb}</p>
                </div>
              </div>
              <button
                onClick={onClose}
                className="rounded-md p-1 text-white/40 transition-colors hover:bg-glass-100 hover:text-white"
                title="Close"
              >
                <X className="h-4 w-4" />
              </button>
            </div>

            {/* Booking summary */}
            <div className="border-b border-glass-border px-5 py-3 bg-glass-100/40">
              <p className="text-xs text-white/60">
                <span className="text-white/90">{booking.pickup_label}</span>
                <span className="mx-1.5 text-white/30">→</span>
                <span className="text-white/90">{booking.dropoff_label}</span>
              </p>
              <p className="mt-1 font-mono text-2xs text-white/40">
                {booking.cargo_weight_kg.toLocaleString()} kg · scheduled{" "}
                {new Date(booking.pickup_at).toLocaleString("en-PH", {
                  month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
                })}
              </p>
            </div>

            {/* Form */}
            <div className="space-y-3 px-5 py-4">
              <div>
                <label className="mb-1 block text-2xs font-mono uppercase tracking-wider text-white/50">
                  Driver / Collected by
                </label>
                <input
                  value={pickedUpBy}
                  onChange={(e) => setPickedUpBy(e.target.value)}
                  placeholder="Driver J. Santos"
                  className="w-full rounded-lg border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/25 focus:border-green-signal/50 focus:outline-none"
                />
              </div>
              <div>
                <label className="mb-1 block text-2xs font-mono uppercase tracking-wider text-white/50">
                  Pickup notes (optional)
                </label>
                <textarea
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                  rows={2}
                  placeholder="Condition, seal #, handover witness…"
                  className="w-full resize-none rounded-lg border border-glass-border bg-canvas px-3 py-2 text-sm text-white placeholder-white/25 focus:border-green-signal/50 focus:outline-none"
                />
              </div>
              <p className="text-2xs text-white/40">
                Confirming flips the shipment to <span className="font-mono text-cyan-neon">in_transit</span>{" "}
                and notifies the customer.
              </p>
            </div>

            {/* Actions */}
            <div className="flex items-center justify-end gap-2 border-t border-glass-border px-5 py-3">
              <button
                onClick={onClose}
                disabled={busy}
                className="rounded-md border border-glass-border px-3 py-1.5 text-xs font-mono text-white/70 transition-colors hover:bg-glass-100 disabled:opacity-50"
              >
                Cancel
              </button>
              <button
                onClick={handleConfirm}
                disabled={busy}
                className={cn(
                  "flex items-center gap-1.5 rounded-md border border-green-signal/40 bg-green-surface px-3 py-1.5 text-xs font-mono text-green-signal transition-all",
                  "hover:shadow-[0_0_10px_rgba(0,255,136,0.45)] disabled:opacity-50 disabled:cursor-not-allowed",
                )}
              >
                {busy ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <Truck className="h-3 w-3" />
                )}
                {busy ? "Recording…" : "Confirm Pickup"}
              </button>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
