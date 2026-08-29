"use client";

/**
 * The plan a tenant is on, and how to change it.
 *
 * Before this the Settings page showed `subscription_tier` as a read-only row
 * and there was no way to change it anywhere in any portal — `PUT
 * /v1/tenants/:id/tier` needs `tenants:manage`, which no role holds, so every
 * price on the public pricing page was decoration.
 *
 * Two things this screen deliberately does not do:
 *
 *  - It never asks the server to set a tier. It asks for a checkout page, and
 *    the tier moves when the payment is captured. There is no endpoint here
 *    that could be called to upgrade for free.
 *  - It takes no card details. The gateway's own hosted page does, which is
 *    what keeps the platform in PCI SAQ-A scope.
 */

import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import {
  subscriptionApi,
  formatMoney,
  perMonthCents,
  type BillingInterval,
  type CurrentSubscription,
  type SubscriptionPlan,
} from "@/lib/api/subscription";

/** What each status means to whoever is paying, not to the state machine. */
const STATUS_COPY: Record<
  CurrentSubscription["status"],
  { label: string; tone: "green" | "amber" | "red" | "muted"; note: string }
> = {
  pending_payment: {
    label: "Payment not completed",
    tone: "amber",
    note: "Nothing has been charged. Your plan changes when the payment goes through.",
  },
  active: { label: "Active", tone: "green", note: "" },
  past_due: {
    label: "Payment overdue",
    tone: "amber",
    note: "Your plan is still active for now. Renew to avoid dropping back to Starter.",
  },
  cancelled: {
    label: "Cancelling",
    tone: "amber",
    note: "You keep this plan until the end of the period you have already paid for.",
  },
  lapsed: {
    label: "Lapsed",
    tone: "muted",
    note: "This plan ended and your account is on Starter.",
  },
};

const TIER_LABEL: Record<string, string> = {
  starter: "Starter",
  growth: "Growth",
  business: "Business",
  enterprise: "Enterprise",
};

function tierLabel(tier: string): string {
  return TIER_LABEL[tier] ?? tier;
}

