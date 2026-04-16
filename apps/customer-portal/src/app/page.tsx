"use client";
/**
 * Customer Portal — Tracking Home
 * Public page: enter AWB / tracking number → see live status.
 */
import { useState, useTransition } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Package, Search, MapPin, Clock, CheckCircle2, XCircle, AlertCircle, Truck } from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

interface StatusEvent {
  status:      string;
  description: string;
  location?:   string;
  occurred_at: string;
}

interface TrackingResult {
  tracking_number: string;
  status:          string;
  origin_city:     string;
  destination_city: string;
  eta?:            string;
  timeline:        StatusEvent[];
  driver_name?:    string;
  driver_lat?:     number;
  driver_lng?:     number;
}

// ── API ───────────────────────────────────────────────────────────────────────

const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8000";

async function fetchTracking(tn: string): Promise<TrackingResult | null> {
  try {
    const res = await fetch(`${API_BASE}/v1/tracking/public/${encodeURIComponent(tn)}`, {
      cache: "no-store",
    });
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const json = await res.json();
    const d = json.data;
    return {
      tracking_number:   d.tracking_number,
      status:            d.status,
      origin_city:       d.origin_city,
      destination_city:  d.destination_city,
      eta:               d.eta,
      driver_name:       d.driver?.name,
      driver_lat:        d.driver?.lat,
      driver_lng:        d.driver?.lng,
      timeline:          (d.events ?? []).map((e: { status: string; description: string; location?: string; occurred_at: string }) => ({
        status:      e.status,
        description: e.description,
        location:    e.location,
        occurred_at: e.occurred_at,
      })),
    };
  } catch {
    return null;
  }
}

// ── Status config ─────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<string, { label: string; color: string; icon: React.ElementType }> = {
  pending:            { label: "Pending",           color: "#FFAB00", icon: Clock },
  confirmed:          { label: "Confirmed",         color: "#00E5FF", icon: CheckCircle2 },
  picked_up:          { label: "Picked Up",         color: "#00E5FF", icon: Package },
  in_transit:         { label: "In Transit",        color: "#A855F7", icon: Truck },
  at_hub:             { label: "At Hub",            color: "#A855F7", icon: MapPin },
  out_for_delivery:   { label: "Out for Delivery",  color: "#00FF88", icon: Truck },
  delivered:          { label: "Delivered",         color: "#00FF88", icon: CheckCircle2 },
  failed:             { label: "Delivery Failed",   color: "#FF3B5C", icon: XCircle },
  cancelled:          { label: "Cancelled",         color: "#6B7280", icon: XCircle },
  returned:           { label: "Returned",          color: "#FFAB00", icon: AlertCircle },
};

function getStatus(key: string) {
  return STATUS_CONFIG[key] ?? { label: key, color: "#00E5FF", icon: Package };
}

// ── Components ────────────────────────────────────────────────────────────────

function TimelineStep({ event, isLast }: { event: StatusEvent; isLast: boolean }) {
  const cfg = getStatus(event.status);
  const Icon = cfg.icon;
  const date = new Date(event.occurred_at);

  return (
    <div className="flex gap-4">
      {/* Line + dot */}
      <div className="flex flex-col items-center">
        <div
          className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full border"
          style={{ borderColor: `${cfg.color}40`, background: `${cfg.color}12`, boxShadow: `0 0 10px ${cfg.color}30` }}
        >
          <Icon size={14} style={{ color: cfg.color }} />
        </div>
        {!isLast && (
          <div className="mt-1 flex-1 w-px bg-white/10 min-h-[24px]" />
        )}
      </div>
      {/* Content */}
      <div className="pb-6 min-w-0">
        <p className="text-sm font-medium text-white">{event.description}</p>
        {event.location && (
          <p className="text-xs text-white/40 flex items-center gap-1 mt-0.5">
            <MapPin size={10} /> {event.location}
          </p>
        )}
        <p className="text-2xs font-mono text-white/25 mt-1">
          {date.toLocaleDateString("en-PH", { month: "short", day: "numeric" })} ·{" "}
          {date.toLocaleTimeString("en-PH", { hour: "2-digit", minute: "2-digit" })}
        </p>
      </div>
    </div>
  );
}

