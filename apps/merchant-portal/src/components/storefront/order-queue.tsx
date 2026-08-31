"use client";
/**
 * The vendor's live order queue.
 *
 * Written for a screen on a counter, not a desk. Every action is a tap on a
 * preset: a kitchen at a lunch rush does not type a ready time and does not
 * compose a rejection reason, and a free-text box at that moment yields "asdf"
 * — which the substitution path downstream then has to interpret.
 *
 * Unanswered orders sort first and are loudest. They are the only rows costing
 * a customer time; everything below them is already in hand.
 */
import { useState } from "react";
import { motion } from "framer-motion";
import { AlarmClock, Bell, BellOff, Check, ChefHat, Clock, PackageCheck, Utensils, X } from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { variants } from "@/lib/design-system/tokens";
import {
  LegConflictError,
  vendorOrdersApi,
  type LegStatus,
  type VendorLegRow,
} from "@/lib/api/vendor-orders";

/** A kitchen taps; it does not type. */
const READY_PRESETS = [10, 15, 20, 30, 45];

/**
 * Rejection is a fact the substitution path reads, so it is a closed set. The
 * strings are what an ops person sees on an escalated order at 1pm.
 */
const REJECT_REASONS = ["Out of stock", "Closing", "Too busy", "Cannot fulfil"];

const peso = (cents: number) =>
  `₱${(cents / 100).toLocaleString("en-PH", { minimumFractionDigits: 2 })}`;

/** Whole minutes since `iso`. Never negative — a clock skew reads as "just now". */
function ageMinutes(iso: string): number {
  const ms = Date.now() - new Date(iso).getTime();
  return Math.max(0, Math.floor(ms / 60_000));
}

const STATUS_LABEL: Record<LegStatus, string> = {
  pending: "Needs an answer",
  accepted: "Accepted",
  preparing: "Preparing",
  ready: "Ready for pickup",
};

/** Sorts unanswered first, then oldest first inside each group. */
function queueOrder(a: VendorLegRow, b: VendorLegRow): number {
  if (a.status === "pending" && b.status !== "pending") return -1;
  if (b.status === "pending" && a.status !== "pending") return 1;
  return new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
}

interface Props {
  legs: VendorLegRow[];
  loaded: boolean;
  error: string | null;
  unanswered: number;
  muted: boolean;
  onToggleMute: () => void;
  onChanged: () => Promise<void> | void;
}

