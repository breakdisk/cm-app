"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Search, Package, Loader2, MapPin, Clock, CheckCircle2, XCircle, Truck, Zap } from "lucide-react";

interface TrackingEvent {
  status:    string;
  location:  string;
  timestamp: string;
  note?:     string;
}

interface TrackingResult {
  awb:                string;
  status:             string;
  origin:             string;
  destination:        string;
  estimated_delivery?: string;
  events:             TrackingEvent[];
}

const STATUS_ICONS: Record<string, React.ElementType> = {
  delivered:        CheckCircle2,
  failed:           XCircle,
  in_transit:       Truck,
  out_for_delivery: Truck,
  default:          Package,
};

export default function TrackPage() {
  const [awb, setAwb]         = useState("");
  const [loading, setLoading] = useState(false);
  const [result, setResult]   = useState<TrackingResult | null>(null);
  const [error, setError]     = useState<string | null>(null);

  async function handleSearch(e: React.FormEvent) {
    e.preventDefault();
    if (!awb.trim()) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const res = await fetch(
        `/api/v1/tracking/${awb.trim().toUpperCase()}`
      );
      if (res.status === 404) {
        setError("No shipment found for this tracking number.");
        return;
      }
      if (!res.ok) throw new Error("Server error");
      const data = await res.json();
      setResult(data);
    } catch {
      setError("Unable to fetch tracking info. Please try again.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen bg-[#050810] px-4 py-16">
      <div
        className="pointer-events-none fixed inset-0 opacity-20"
        style={{
          backgroundImage: "linear-gradient(rgba(0,229,255,0.04) 1px, transparent 1px), linear-gradient(90deg, rgba(0,229,255,0.04) 1px, transparent 1px)",
          backgroundSize: "48px 48px",
        }}
      />

      <div className="max-w-2xl mx-auto relative">
        <a href="/" className="flex items-center gap-2.5 mb-12">
          <div className="relative w-7 h-7 flex items-center justify-center">
            <div className="absolute inset-0 rounded-lg bg-gradient-to-br from-cyan-neon/30 to-purple-plasma/30" />
            <Zap className="w-3.5 h-3.5 text-cyan-neon relative z-10" strokeWidth={2.5} />
          </div>
          <span className="text-base font-bold tracking-tight" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
            <span className="bg-gradient-to-r from-cyan-neon to-purple-plasma bg-clip-text text-transparent">Cargo</span>
            <span className="text-white">Market</span>
          </span>
        </a>

        <h1 className="text-3xl font-bold text-white mb-2" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
          Track Shipment
        </h1>
        <p className="text-white/40 mb-8">Enter your AWB number to see real-time status</p>

        <form onSubmit={handleSearch} className="flex gap-3 mb-10">
          <input
            type="text"
            value={awb}
            onChange={(e) => setAwb(e.target.value)}
            placeholder="e.g. LS-A1B2C3D4"
            className="flex-1 rounded-xl border border-white/[0.08] bg-white/[0.04] px-5 py-4 text-base text-white placeholder:text-white/20 outline-none focus:border-cyan-neon/40 focus:bg-white/[0.06] transition-all font-mono"
          />
          <button
            type="submit"
            disabled={loading || !awb.trim()}
            className="rounded-xl bg-gradient-to-r from-cyan-neon to-purple-plasma px-6 py-4 text-[#050810] font-semibold hover:shadow-glow-cyan transition-all duration-300 disabled:opacity-50 flex items-center gap-2"
          >
            {loading ? <Loader2 className="h-5 w-5 animate-spin" /> : <Search className="h-5 w-5" />}
          </button>
        </form>

        {error && (
          <div className="rounded-xl border border-red-500/20 bg-red-500/10 px-5 py-4 text-sm text-red-400 mb-6">
            {error}
          </div>
        )}

        <AnimatePresence>
          {result && (
            <motion.div
              initial={{ opacity: 0, y: 16 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
            >
              <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] p-6 mb-4">
                <div className="flex items-start justify-between mb-4">
                  <div>
                    <div className="text-xs text-white/30 mb-1 font-mono">AWB</div>
                    <div className="text-xl font-bold text-white font-mono">{result.awb}</div>
                  </div>
                  <div className="rounded-full border border-cyan-neon/30 bg-cyan-neon/10 px-4 py-1.5 text-sm text-cyan-neon font-semibold capitalize">
                    {result.status.replace(/_/g, " ")}
                  </div>
                </div>
                <div className="flex gap-6 text-sm text-white/50">
                  <div className="flex items-center gap-1.5">
                    <MapPin className="h-3.5 w-3.5" />
                    {result.origin} → {result.destination}
                  </div>
                  {result.estimated_delivery && (
                    <div className="flex items-center gap-1.5">
                      <Clock className="h-3.5 w-3.5" />
                      ETA {result.estimated_delivery}
                    </div>
                  )}
                </div>
              </div>

              <div className="rounded-2xl border border-white/[0.08] bg-white/[0.03] p-6">
                <h2 className="text-sm font-semibold text-white/60 mb-5 uppercase tracking-wider">Tracking History</h2>
                <div className="relative">
                  <div className="absolute left-[9px] top-2 bottom-2 w-px bg-white/[0.06]" />
                  <div className="flex flex-col gap-5">
                    {result.events.map((event, i) => {
                      const Icon = STATUS_ICONS[event.status] ?? STATUS_ICONS.default;
                      return (
                        <div key={i} className="flex gap-4 relative">
                          <div className="flex-shrink-0 w-[18px] h-[18px] rounded-full border border-cyan-neon/40 bg-cyan-neon/10 flex items-center justify-center mt-0.5 z-10">
                            <Icon className="h-2.5 w-2.5 text-cyan-neon" />
                          </div>
                          <div>
                            <div className="text-sm font-medium text-white capitalize">{event.status.replace(/_/g, " ")}</div>
                            <div className="text-xs text-white/40 mt-0.5">{event.location} · {event.timestamp}</div>
                            {event.note && <div className="text-xs text-white/30 mt-0.5">{event.note}</div>}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <div className="mt-12 text-center">
          <p className="text-sm text-white/30 mb-3">Want to ship with CargoMarket?</p>
          <a
            href="/login?role=merchant"
            className="inline-flex items-center gap-2 rounded-xl bg-gradient-to-r from-cyan-neon to-purple-plasma px-6 py-3 text-sm font-semibold text-[#050810] hover:shadow-glow-cyan transition-all duration-300"
          >
            Get Started Free
          </a>
        </div>
      </div>
    </div>
  );
}