function TrackingCard({ result }: { result: TrackingResult }) {
  const cfg = getStatus(result.status);
  const Icon = cfg.icon;

  return (
    <motion.div
      initial={{ opacity: 0, y: 24 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ type: "spring", stiffness: 300, damping: 30 }}
      className="mt-8 overflow-hidden rounded-2xl border border-white/10"
      style={{ background: "rgba(255,255,255,0.04)", backdropFilter: "blur(16px)" }}
    >
      {/* Status bar */}
      <div
        className="px-6 py-4 flex items-center justify-between border-b border-white/08"
        style={{ background: `${cfg.color}10`, borderTop: `2px solid ${cfg.color}60` }}
      >
        <div className="flex items-center gap-3">
          <div
            className="flex h-10 w-10 items-center justify-center rounded-xl"
            style={{ background: `${cfg.color}20`, boxShadow: `0 0 16px ${cfg.color}40` }}
          >
            <Icon size={18} style={{ color: cfg.color }} />
          </div>
          <div>
            <p className="text-xs font-mono text-white/40 uppercase tracking-widest">Current Status</p>
            <p className="font-heading font-semibold text-white text-lg leading-tight">{cfg.label}</p>
          </div>
        </div>
        {result.eta && (
          <div className="text-right">
            <p className="text-xs font-mono text-white/40">ETA</p>
            <p className="text-sm font-medium text-white">{result.eta}</p>
          </div>
        )}
      </div>

      {/* Tracking number + route */}
      <div className="px-6 py-4 flex items-center gap-4 border-b border-white/06">
        <div>
          <p className="text-xs font-mono text-white/30">Tracking Number</p>
          <p className="font-mono text-cyan-neon font-bold tracking-widest">{result.tracking_number}</p>
        </div>
        <div className="flex-1 flex items-center gap-2 justify-center">
          <span className="text-xs text-white/40">{result.origin_city}</span>
          <div className="flex-1 h-px bg-gradient-to-r from-white/10 via-cyan-neon/30 to-white/10" />
          <span className="text-xs text-white/40">{result.destination_city}</span>
        </div>
        {result.driver_name && (
          <div className="text-right">
            <p className="text-xs font-mono text-white/30">Courier</p>
            <p className="text-xs font-medium text-white">{result.driver_name}</p>
          </div>
        )}
      </div>

      {/* Timeline */}
      <div className="px-6 pt-6">
        <p className="text-xs font-mono uppercase tracking-widest text-white/30 mb-5">Shipment Timeline</p>
        {[...result.timeline].reverse().map((event, i) => (
          <TimelineStep key={i} event={event} isLast={i === result.timeline.length - 1} />
        ))}
      </div>

      {/* CTA row */}
      <div className="px-6 pb-6 flex gap-3">
        {result.status !== "delivered" && result.status !== "cancelled" && (
          <a
            href={`/reschedule?tn=${result.tracking_number}`}
            className="flex-1 rounded-xl border border-white/10 py-2.5 text-sm text-center text-white/60 hover:text-white hover:border-white/20 transition-colors"
          >
            Reschedule Delivery
          </a>
        )}
        {result.status === "delivered" && (
          <a
            href={`/feedback?tn=${result.tracking_number}`}
            className="flex-1 rounded-xl border border-green-signal/20 py-2.5 text-sm text-center text-green-signal/70 hover:text-green-signal hover:border-green-signal/40 transition-colors"
          >
            Rate Your Delivery
          </a>
        )}
        <a
          href="https://wa.me/639000000000"
          target="_blank"
          rel="noreferrer"
          className="rounded-xl border border-white/10 px-4 py-2.5 text-sm text-white/60 hover:text-white hover:border-white/20 transition-colors"
        >
          Support
        </a>
      </div>
    </motion.div>
  );
}

// ── Main page ─────────────────────────────────────────────────────────────────

export default function TrackingPage() {
  const [query, setQuery]     = useState("");
  const [result, setResult]   = useState<TrackingResult | null>(null);
  const [notFound, setNotFound] = useState(false);
  const [isPending, startTransition] = useTransition();

  function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    const tn = query.trim().toUpperCase();
    if (!tn) return;
    setResult(null);
    setNotFound(false);
    startTransition(async () => {
      const data = await fetchTracking(tn);
      if (data) { setResult(data); }
      else      { setNotFound(true); }
    });
  }

  return (
    <div className="mx-auto max-w-2xl px-4 py-16">
      {/* Hero */}
      <motion.div
        initial={{ opacity: 0, y: -16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ type: "spring", stiffness: 260, damping: 25 }}
        className="text-center mb-10"
      >
        <div
          className="mx-auto mb-4 flex h-14 w-14 items-center justify-center rounded-2xl"
          style={{ background: "linear-gradient(135deg, rgba(0,229,255,0.2), rgba(168,85,247,0.2))", border: "1px solid rgba(0,229,255,0.2)" }}
        >
          <Package size={22} style={{ color: "#00E5FF" }} />
        </div>
        <h1 className="font-heading text-3xl font-bold text-white mb-2">Track Your Delivery</h1>
        <p className="text-sm text-white/40">
          Enter your tracking number to see real-time updates
        </p>
      </motion.div>

      {/* Search form */}
      <form onSubmit={handleSearch} className="flex gap-2">
        <div
          className="flex flex-1 items-center gap-3 rounded-xl border px-4 py-3 transition-all"
          style={{
            background: "rgba(255,255,255,0.04)",
            backdropFilter: "blur(12px)",
            borderColor: query ? "rgba(0,229,255,0.4)" : "rgba(255,255,255,0.08)",
          }}
        >
          <Search size={16} className="text-white/30 flex-shrink-0" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="CM-PH1-S0000001A"
            className="flex-1 bg-transparent text-sm font-mono text-white placeholder:text-white/20 outline-none uppercase"
            spellCheck={false}
          />
        </div>
        <button
          type="submit"
          disabled={isPending || !query.trim()}
          className="rounded-xl px-6 py-3 text-sm font-medium text-canvas transition-all disabled:opacity-50"
          style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
        >
          {isPending ? "…" : "Track"}
        </button>
      </form>

      {/* Hint */}
      <p className="mt-2 text-center text-2xs font-mono text-white/20">
        Tracking numbers start with LS- (e.g. LS-A1B2C3D4)
      </p>

      {/* Result */}
      <AnimatePresence mode="wait">
        {result && <TrackingCard key={result.tracking_number} result={result} />}

        {notFound && (
          <motion.div
            key="not-found"
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            className="mt-8 rounded-2xl border border-red-signal/20 bg-red-surface px-6 py-8 text-center"
          >
            <XCircle size={32} className="mx-auto mb-3 text-red-signal/60" />
            <p className="font-heading font-semibold text-white">Tracking number not found</p>
            <p className="mt-1 text-sm text-white/40">
              Check the number and try again, or contact the merchant.
            </p>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
