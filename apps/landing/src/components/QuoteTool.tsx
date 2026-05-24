"use client";

/**
 * QuoteTool — public-facing quote wizard on the landing page.
 *
 * 5 steps:
 *   1. Freight mode  (Sea / Air)
 *   2. Origin        (dropdown from rate card)
 *   3. Box size      (visual cards + quantity)
 *   4. PH province   (text input with zone preview)
 *   5. Quote result  (line breakdown + Book CTA)
 *
 * All computation is client-side — no API call required.
 * Currency: all amounts in origin currency; PH delivery converted via static FX.
 */

import { useState, useCallback, useEffect } from "react";
import { X, Ship, Plane, Package, MapPin, ChevronRight, ChevronLeft, ArrowRight, Ruler, Calculator } from "lucide-react";
import {
  BOX_SIZES, SEA_CARGO, AIR_ZONES, computeQuote, resolveProvince,
  fmtAmount, type QuoteResult,
} from "@/lib/quote-engine";

// ── Design tokens (match globals.css + tailwind.config.ts) ────────────────────
const STEP_COLORS = ["text-cyan-neon", "text-purple-plasma", "text-green-signal", "text-amber-signal", "text-cyan-neon"];
const COMPONENT_COLOR: Record<string, string> = {
  sea:         "text-cyan-neon",
  air:         "text-purple-plasma",
  ph_delivery: "text-green-signal",
};

// ── Step indicator ─────────────────────────────────────────────────────────────
function StepDots({ current, total }: { current: number; total: number }) {
  return (
    <div className="flex items-center gap-2">
      {Array.from({ length: total }).map((_, i) => (
        <div
          key={i}
          className={`h-1.5 rounded-full transition-all duration-300 ${
            i < current
              ? "w-6 bg-cyan-neon"
              : i === current
              ? "w-8 bg-cyan-neon shadow-glow-cyan"
              : "w-3 bg-white/20"
          }`}
        />
      ))}
    </div>
  );
}

// ── Box size card ──────────────────────────────────────────────────────────────
function BoxSizeCard({
  size, selected, onSelect,
}: {
  size: typeof BOX_SIZES[number];
  selected: boolean;
  onSelect: () => void;
}) {
  // Scale the visual box proportionally
  const [l, w, h] = size.dims_cm;
  const maxDim = Math.max(l, w, h);
  const bW = Math.round((w / maxDim) * 56);
  const bH = Math.round((h / maxDim) * 40);

  return (
    <button
      onClick={onSelect}
      className={`relative flex flex-col items-center gap-2 p-3 rounded-xl border transition-all duration-200 cursor-pointer ${
        selected
          ? "border-cyan-neon/60 bg-cyan-neon/8 shadow-glow-cyan"
          : "border-white/10 bg-white/[0.03] hover:border-white/25 hover:bg-white/[0.05]"
      }`}
    >
      {/* Box shape SVG */}
      <div className="h-12 flex items-end justify-center">
        <svg width={bW + 12} height={bH + 8} viewBox={`0 0 ${bW + 12} ${bH + 8}`}>
          {/* Front face */}
          <rect x={6} y={4} width={bW} height={bH} rx={2}
            fill={selected ? "rgba(0,229,255,0.12)" : "rgba(255,255,255,0.06)"}
            stroke={selected ? "#00E5FF" : "rgba(255,255,255,0.2)"} strokeWidth={1} />
          {/* Top edge */}
          <path d={`M6,4 L10,0 L${bW + 10},0 L${bW + 6},4`}
            fill={selected ? "rgba(0,229,255,0.08)" : "rgba(255,255,255,0.04)"}
            stroke={selected ? "#00E5FF" : "rgba(255,255,255,0.15)"} strokeWidth={1} />
          {/* Right edge */}
          <path d={`M${bW + 6},4 L${bW + 10},0 L${bW + 10},${bH} L${bW + 6},${bH + 4}`}
            fill={selected ? "rgba(0,229,255,0.06)" : "rgba(255,255,255,0.03)"}
            stroke={selected ? "#00E5FF" : "rgba(255,255,255,0.12)"} strokeWidth={1} />
        </svg>
      </div>

      <div className="text-center">
        <div className={`text-sm font-bold ${selected ? "text-cyan-neon" : "text-white"}`}
          style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
          {size.name}
        </div>
        <div className="text-[10px] text-slate-500 font-mono mt-0.5">{size.dimensions}</div>
        <div className="text-[10px] text-slate-600 mt-0.5">≤ {size.max_weight_kg} kg · {size.cbm} m³</div>
      </div>

      {selected && (
        <div className="absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-cyan-neon flex items-center justify-center">
          <div className="w-2 h-2 rounded-full bg-[#050810]" />
        </div>
      )}
    </button>
  );
}

