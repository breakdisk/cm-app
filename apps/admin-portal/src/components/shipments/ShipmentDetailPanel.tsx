"use client";
/**
 * ShipmentDetailPanel
 *
 * Right-side slide-over panel that shows full shipment details + POD evidence
 * for a selected shipment. Opened from the Shipments list page when a row is
 * clicked; closed via the × button or backdrop click.
 *
 * POD is fetched from GET /v1/pods?shipment_id={id}. The service also supports
 * ?task_id={id} but we always have shipment_id on the admin side.
 */
import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import {
  X,
  Camera,
  PenLine,
  MapPin,
  CheckCircle2,
  AlertTriangle,
  Clock,
  Package,
  Phone,
  Truck,
  Weight,
  Banknote,
  ChevronRight,
} from "lucide-react";

import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { authFetch } from "@/lib/auth/auth-fetch";
import {
  cancelShipment,
  rescheduleShipment,
  overrideShipmentStatus,
} from "@/lib/api/shipments";
import { usePermissions } from "@/hooks/usePermissions";

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

// ── Types ──────────────────────────────────────────────────────────────────────

interface ApiAddress {
  line1: string;
  line2?: string | null;
  city: string;
  province: string;
  postal_code: string;
  country_code: string;
  coordinates?: { lat: number; lng: number } | null;
}

interface ApiMoney {
  amount: number;
  currency: string;
}

export interface ApiShipment {
  id: string;
  tenant_id: string;
  merchant_id: string;
  customer_id: string;
  customer_name: string;
  customer_phone: string;
  customer_email?: string | null;
  booked_by_customer: boolean;
  awb: string;
  piece_count: number;
  status: string;
  service_type: string;
  origin: ApiAddress;
  destination: ApiAddress;
  weight: { grams: number };
  declared_value?: ApiMoney | null;
  cod_amount?: ApiMoney | null;
  created_at: string;
  updated_at: string;
}

interface PodData {
  pod_id: string;
  shipment_id: string;
  task_id?: string | null;
  status: string;
  recipient_name?: string | null;
  geofence_verified: boolean;
  capture_lat?: number | null;
  capture_lng?: number | null;
  photo_url?: string | null;
  signature_url?: string | null;
  otp_verified?: boolean | null;
  cod_collected_cents?: number | null;
  captured_at?: string | null;
}

// ── Valid status list for override dropdown ────────────────────────────────────

const ALL_STATUSES = [
  "pending",
  "confirmed",
  "pickup_assigned",
  "picked_up",
  "in_transit",
  "at_hub",
  "out_for_delivery",
  "delivery_attempted",
  "delivered",
  "partial_delivery",
  "piece_exception",
  "customs_hold",
  "failed",
  "cancelled",
  "returned",
] as const;

const TIME_SLOTS = ["morning", "afternoon", "anytime"] as const;

interface ShipmentDetailPanelProps {
  shipment: ApiShipment | null;
  onClose: () => void;
  onActionComplete?: () => void;
}

// ── Status timeline ────────────────────────────────────────────────────────────

const TIMELINE_STEPS: { key: string; label: string }[] = [
  { key: "pending",            label: "Pending" },
  { key: "confirmed",          label: "Confirmed" },
  { key: "pickup_assigned",    label: "Pickup Assigned" },
  { key: "picked_up",          label: "Picked Up" },
  { key: "in_transit",         label: "In Transit" },
  { key: "at_hub",             label: "At Hub" },
  { key: "out_for_delivery",   label: "Out for Delivery" },
  { key: "delivered",          label: "Delivered" },
];

const TERMINAL_STATUSES = new Set(["delivered", "failed", "cancelled", "returned"]);

const STATUS_VARIANT: Record<string, "green" | "cyan" | "amber" | "red" | "purple"> = {
  pending:            "amber",
  confirmed:          "cyan",
  pickup_assigned:    "cyan",
  picked_up:          "cyan",
  in_transit:         "purple",
  at_hub:             "purple",
  out_for_delivery:   "cyan",
  delivery_attempted: "amber",
  delivered:          "green",
  partial_delivery:   "amber",
  piece_exception:    "amber",
  customs_hold:       "amber",
  failed:             "red",
  cancelled:          "red",
  returned:           "red",
};