export function OrderQueue({
  legs, loaded, error, unanswered, muted, onToggleMute, onChanged,
}: Props) {
  // Keyed by leg so one slow request cannot disable every other row's buttons.
  const [busy, setBusy] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [rejecting, setRejecting] = useState<string | null>(null);

  async function run(legId: string, fn: () => Promise<unknown>) {
    setBusy(legId);
    setNotice(null);
    try {
      await fn();
    } catch (e) {
      // A conflict is not a failure the user caused — somebody else moved it.
      // Refetching is the fix, and saying so beats showing a status code.
      setNotice(
        e instanceof LegConflictError
          ? "Someone else already updated that order. Refreshed."
          : e instanceof Error
            ? e.message
            : "That did not go through.",
      );
    } finally {
      setBusy(null);
      setRejecting(null);
      await onChanged();
    }
  }

  const sorted = [...legs].sort(queueOrder);

  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="font-heading text-2xl font-semibold text-white">Orders</h1>
          <p className="text-sm text-white/50">
            {unanswered > 0
              ? `${unanswered} waiting for your answer`
              : "Nothing waiting on you"}
          </p>
        </div>
        <button
          type="button"
          onClick={onToggleMute}
          aria-pressed={muted}
          className="flex items-center gap-2 rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white/70 transition hover:bg-white/10"
        >
          {muted ? <BellOff className="h-4 w-4" /> : <Bell className="h-4 w-4" />}
          {muted ? "Sound off" : "Sound on"}
        </button>
      </header>

      {notice && (
        <div className="rounded-lg border border-amber-signal/30 bg-amber-signal/10 px-4 py-3 text-sm text-amber-signal">
          {notice}
        </div>
      )}
      {error && (
        <div className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-4 py-3 text-sm text-red-signal">
          {error}
        </div>
      )}

      {loaded && sorted.length === 0 && !error && (
        <GlassCard>
          <div className="flex flex-col items-center gap-2 py-10 text-center">
            <Utensils className="h-8 w-8 text-white/25" />
            <p className="text-sm text-white/50">No live orders right now.</p>
            <p className="text-xs text-white/30">
              This screen checks for new orders on its own. Leave it open.
            </p>
          </div>
        </GlassCard>
      )}

      <motion.div
        variants={variants.staggerContainer}
        initial="hidden"
        animate="visible"
        className="space-y-3"
      >
        {sorted.map((leg) => {
          const waiting = leg.status === "pending";
          const age = ageMinutes(leg.created_at);
          const isBusy = busy === leg.leg_id;

          return (
            <motion.div key={leg.leg_id} variants={variants.fadeInUp}>
              <GlassCard glow={waiting ? "amber" : "none"} accent={waiting}>
                <div className="flex flex-wrap items-start justify-between gap-4">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-mono text-xs uppercase tracking-wider text-white/40">
                        #{leg.order_id.slice(0, 8)}
                      </span>
                      <span
                        className={
                          waiting
                            ? "rounded-full border border-amber-signal/40 bg-amber-signal/10 px-2 py-0.5 text-xs text-amber-signal"
                            : "rounded-full border border-white/10 bg-white/5 px-2 py-0.5 text-xs text-white/60"
                        }
                      >
                        {STATUS_LABEL[leg.status]}
                      </span>
                    </div>
                    <p className="mt-1 text-xl font-semibold text-white">
                      {peso(leg.goods_subtotal_cents)}
                    </p>
                    <p className="mt-0.5 flex items-center gap-1.5 text-xs text-white/40">
                      <Clock className="h-3 w-3" />
                      {age === 0 ? "just now" : `${age} min ago`}
                      {leg.ready_in_minutes !== null && (
                        <span className="ml-2 flex items-center gap-1 text-white/50">
                          <AlarmClock className="h-3 w-3" />
                          promised {leg.ready_in_minutes} min
                        </span>
                      )}
                    </p>
                  </div>

                  <div className="flex flex-wrap items-center gap-2">
                    {waiting && rejecting !== leg.leg_id && (
                      <>
                        {READY_PRESETS.map((m) => (
                          <button
                            key={m}
                            type="button"
                            disabled={isBusy}
                            onClick={() =>
                              run(leg.leg_id, () => vendorOrdersApi.accept(leg.leg_id, m))
                            }
                            className="rounded-lg border border-green-signal/40 bg-green-signal/10 px-3 py-2 text-sm font-medium text-green-signal transition hover:bg-green-signal/20 disabled:opacity-40"
                          >
                            {m}m
                          </button>
                        ))}
                        <button
                          type="button"
                          disabled={isBusy}
                          onClick={() => setRejecting(leg.leg_id)}
                          className="rounded-lg border border-white/10 bg-white/5 p-2 text-white/50 transition hover:bg-white/10 disabled:opacity-40"
                          aria-label="Reject this order"
                        >
                          <X className="h-4 w-4" />
                        </button>
                      </>
                    )}

                    {waiting && rejecting === leg.leg_id && (
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-xs text-white/50">Why?</span>
                        {REJECT_REASONS.map((r) => (
                          <button
                            key={r}
                            type="button"
                            disabled={isBusy}
                            onClick={() =>
                              run(leg.leg_id, () => vendorOrdersApi.reject(leg.leg_id, r))
                            }
                            className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-sm text-red-signal transition hover:bg-red-signal/20 disabled:opacity-40"
                          >
                            {r}
                          </button>
                        ))}
                        <button
                          type="button"
                          onClick={() => setRejecting(null)}
                          className="px-2 text-xs text-white/40 hover:text-white/70"
                        >
                          Cancel
                        </button>
                      </div>
                    )}

                    {(leg.status === "accepted" || leg.status === "preparing") && (
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => run(leg.leg_id, () => vendorOrdersApi.ready(leg.leg_id))}
                        className="flex items-center gap-2 rounded-lg border border-cyan-neon/40 bg-cyan-neon/10 px-4 py-2 text-sm font-medium text-cyan-neon transition hover:bg-cyan-neon/20 disabled:opacity-40"
                      >
                        <ChefHat className="h-4 w-4" />
                        Ready
                      </button>
                    )}

                    {leg.status === "ready" && (
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => run(leg.leg_id, () => vendorOrdersApi.served(leg.leg_id))}
                        className="flex items-center gap-2 rounded-lg border border-green-signal/40 bg-green-signal/10 px-4 py-2 text-sm font-medium text-green-signal transition hover:bg-green-signal/20 disabled:opacity-40"
                      >
                        <PackageCheck className="h-4 w-4" />
                        Handed over
                      </button>
                    )}

                    {!waiting && leg.accepted_at && (
                      <Check className="h-4 w-4 text-green-signal/60" aria-hidden />
                    )}
                  </div>
                </div>
              </GlassCard>
            </motion.div>
          );
        })}
      </motion.div>
    </div>
  );
}