function formatDate(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export function BillingTab() {
  const [plans, setPlans] = useState<SubscriptionPlan[] | null>(null);
  const [current, setCurrent] = useState<CurrentSubscription | null>(null);
  const [interval, setInterval] = useState<BillingInterval>("annual");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [p, c] = await Promise.all([
        subscriptionApi.listPlans(),
        subscriptionApi.current(),
      ]);
      setPlans(p);
      setCurrent(c);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load your plan.");
      setPlans([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function subscribe(plan: SubscriptionPlan) {
    setBusy(plan.id);
    setError(null);
    try {
      const out = await subscriptionApi.checkout(plan.tier, plan.interval, plan.currency);
      // Same tab: the card page redirects back to the return URL, and a popup
      // is the thing browsers block.
      window.location.href = out.checkout_url;
    } catch (e) {
      setError(
        (e as { message?: string })?.message ?? "Could not start the checkout.",
      );
      setBusy(null);
    }
  }

  async function cancel() {
    setBusy("cancel");
    setError(null);
    try {
      await subscriptionApi.cancel();
      await load();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Could not cancel.");
    } finally {
      setBusy(null);
    }
  }

  const shown = (plans ?? []).filter((p) => p.interval === interval);
  const status = current ? STATUS_COPY[current.status] : null;

  return (
    <motion.div variants={variants.fadeInUp} className="space-y-6">
      {error && (
        <div
          role="alert"
          className="rounded-lg border border-[#FF3B5C]/25 bg-[#FF3B5C]/[0.07] px-4 py-3 text-sm text-[#FF3B5C]"
        >
          {error}
        </div>
      )}

      <GlassCard title="Your plan">
        {current === null ? (
          <div className="space-y-2 py-2">
            <div className="flex items-center gap-3">
              <span className="text-xl font-bold text-white font-space-grotesk">Starter</span>
              <NeonBadge variant="green">Free forever</NeonBadge>
            </div>
            <p className="text-xs text-white/40 max-w-lg">
              Up to 500 shipments a month. Choose a plan below to raise your
              limits and unlock AI dispatch, campaigns and multi-carrier
              management.
            </p>
          </div>
        ) : (
          <div className="space-y-4 py-1">
            <div className="flex flex-wrap items-center gap-3">
              <span className="text-xl font-bold text-white font-space-grotesk">
                {tierLabel(current.effective_tier)}
              </span>
              {status && <NeonBadge variant={status.tone}>{status.label}</NeonBadge>}
              {/* Surfaced rather than hidden. A paid subscription whose tier
                  never reached the identity service is the one failure mode
                  where the money moved and the entitlement did not — support
                  cannot diagnose it without seeing it. */}
              {current.status === "active" && !current.entitlement_synced && (
                <NeonBadge variant="amber">Applying…</NeonBadge>
              )}
            </div>

            {status?.note && (
              <p className="text-xs text-white/50 max-w-lg">{status.note}</p>
            )}

            <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
              {[
                { label: "Charge", value: formatMoney(current.amount_cents, current.currency) },
                { label: "Started", value: formatDate(current.current_period_start) },
                {
                  label: current.status === "cancelled" ? "Ends" : "Renews",
                  value: formatDate(current.current_period_end),
                },
              ].map((row) => (
                <div
                  key={row.label}
                  className="rounded-lg border border-white/[0.06] bg-white/[0.02] px-3 py-2"
                >
                  <div className="text-[10px] uppercase tracking-widest text-white/35 font-mono">
                    {row.label}
                  </div>
                  <div className="text-sm text-white font-medium mt-0.5">{row.value}</div>
                </div>
              ))}
            </div>

            {(current.status === "active" || current.status === "past_due") && (
              <button
                onClick={() => void cancel()}
                disabled={busy === "cancel"}
                className="text-xs text-white/40 hover:text-[#FF3B5C] underline underline-offset-4 disabled:opacity-50"
              >
                {busy === "cancel" ? "Cancelling…" : "Cancel at the end of this period"}
              </button>
            )}
          </div>
        )}
      </GlassCard>

      <GlassCard title="Change plan">
        <div className="space-y-5">
          <div className="flex items-center gap-1 bg-white/[0.03] border border-white/[0.08] rounded-xl p-1 w-fit">
            {(["monthly", "annual"] as const).map((i) => (
              <button
                key={i}
                onClick={() => setInterval(i)}
                className={`px-4 py-1.5 rounded-lg text-xs font-medium transition-all capitalize ${
                  interval === i
                    ? "bg-[#00E5FF]/10 text-[#00E5FF] border border-[#00E5FF]/20"
                    : "text-white/40 hover:text-white/70"
                }`}
              >
                {i}
              </button>
            ))}
          </div>

          {plans === null ? (
            <p className="text-sm text-white/40 py-4">Loading plans…</p>
          ) : shown.length === 0 ? (
            <p className="text-sm text-white/40 py-4">
              No {interval} plans are available for your billing currency.
            </p>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {shown.map((plan) => {
                const isCurrent =
                  current?.effective_tier === plan.tier &&
                  current?.status !== "lapsed";
                return (
                  <div
                    key={plan.id}
                    className={`rounded-xl border p-5 space-y-3 ${
                      isCurrent
                        ? "border-[#00FF88]/30 bg-[#00FF88]/[0.04]"
                        : "border-white/[0.08] bg-white/[0.02]"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-base font-bold text-white font-space-grotesk">
                        {tierLabel(plan.tier)}
                      </span>
                      {isCurrent && <NeonBadge variant="green">Current</NeonBadge>}
                    </div>

                    <div>
                      <span className="text-2xl font-bold text-white">
                        {formatMoney(perMonthCents(plan), plan.currency)}
                      </span>
                      <span className="text-xs text-white/40 ml-1">/month</span>
                    </div>

                    {/* The number actually charged, whenever it differs from
                        the per-month figure above it. An annual plan that only
                        showed "$99/month" would be quietly taking $1,188. */}
                    {plan.interval === "annual" && (
                      <p className="text-[11px] text-white/40">
                        Billed once at {formatMoney(plan.amount_cents, plan.currency)} for
                        12 months.
                      </p>
                    )}

                    <button
                      onClick={() => void subscribe(plan)}
                      disabled={busy !== null || isCurrent}
                      className={`w-full py-2 rounded-lg text-sm font-medium border transition-colors disabled:opacity-40 ${
                        isCurrent
                          ? "border-white/[0.08] text-white/40"
                          : "border-[#00E5FF]/30 text-[#00E5FF] hover:bg-[#00E5FF]/10"
                      }`}
                    >
                      {isCurrent
                        ? "Your current plan"
                        : busy === plan.id
                          ? "Opening checkout…"
                          : current
                            ? `Switch to ${tierLabel(plan.tier)}`
                            : `Choose ${tierLabel(plan.tier)}`}
                    </button>
                  </div>
                );
              })}
            </div>
          )}

          <p className="text-[11px] text-white/35 max-w-2xl leading-relaxed">
            Card details are entered on our payment provider&apos;s own secure page —
            they never pass through this portal. Enterprise is priced per
            deployment; contact sales rather than checking out here.
          </p>
        </div>
      </GlassCard>
    </motion.div>
  );
}