// ── Main component ─────────────────────────────────────────────────────────────

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export default function QuoteTool({ isOpen, onClose }: Props) {
  // ── Wizard state ─────────────────────────────────────────────────────────────
  const [step, setStep] = useState(0);
  const [mode, setMode] = useState<"sea" | "air">("sea");
  const [originKey, setOriginKey] = useState("");
  const [sizeId, setSizeId] = useState("jumbo");
  const [qty, setQty] = useState(1);
  const [weightKg, setWeightKg] = useState(20);
  const [province, setProvince] = useState("");
  const [result, setResult] = useState<QuoteResult | null>(null);

  // Reset on open
  useEffect(() => {
    if (isOpen) {
      setStep(0);
      setMode("sea");
      setOriginKey(SEA_CARGO[0].origin);
      setSizeId("jumbo");
      setQty(1);
      setWeightKg(20);
      setProvince("");
      setResult(null);
    }
  }, [isOpen]);

  // Update default origin when mode changes
  useEffect(() => {
    setOriginKey(mode === "sea" ? SEA_CARGO[0].origin : AIR_ZONES[0].zone_name);
  }, [mode]);

  const origins = mode === "sea" ? SEA_CARGO.map(r => r.origin) : AIR_ZONES.map(z => z.zone_name);

  const provinceResolved = resolveProvince(province);

  const canAdvance = useCallback(() => {
    if (step === 0) return true;
    if (step === 1) return !!originKey;
    if (step === 2) return !!sizeId && qty >= 1;
    if (step === 3) return true; // province optional
    return false;
  }, [step, originKey, sizeId, qty]);

  const handleNext = useCallback(() => {
    if (step === 3) {
      // Compute quote
      const size = BOX_SIZES.find(s => s.id === sizeId);
      const dims: [number, number, number] = size ? size.dims_cm : [0, 0, 0];
      const r = computeQuote(mode, originKey, sizeId, qty, weightKg, dims, province);
      setResult(r);
      setStep(4);
    } else {
      setStep(s => s + 1);
    }
  }, [step, mode, originKey, sizeId, qty, weightKg, province]);

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  }, [onClose]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-[#050810]/85 backdrop-blur-md"
        onClick={onClose}
      />

      {/* Modal */}
      <div className="relative w-full max-w-2xl max-h-[90vh] flex flex-col glass-panel-bright rounded-2xl border border-white/10 overflow-hidden shadow-[0_0_80px_rgba(0,229,255,0.08),0_40px_120px_rgba(0,0,0,0.9)]">

        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-white/[0.06]">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-lg bg-cyan-neon/10 border border-cyan-neon/20 flex items-center justify-center">
              <Calculator className="w-4 h-4 text-cyan-neon" />
            </div>
            <div>
              <div className="text-white font-semibold text-sm" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                Balikbayan Box Quote
              </div>
              <div className="text-xs text-slate-500">Instant price estimate · No sign-up required</div>
            </div>
          </div>
          <div className="flex items-center gap-4">
            <StepDots current={step} total={5} />
            <button
              onClick={onClose}
              className="w-7 h-7 rounded-lg glass-panel border border-white/10 flex items-center justify-center text-slate-400 hover:text-white hover:border-white/25 transition-colors"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>

        {/* Body — scrollable */}
        <div className="flex-1 overflow-y-auto p-6">

          {/* ── Step 0: Freight mode ───────────────────────────────────────── */}
          {step === 0 && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  How are you sending?
                </h2>
                <p className="text-sm text-slate-400">Choose your preferred shipping method.</p>
              </div>
              <div className="grid grid-cols-2 gap-4">
                {(["sea", "air"] as const).map(m => (
                  <button
                    key={m}
                    onClick={() => setMode(m)}
                    className={`flex flex-col items-center gap-4 p-6 rounded-2xl border transition-all duration-200 ${
                      mode === m
                        ? m === "sea"
                          ? "border-cyan-neon/50 bg-cyan-neon/6 shadow-glow-cyan"
                          : "border-purple-plasma/50 bg-purple-plasma/6 shadow-glow-purple"
                        : "border-white/10 bg-white/[0.03] hover:bg-white/[0.06]"
                    }`}
                  >
                    {m === "sea"
                      ? <Ship className={`w-10 h-10 ${mode === "sea" ? "text-cyan-neon" : "text-slate-500"}`} />
                      : <Plane className={`w-10 h-10 ${mode === "air" ? "text-purple-plasma" : "text-slate-500"}`} />
                    }
                    <div className="text-center">
                      <div className={`text-base font-bold ${mode === m ? (m === "sea" ? "text-cyan-neon" : "text-purple-plasma") : "text-white"}`}
                        style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                        {m === "sea" ? "Sea Cargo" : "Air Cargo"}
                      </div>
                      <div className="text-xs text-slate-500 mt-1">
                        {m === "sea" ? "30–60 days · Most economical" : "2–14 days · Fastest delivery"}
                      </div>
                    </div>
                  </button>
                ))}
              </div>
              <div className="glass-panel rounded-xl p-3 border border-white/[0.06]">
                <div className="flex items-start gap-2.5">
                  <Package className="w-4 h-4 text-amber-signal mt-0.5 shrink-0" />
                  <p className="text-xs text-slate-400">
                    <span className="text-amber-signal font-medium">Sea cargo</span> is the traditional Balikbayan box shipping method —
                    affordable for large boxes sent overseas to the Philippines.
                    Air cargo is faster but priced by chargeable weight.
                  </p>
                </div>
              </div>
            </div>
          )}

          {/* ── Step 1: Origin ─────────────────────────────────────────────── */}
          {step === 1 && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  {mode === "sea" ? "Where are you shipping from?" : "Select your region"}
                </h2>
                <p className="text-sm text-slate-400">
                  {mode === "sea" ? "Pick the city/state closest to your location." : "Choose the zone that covers your country."}
                </p>
              </div>
              <div className="relative">
                <select
                  value={originKey}
                  onChange={e => setOriginKey(e.target.value)}
                  className="w-full px-4 py-3 rounded-xl border border-white/10 bg-white/[0.04] text-white text-sm appearance-none outline-none focus:border-cyan-neon/40 cursor-pointer"
                  style={{ fontFamily: "'Space Grotesk', sans-serif" }}
                >
                  {origins.map(o => (
                    <option key={o} value={o} className="bg-[#0a0f1e]">{o}</option>
                  ))}
                </select>
                <ChevronRight className="absolute right-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500 rotate-90 pointer-events-none" />
              </div>

              {/* Origin detail card */}
              {mode === "sea" && (() => {
                const row = SEA_CARGO.find(r => r.origin === originKey);
                return row ? (
                  <div className="glass-panel rounded-xl p-4 border border-cyan-neon/10">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs text-slate-500">Transit time</span>
                      <span className="text-xs font-mono text-cyan-neon">{row.transit_days}</span>
                    </div>
                    <div className="flex items-center justify-between">
                      <span className="text-xs text-slate-500">Billed in</span>
                      <span className="text-xs font-mono text-white">{row.currency}</span>
                    </div>
                  </div>
                ) : null;
              })()}
              {mode === "air" && (() => {
                const zone = AIR_ZONES.find(z => z.zone_name === originKey);
                return zone ? (
                  <div className="glass-panel rounded-xl p-4 border border-purple-plasma/10">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs text-slate-500">Transit time</span>
                      <span className="text-xs font-mono text-purple-plasma">{zone.transit_days}</span>
                    </div>
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs text-slate-500">Rate per kg</span>
                      <span className="text-xs font-mono text-white">${zone.rate_per_kg.toFixed(2)} USD</span>
                    </div>
                    <div className="text-xs text-slate-600 mt-2 leading-relaxed">{zone.origins}</div>
                  </div>
                ) : null;
              })()}
            </div>
          )}

          {/* ── Step 2: Box size ───────────────────────────────────────────── */}
          {step === 2 && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  Select your box size
                </h2>
                <p className="text-sm text-slate-400">
                  Choose the standard size closest to your box.
                  {mode === "air" && " For air, you can also enter exact dimensions."}
                </p>
              </div>

              <div className="grid grid-cols-3 gap-3">
                {BOX_SIZES.map(size => (
                  <BoxSizeCard
                    key={size.id}
                    size={size}
                    selected={sizeId === size.id}
                    onSelect={() => setSizeId(size.id)}
                  />
                ))}
              </div>

              {/* Quantity + weight row */}
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="text-xs text-slate-500 mb-2 block">Number of boxes</label>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => setQty(q => Math.max(1, q - 1))}
                      className="w-8 h-8 rounded-lg glass-panel border border-white/10 text-white hover:border-white/25 flex items-center justify-center text-lg leading-none"
                    >−</button>
                    <span className="w-8 text-center text-white font-mono text-sm">{qty}</span>
                    <button
                      onClick={() => setQty(q => Math.min(20, q + 1))}
                      className="w-8 h-8 rounded-lg glass-panel border border-white/10 text-white hover:border-white/25 flex items-center justify-center text-lg leading-none"
                    >+</button>
                  </div>
                </div>
                {mode === "air" && (
                  <div>
                    <label className="text-xs text-slate-500 mb-2 block">Actual weight (kg)</label>
                    <input
                      type="number"
                      min={1}
                      max={100}
                      value={weightKg}
                      onChange={e => setWeightKg(Number(e.target.value))}
                      className="w-full px-3 py-2 rounded-lg border border-white/10 bg-white/[0.04] text-white text-sm font-mono outline-none focus:border-cyan-neon/40"
                    />
                  </div>
                )}
              </div>

              {/* Selected size summary */}
              {(() => {
                const size = BOX_SIZES.find(s => s.id === sizeId);
                return size ? (
                  <div className="glass-panel rounded-xl p-3 border border-white/[0.06]">
                    <div className="flex items-center gap-3">
                      <Ruler className="w-4 h-4 text-cyan-neon shrink-0" />
                      <div className="text-xs text-slate-400">
                        <span className="text-white font-medium">{size.name}</span>
                        {" · "}{size.dimensions}
                        {" · "}<span className="text-cyan-neon font-mono">{size.cbm} m³</span>
                        {" · "}max {size.max_weight_kg} kg
                        {qty > 1 && <span className="text-amber-signal"> × {qty} boxes = {(size.cbm * qty).toFixed(4)} m³ total</span>}
                      </div>
                    </div>
                  </div>
                ) : null;
              })()}
            </div>
          )}

          {/* ── Step 3: Province ───────────────────────────────────────────── */}
          {step === 3 && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  Where in the Philippines?
                </h2>
                <p className="text-sm text-slate-400">
                  Enter the recipient's province or city to include local delivery cost.
                </p>
              </div>

              <div className="relative">
                <MapPin className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-500" />
                <input
                  type="text"
                  placeholder="e.g. Cebu, Davao, Metro Manila…"
                  value={province}
                  onChange={e => setProvince(e.target.value)}
                  className="w-full pl-9 pr-4 py-3 rounded-xl border border-white/10 bg-white/[0.04] text-white placeholder-slate-600 text-sm outline-none focus:border-cyan-neon/40"
                />
              </div>

              {province && (
                <div className={`glass-panel rounded-xl p-4 border ${provinceResolved ? "border-green-signal/20" : "border-amber-signal/20"}`}>
                  {provinceResolved ? (
                    <div className="space-y-2">
                      <div className="flex items-center gap-2">
                        <div className="w-2 h-2 rounded-full bg-green-signal" />
                        <span className="text-xs text-green-signal font-medium">Zone matched</span>
                      </div>
                      <div className="text-sm text-white font-medium">
                        {PH_ZONES_MAP[provinceResolved.zone_code] ?? provinceResolved.zone_code}
                      </div>
                      <div className="text-xs text-slate-500">{provinceResolved.zone_code}</div>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 rounded-full bg-amber-signal animate-pulse" />
                      <span className="text-xs text-amber-signal">Province not found — quote will exclude local delivery</span>
                    </div>
                  )}
                </div>
              )}

              {!province && (
                <div className="glass-panel rounded-xl p-3 border border-white/[0.06]">
                  <p className="text-xs text-slate-500">
                    You can skip this step. The quote will show freight costs only. PH local delivery pricing is added automatically when a province is matched.
                  </p>
                </div>
              )}
            </div>
          )}

          {/* ── Step 4: Result ─────────────────────────────────────────────── */}
          {step === 4 && result && (
            <div className="space-y-6">
              <div>
                <h2 className="text-xl font-bold text-white mb-1" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  Your estimate is ready
                </h2>
                <p className="text-sm text-slate-400">
                  Indicative price · Final invoice may vary by ±5%
                  {result.transit_days && <> · <span className="text-cyan-neon">{result.transit_days} transit</span></>}
                </p>
              </div>

              {/* Line items */}
              <div className="space-y-2">
                {result.lines.map((line, i) => (
                  <div key={i} className="glass-panel rounded-xl px-4 py-3 border border-white/[0.06] flex items-start justify-between gap-3">
                    <div className="flex-1 min-w-0">
                      <div className={`text-sm font-medium ${COMPONENT_COLOR[line.component] ?? "text-white"}`}>
                        {line.label}
                      </div>
                      {line.note && (
                        <div className="text-xs text-slate-600 mt-0.5 truncate">{line.note}</div>
                      )}
                    </div>
                    <div className="text-sm font-mono font-bold text-white shrink-0">
                      {fmtAmount(line.amount, line.currency)}
                    </div>
                  </div>
                ))}
              </div>

              {/* Total */}
              <div className="rounded-2xl border border-cyan-neon/20 bg-cyan-neon/5 shadow-glow-cyan p-4 flex items-center justify-between">
                <div>
                  <div className="text-xs text-slate-400">Estimated Total</div>
                  <div className="text-xs text-slate-600 mt-0.5">All-in · {result.origin_currency} · incl. PH delivery</div>
                </div>
                <div className="text-2xl font-bold text-cyan-neon" style={{ fontFamily: "'Space Grotesk', sans-serif" }}>
                  {fmtAmount(result.total_origin_currency, result.origin_currency)}
                </div>
              </div>

              {/* CBM info */}
              {result.cbm > 0 && (
                <div className="glass-panel rounded-xl p-3 border border-white/[0.06]">
                  <div className="flex items-center justify-between text-xs">
                    <span className="text-slate-500">Box volume (CBM)</span>
                    <span className="font-mono text-purple-plasma">{result.cbm} m³</span>
                  </div>
                </div>
              )}

              {/* Book CTA */}
              <div className="space-y-3 pt-2">
                <a
                  href="/setup"
                  className="group w-full flex items-center justify-center gap-2 px-6 py-3.5 rounded-xl text-sm font-semibold bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:shadow-glow-cyan transition-all duration-300 hover:scale-[1.02]"
                >
                  Book This Shipment
                  <ArrowRight className="w-4 h-4 group-hover:translate-x-1 transition-transform" />
                </a>
                <button
                  onClick={() => { setStep(0); setResult(null); }}
                  className="w-full px-6 py-2.5 rounded-xl text-sm text-slate-400 hover:text-white glass-panel border border-white/10 hover:border-white/20 transition-all"
                >
                  Start a new quote
                </button>
              </div>

              {/* Disclaimer */}
              <p className="text-[10px] text-slate-700 text-center">
                Rates are indicative estimates based on standard Balikbayan box sizes.
                PH delivery amounts are converted from PHP using indicative FX rates.
                Final invoice issued at time of booking.
              </p>
            </div>
          )}
        </div>

        {/* Footer nav (steps 0–3) */}
        {step < 4 && (
          <div className="px-6 py-4 border-t border-white/[0.06] flex items-center justify-between">
            <button
              onClick={() => step === 0 ? onClose() : setStep(s => s - 1)}
              className="flex items-center gap-1.5 text-sm text-slate-400 hover:text-white transition-colors"
            >
              <ChevronLeft className="w-4 h-4" />
              {step === 0 ? "Cancel" : "Back"}
            </button>
            <button
              disabled={!canAdvance()}
              onClick={handleNext}
              className={`flex items-center gap-2 px-5 py-2.5 rounded-xl text-sm font-semibold transition-all duration-200 ${
                canAdvance()
                  ? "bg-gradient-to-r from-cyan-neon to-purple-plasma text-[#050810] hover:shadow-glow-cyan hover:scale-[1.02]"
                  : "bg-white/10 text-slate-600 cursor-not-allowed"
              }`}
            >
              {step === 3 ? "Calculate Price" : "Next"}
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// Inline zone name map for the province step display
const PH_ZONES_MAP: Record<string, string> = {
  "Zone 1A": "Metro Manila / NCR",
  "Zone 2A": "Region III — Bulacan / Pampanga",
  "Zone 2B": "CALABARZON",
  "Zone 2C": "Quezon Province / Lucena",
  "Zone 3A": "Ilocos / La Union / Pangasinan",
  "Zone 3B": "Cagayan Valley",
  "Zone 3C": "CAR — Baguio / Cordillera",
  "Zone 4A": "Bicol — Camarines / Naga",
  "Zone 4B": "Bicol — Albay / Sorsogon",
  "Zone 4C": "MIMAROPA",
  "Zone 4D": "Palawan — Puerto Princesa",
  "Zone 5A": "Metro Cebu",
  "Zone 5B": "Cebu Province",
  "Zone 5C": "Iloilo / Bacolod City",
  "Zone 5D": "Western Visayas — Provinces",
  "Zone 5E": "Negros Oriental / Bohol",
  "Zone 5F": "Eastern Visayas — Leyte / Samar",
  "Zone 5G": "Aklan / Antique / Capiz",
  "Zone 6A": "Metro Davao",
  "Zone 6B": "Davao Region — Provinces",
  "Zone 6C": "Cagayan de Oro / Bukidnon",
  "Zone 6D": "GenSan / Sarangani / S. Cotabato",
  "Zone 6E": "Zamboanga",
  "Zone 6F": "Lanao / Iligan — BARMM",
  "Zone 6G": "Cotabato / Maguindanao — BARMM",
  "Zone 6H": "Caraga — Surigao / Agusan",
  "Zone 7A": "Sulu / Tawi-Tawi / Basilan",
  "Zone 7B": "Batanes",
};