function resolveTimelineStepIndex(status: string): number {
  const idx = TIMELINE_STEPS.findIndex((s) => s.key === status);
  // For statuses outside the normal flow (failed / cancelled / returned) treat
  // the shipment as having reached the last shown step but mark it differently
  // via the status badge — the caller handles that.
  return idx === -1 ? -1 : idx;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

function prettyStatus(s: string): string {
  return s.replace(/_/g, " ");
}

function formatMoney(m: ApiMoney | null | undefined): string | null {
  if (!m) return null;
  const symbol = m.currency === "PHP" ? "₱" : `${m.currency} `;
  return `${symbol}${(m.amount / 100).toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

// ── Component ──────────────────────────────────────────────────────────────────

export function ShipmentDetailPanel({ shipment, onClose, onActionComplete }: ShipmentDetailPanelProps) {
  const [pod,        setPod]        = useState<PodData | null>(null);
  const [podLoading, setPodLoading] = useState(false);
  const [podError,   setPodError]   = useState<string | null>(null);
  const backdropRef = useRef<HTMLDivElement>(null);

  const { hasPermission } = usePermissions();
  const canCancel   = hasPermission("shipments:cancel");
  const canUpdate   = hasPermission("shipments:update");

  // ── Action state ──────────────────────────────────────────────────────────
  const [actionBusy,    setActionBusy]    = useState(false);
  const [actionError,   setActionError]   = useState<string | null>(null);
  const [showReschedule, setShowReschedule] = useState(false);
  const [rescheduleDate, setRescheduleDate] = useState("");
  const [rescheduleSlot, setRescheduleSlot] = useState<string>("anytime");
  const [showOverride,  setShowOverride]  = useState(false);
  const [overrideStatus, setOverrideStatus] = useState("");

  async function handleCancel() {
    if (!shipment) return;
    if (!window.confirm(`Cancel shipment ${shipment.awb}? This cannot be undone.`)) return;
    setActionBusy(true);
    setActionError(null);
    try {
      await cancelShipment(shipment.id, "Cancelled by ops");
      onActionComplete?.();
      onClose();
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : "Cancel failed");
    } finally {
      setActionBusy(false);
    }
  }

  async function handleReschedule() {
    if (!shipment || !rescheduleDate) return;
    setActionBusy(true);
    setActionError(null);
    try {
      await rescheduleShipment(shipment.id, {
        preferred_date: rescheduleDate,
        preferred_time_slot: rescheduleSlot || null,
        reason: "Rescheduled by ops admin",
      });
      setShowReschedule(false);
      onActionComplete?.();
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : "Reschedule failed");
    } finally {
      setActionBusy(false);
    }
  }

  async function handleOverride() {
    if (!shipment || !overrideStatus) return;
    setActionBusy(true);
    setActionError(null);
    try {
      await overrideShipmentStatus(shipment.id, {
        status: overrideStatus,
        reason: "Manual override by ops admin",
      });
      setShowOverride(false);
      onActionComplete?.();
    } catch (e: unknown) {
      setActionError(e instanceof Error ? e.message : "Override failed");
    } finally {
      setActionBusy(false);
    }
  }

  // Fetch POD whenever the selected shipment changes
  useEffect(() => {
    if (!shipment) {
      setPod(null);
      setPodError(null);
      return;
    }

    let cancelled = false;

    async function fetchPod() {
      if (!shipment) return;
      setPodLoading(true);
      setPod(null);
      setPodError(null);
      try {
        const res = await authFetch(
          `${API_BASE}/v1/pods?shipment_id=${shipment.id}`
        );
        if (res.status === 404) {
          // No POD yet — not an error, just empty state
          if (!cancelled) setPod(null);
          return;
        }
        if (!res.ok) {
          throw new Error(`${res.status} ${res.statusText}`);
        }
        const json = await res.json();
        // Response shape: { data: PodData } or { data: [] } for no-POD
        if (!cancelled) {
          const payload = json?.data;
          if (Array.isArray(payload) && payload.length === 0) {
            setPod(null);
          } else if (Array.isArray(payload)) {
            setPod(payload[0] as PodData);
          } else if (payload && typeof payload === "object" && "pod_id" in payload) {
            setPod(payload as PodData);
          } else {
            setPod(null);
          }
        }
      } catch (e: unknown) {
        if (!cancelled) {
          setPodError(e instanceof Error ? e.message : "Failed to load POD");
        }
      } finally {
        if (!cancelled) setPodLoading(false);
      }
    }

    fetchPod();
    return () => { cancelled = true; };
  }, [shipment?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  // Close on Escape
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const statusVariant = shipment ? (STATUS_VARIANT[shipment.status] ?? "cyan") : "cyan";
  const timelineIdx   = shipment ? resolveTimelineStepIndex(shipment.status) : -1;
  const isTerminal    = shipment ? TERMINAL_STATUSES.has(shipment.status) : false;
  const isFailed      = shipment ? ["failed", "cancelled", "returned"].includes(shipment.status) : false;

  return (
    <AnimatePresence>
      {shipment && (
        <>
          {/* Backdrop */}
          <motion.div
            ref={backdropRef}
            key="backdrop"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2 }}
            className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm"
            onClick={onClose}
          />

          {/* Panel */}
          <motion.div
            key="panel"
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={{ type: "spring", stiffness: 320, damping: 32, mass: 0.9 }}
            className="fixed right-0 top-0 bottom-0 z-50 flex w-[480px] max-w-full flex-col overflow-hidden"
            style={{ background: "rgba(5,8,16,0.97)", borderLeft: "1px solid rgba(255,255,255,0.08)" }}
          >
            {/* ── Panel Header ── */}
            <div
              className="flex items-start justify-between gap-4 border-b border-glass-border px-6 py-5"
              style={{ backdropFilter: "blur(20px)" }}
            >
              <div className="min-w-0 flex-1">
                <p className="font-mono text-xs text-white/40 uppercase tracking-widest mb-1">
                  Shipment Detail
                </p>
                <p className="font-mono text-lg font-bold text-cyan-neon truncate">
                  {shipment.awb}
                </p>
                <div className="mt-1.5 flex items-center gap-2 flex-wrap">
                  <NeonBadge variant={statusVariant} dot pulse={!isTerminal}>
                    {prettyStatus(shipment.status)}
                  </NeonBadge>
                </div>
              </div>
              <button
                onClick={onClose}
                className="mt-0.5 flex-shrink-0 rounded-lg p-1.5 text-white/40 hover:bg-glass-200 hover:text-white transition-colors"
                aria-label="Close panel"
              >
                <X size={18} />
              </button>
            </div>

            {/* ── Scrollable body ── */}
            <div className="flex-1 overflow-y-auto overscroll-contain px-6 py-5 space-y-5">

              {/* Recipient summary */}
              <div className="flex items-center gap-3">
                <div className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full bg-cyan-surface border border-cyan-neon/20">
                  <span className="font-mono text-sm font-bold text-cyan-neon">
                    {shipment.customer_name.charAt(0).toUpperCase()}
                  </span>
                </div>
                <div className="min-w-0">
                  <p className="text-sm font-semibold text-white truncate">{shipment.customer_name}</p>
                  <p className="font-mono text-2xs text-white/40 truncate">{shipment.customer_phone}</p>
                </div>
              </div>

              {/* ── Delivery Details ── */}
              <section>
                <SectionHeading>Delivery Details</SectionHeading>
                <GlassCard size="sm" className="mt-2 space-y-3">
                  <DetailRow
                    icon={<MapPin size={13} className="text-cyan-neon" />}
                    label="Destination"
                    value={[
                      shipment.destination.line1,
                      shipment.destination.line2,
                      `${shipment.destination.city}, ${shipment.destination.province} ${shipment.destination.postal_code}`,
                    ]
                      .filter(Boolean)
                      .join(", ")}
                  />
                  <DetailRow
                    icon={<MapPin size={13} className="text-white/30" />}
                    label="Origin"
                    value={`${shipment.origin.city}, ${shipment.origin.province}`}
                  />
                  <DetailRow
                    icon={<Phone size={13} className="text-white/30" />}
                    label="Phone"
                    value={shipment.customer_phone}
                    mono
                  />
                  <DetailRow
                    icon={<Truck size={13} className="text-purple-plasma" />}
                    label="Service"
                    value={prettyStatus(shipment.service_type)}
                  />
                  <DetailRow
                    icon={<Weight size={13} className="text-white/30" />}
                    label="Weight"
                    value={shipment.weight.grams >= 1000
                      ? `${(shipment.weight.grams / 1000).toFixed(2)} kg`
                      : `${shipment.weight.grams} g`}
                    mono
                  />
                  {shipment.cod_amount && (
                    <DetailRow
                      icon={<Banknote size={13} className="text-amber-signal" />}
                      label="COD Amount"
                      value={formatMoney(shipment.cod_amount) ?? "—"}
                      mono
                      highlight="amber"
                    />
                  )}
                  {shipment.piece_count > 1 && (
                    <DetailRow
                      icon={<Package size={13} className="text-white/30" />}
                      label="Pieces"
                      value={`${shipment.piece_count}`}
                      mono
                    />
                  )}
                  <DetailRow
                    icon={<Clock size={13} className="text-white/30" />}
                    label="Created"
                    value={formatDate(shipment.created_at)}
                    mono
                  />
                </GlassCard>
              </section>

              {/* ── POD Evidence ── */}
              <section>
                <SectionHeading>Proof of Delivery</SectionHeading>
                <div className="mt-2">
                  {podLoading && <PodSkeleton />}
                  {!podLoading && podError && (
                    <GlassCard size="sm">
                      <p className="text-xs font-mono text-red-400">{podError}</p>
                    </GlassCard>
                  )}
                  {!podLoading && !podError && !pod && <NoPodPlaceholder />}
                  {!podLoading && !podError && pod && <PodCard pod={pod} />}
                </div>
              </section>

              {/* ── Timeline ── */}
              <section>
                <SectionHeading>Status Timeline</SectionHeading>
                <GlassCard size="sm" className="mt-2">
                  {isFailed ? (
                    /* Failed / cancelled — show a collapsed terminal state */
                    <div className="flex items-center gap-3">
                      <div className="flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-full border border-red-signal/40 bg-red-surface">
                        <X size={12} className="text-red-signal" />
                      </div>
                      <div>
                        <p className="text-xs font-semibold text-red-400 capitalize">
                          {prettyStatus(shipment.status)}
                        </p>
                        <p className="text-2xs font-mono text-white/30">
                          {formatDate(shipment.updated_at)}
                        </p>
                      </div>
                    </div>
                  ) : (
                    <ol className="space-y-0">
                      {TIMELINE_STEPS.map((step, idx) => {
                        const isDone    = timelineIdx > idx;
                        const isCurrent = timelineIdx === idx;
                        const isFuture  = timelineIdx < idx;
                        return (
                          <li key={step.key} className="flex items-start gap-3">
                            {/* Connector line + dot */}
                            <div className="flex flex-col items-center">
                              <div
                                className={[
                                  "flex h-5 w-5 flex-shrink-0 items-center justify-center rounded-full border transition-all",
                                  isDone    && "border-cyan-neon/50 bg-cyan-surface",
                                  isCurrent && "border-green-signal/70 bg-green-surface shadow-glow-sm-green",
                                  isFuture  && "border-glass-border bg-glass-100",
                                ].filter(Boolean).join(" ")}
                              >
                                {isDone && (
                                  <CheckCircle2 size={11} className="text-cyan-neon" />
                                )}
                                {isCurrent && (
                                  <span className="h-2 w-2 rounded-full bg-green-signal animate-beacon" />
                                )}
                              </div>
                              {idx < TIMELINE_STEPS.length - 1 && (
                                <div
                                  className={[
                                    "my-0.5 w-px flex-1",
                                    isDone ? "bg-cyan-neon/25 min-h-[14px]" : "bg-glass-border min-h-[14px]",
                                  ].join(" ")}
                                />
                              )}
                            </div>
                            {/* Label */}
                            <p
                              className={[
                                "pb-2 pt-0.5 text-xs",
                                isDone    && "text-white/50",
                                isCurrent && "font-semibold text-green-signal",
                                isFuture  && "text-white/20",
                              ].filter(Boolean).join(" ")}
                            >
                              {step.label}
                            </p>
                          </li>
                        );
                      })}
                    </ol>
                  )}
                </GlassCard>
              </section>

              {/* ── Admin Actions — only rendered when caller has at least one mutation permission ── */}
              {(canCancel || canUpdate) && (
              <section>
                <SectionHeading>Admin Actions</SectionHeading>

                {actionError && (
                  <div className="mt-2 rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs font-mono text-red-400">
                    {actionError}
                  </div>
                )}

                <div className="mt-2 flex flex-wrap gap-2">
                  {/* Cancel button — shipments:cancel, only for Pending / Confirmed */}
                  {canCancel && (shipment.status === "pending" || shipment.status === "confirmed") && (
                    <button
                      onClick={handleCancel}
                      disabled={actionBusy}
                      className="rounded-lg border border-red-500/30 bg-red-500/10 px-3 py-1.5 text-xs font-mono text-red-400 hover:bg-red-500/20 transition-colors disabled:opacity-50"
                    >
                      Cancel Shipment
                    </button>
                  )}

                  {/* Reschedule button — shipments:update, for delivery_attempted / failed */}
                  {canUpdate && (shipment.status === "delivery_attempted" || shipment.status === "failed") && (
                    <button
                      onClick={() => { setShowReschedule((v) => !v); setShowOverride(false); }}
                      disabled={actionBusy}
                      className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-xs font-mono text-amber-400 hover:bg-amber-500/20 transition-colors disabled:opacity-50"
                    >
                      Reschedule
                    </button>
                  )}

                  {/* Override Status — shipments:update */}
                  {canUpdate && (
                    <button
                      onClick={() => { setShowOverride((v) => !v); setShowReschedule(false); }}
                      disabled={actionBusy}
                      className="rounded-lg border border-purple-500/30 bg-purple-500/10 px-3 py-1.5 text-xs font-mono text-purple-400 hover:bg-purple-500/20 transition-colors disabled:opacity-50"
                    >
                      Override Status
                    </button>
                  )}
                </div>

                {/* Reschedule inline form */}
                {showReschedule && (
                  <GlassCard size="sm" className="mt-3 space-y-3">
                    <p className="text-2xs font-mono uppercase tracking-widest text-white/40">Reschedule Delivery</p>
                    <div className="space-y-2">
                      <div>
                        <label className="block text-2xs font-mono text-white/40 mb-1">Preferred Date</label>
                        <input
                          type="date"
                          value={rescheduleDate}
                          onChange={(e) => setRescheduleDate(e.target.value)}
                          className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs font-mono text-white outline-none focus:border-amber-500/50 transition-colors"
                          min={new Date().toISOString().split("T")[0]}
                        />
                      </div>
                      <div>
                        <label className="block text-2xs font-mono text-white/40 mb-1">Time Slot</label>
                        <select
                          value={rescheduleSlot}
                          onChange={(e) => setRescheduleSlot(e.target.value)}
                          className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs font-mono text-white outline-none focus:border-amber-500/50 transition-colors"
                        >
                          {TIME_SLOTS.map((s) => (
                            <option key={s} value={s} className="bg-canvas capitalize">{s}</option>
                          ))}
                        </select>
                      </div>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={handleReschedule}
                        disabled={actionBusy || !rescheduleDate}
                        className="rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-xs font-mono text-amber-400 hover:bg-amber-500/20 transition-colors disabled:opacity-50"
                      >
                        {actionBusy ? "Saving…" : "Confirm Reschedule"}
                      </button>
                      <button
                        onClick={() => setShowReschedule(false)}
                        className="rounded-lg border border-glass-border px-3 py-1.5 text-xs font-mono text-white/40 hover:text-white transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  </GlassCard>
                )}

                {/* Override Status inline form */}
                {showOverride && canUpdate && (
                  <GlassCard size="sm" className="mt-3 space-y-3">
                    <p className="text-2xs font-mono uppercase tracking-widest text-white/40">Override Status</p>
                    <div>
                      <label className="block text-2xs font-mono text-white/40 mb-1">New Status</label>
                      <select
                        value={overrideStatus}
                        onChange={(e) => setOverrideStatus(e.target.value)}
                        className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs font-mono text-white outline-none focus:border-purple-500/50 transition-colors"
                      >
                        <option value="" disabled className="bg-canvas">Select status…</option>
                        {ALL_STATUSES.map((s) => (
                          <option key={s} value={s} className="bg-canvas capitalize">
                            {s.replace(/_/g, " ")}
                          </option>
                        ))}
                      </select>
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={handleOverride}
                        disabled={actionBusy || !overrideStatus}
                        className="rounded-lg border border-purple-500/30 bg-purple-500/10 px-3 py-1.5 text-xs font-mono text-purple-400 hover:bg-purple-500/20 transition-colors disabled:opacity-50"
                      >
                        {actionBusy ? "Saving…" : "Apply Override"}
                      </button>
                      <button
                        onClick={() => setShowOverride(false)}
                        className="rounded-lg border border-glass-border px-3 py-1.5 text-xs font-mono text-white/40 hover:text-white transition-colors"
                      >
                        Cancel
                      </button>
                    </div>
                  </GlassCard>
                )}
              </section>
              )}

              {/* Bottom padding so last section isn't cut by shadow */}
              <div className="h-4" />
            </div>

            {/* Bottom fade */}
            <div
              className="pointer-events-none absolute bottom-0 left-0 right-0 h-10"
              style={{ background: "linear-gradient(to top, rgba(5,8,16,0.97), transparent)" }}
            />
          </motion.div>
        </>
      )}
    </AnimatePresence>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function SectionHeading({ children }: { children: React.ReactNode }) {
  return (
    <h3 className="flex items-center gap-2 text-2xs font-mono uppercase tracking-widest text-white/30">
      <ChevronRight size={10} className="text-cyan-neon/50" />
      {children}
    </h3>
  );
}

interface DetailRowProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  mono?: boolean;
  highlight?: "amber" | "cyan" | "green";
}

function DetailRow({ icon, label, value, mono, highlight }: DetailRowProps) {
  const valueClass = highlight === "amber"
    ? "text-amber-signal"
    : highlight === "cyan"
    ? "text-cyan-neon"
    : highlight === "green"
    ? "text-green-signal"
    : "text-white/80";

  return (
    <div className="flex items-start gap-2">
      <span className="mt-0.5 flex-shrink-0">{icon}</span>
      <span className="w-20 flex-shrink-0 text-2xs font-mono text-white/30 uppercase tracking-wider">
        {label}
      </span>
      <span className={`flex-1 text-xs break-words ${mono ? "font-mono" : ""} ${valueClass}`}>
        {value}
      </span>
    </div>
  );
}

function PodSkeleton() {
  return (
    <GlassCard size="sm" className="space-y-3 animate-pulse">
      <div className="h-3 w-24 rounded bg-glass-200" />
      <div className="aspect-video w-full rounded-lg bg-glass-200" />
      <div className="h-3 w-32 rounded bg-glass-200" />
      <div className="h-3 w-48 rounded bg-glass-200" />
    </GlassCard>
  );
}

function NoPodPlaceholder() {
  return (
    <GlassCard size="sm">
      <div className="flex flex-col items-center gap-2 py-6 text-center">
        <div className="flex h-10 w-10 items-center justify-center rounded-full border border-glass-border bg-glass-100">
          <Camera size={18} className="text-white/20" />
        </div>
        <p className="text-xs text-white/30">No proof of delivery recorded yet</p>
        <p className="text-2xs font-mono text-white/20">POD is captured by the driver upon delivery</p>
      </div>
    </GlassCard>
  );
}

function PodCard({ pod }: { pod: PodData }) {
  return (
    <GlassCard size="sm" className="space-y-4">
      {/* Status row */}
      <div className="flex items-center justify-between">
        <NeonBadge variant="green" dot>
          {prettyStatus(pod.status)}
        </NeonBadge>
        {pod.geofence_verified ? (
          <span className="inline-flex items-center gap-1 text-2xs font-mono text-green-signal">
            <CheckCircle2 size={12} />
            Geofence verified
          </span>
        ) : (
          <span className="inline-flex items-center gap-1 text-2xs font-mono text-amber-signal">
            <AlertTriangle size={12} />
            Geofence unverified
          </span>
        )}
      </div>

      {/* Photo */}
      <div>
        <p className="mb-1.5 text-2xs font-mono uppercase tracking-widest text-white/30">
          Delivery Photo
        </p>
        {pod.photo_url ? (
          <div className="overflow-hidden rounded-lg border border-glass-border">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={pod.photo_url}
              alt="Proof of delivery photo"
              className="w-full object-cover"
              style={{ maxHeight: 260 }}
            />
          </div>
        ) : (
          <div className="flex h-28 items-center justify-center rounded-lg border border-glass-border bg-glass-100">
            <Camera size={24} className="text-white/20" />
          </div>
        )}
      </div>

      {/* Signature */}
      {pod.signature_url && (
        <div>
          <p className="mb-1.5 flex items-center gap-1.5 text-2xs font-mono uppercase tracking-widest text-white/30">
            <PenLine size={11} className="text-purple-plasma" />
            Signature
          </p>
          <div className="overflow-hidden rounded-lg border border-glass-border bg-white/5">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img
              src={pod.signature_url}
              alt="Recipient signature"
              className="mx-auto block object-contain"
              style={{ maxHeight: 120 }}
            />
          </div>
        </div>
      )}

      {/* Meta grid */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-2.5">
        {pod.recipient_name && (
          <MetaCell label="Signed by" value={pod.recipient_name} />
        )}
        {pod.captured_at && (
          <MetaCell label="Captured at" value={formatDate(pod.captured_at)} mono />
        )}
        {pod.capture_lat != null && pod.capture_lng != null && (
          <MetaCell
            label="GPS"
            value={`${pod.capture_lat.toFixed(5)}, ${pod.capture_lng.toFixed(5)}`}
            mono
          />
        )}
        {pod.otp_verified != null && (
          <MetaCell
            label="OTP"
            value={pod.otp_verified ? "Verified" : "Not used"}
            highlight={pod.otp_verified ? "green" : undefined}
          />
        )}
        {pod.cod_collected_cents != null && pod.cod_collected_cents > 0 && (
          <MetaCell
            label="COD collected"
            value={`₱${(pod.cod_collected_cents / 100).toLocaleString(undefined, { minimumFractionDigits: 2 })}`}
            mono
            highlight="amber"
          />
        )}
      </div>
    </GlassCard>
  );
}

interface MetaCellProps {
  label: string;
  value: string;
  mono?: boolean;
  highlight?: "green" | "amber";
}

function MetaCell({ label, value, mono, highlight }: MetaCellProps) {
  const cls = highlight === "green"
    ? "text-green-signal"
    : highlight === "amber"
    ? "text-amber-signal"
    : "text-white/70";

  return (
    <div className="min-w-0">
      <p className="text-2xs font-mono uppercase tracking-wider text-white/30">{label}</p>
      <p className={`mt-0.5 truncate text-xs ${mono ? "font-mono" : ""} ${cls}`}>{value}</p>
    </div>
  );
}
