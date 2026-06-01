"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  Truck, Store, CheckCircle2, ChevronRight, Loader2, X,
} from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { createFleetApi, type RegisterVehiclePayload, type VehicleType } from "@/lib/api/fleet";
import {
  createVehicleListing,
  type SizeClass,
  type CreateListingPayload,
} from "@/lib/api/marketplace";

// ── Types ──────────────────────────────────────────────────────────────────────

interface Step1Fields {
  plate_number: string;
  vehicle_type: VehicleType;
  make:         string;
  model:        string;
  year:         string;
  color:        string;
}

interface Step2Fields {
  list_on_marketplace:          boolean;
  size_class:                   SizeClass;
  max_weight_kg:                string;
  base_price_php:               string;
  per_km_php:                   string;
  service_area_label:           string;
  idle_from:                    string;
  idle_until:                   string;
}

interface RegisterResult {
  vehicle_id: string;
  plate:      string;
  listed:     boolean;
}

interface Props {
  open:      boolean;
  onClose:   () => void;
  onSuccess: () => void;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const VEHICLE_TYPES: VehicleType[] = ["Motorcycle", "Van", "Truck", "Bicycle", "Car"];

const TYPE_TO_SIZE_CLASS: Record<VehicleType, SizeClass> = {
  Motorcycle: "motorcycle",
  Van:        "van",
  Truck:      "7ton",
  Bicycle:    "scooter_bicycle",
  Car:        "sedan",
};

const SIZE_CLASSES: { value: SizeClass; label: string }[] = [
  { value: "scooter_bicycle",    label: "Scooter / Bicycle" },
  { value: "motorcycle",         label: "Motorcycle" },
  { value: "sedan",              label: "Sedan" },
  { value: "van",                label: "Van" },
  { value: "1ton",               label: "1 Ton" },
  { value: "3ton",               label: "3 Ton" },
  { value: "7ton",               label: "7 Ton" },
  { value: "10ton",              label: "10 Ton" },
  { value: "trailer",            label: "Trailer" },
  { value: "refrigerated_truck", label: "Refrigerated Truck" },
  { value: "recovery_truck",     label: "Recovery Truck" },
];

const STEP_LABELS = ["Vehicle Identity", "Marketplace Listing"];

const THIS_YEAR = new Date().getFullYear();

function defaultStep2(): Step2Fields {
  const now = new Date();
  const end = new Date(now.getTime() + 6 * 3_600_000);
  return {
    list_on_marketplace:        true,
    size_class:                 "van",
    max_weight_kg:              "800",
    base_price_php:             "900",
    per_km_php:                 "18",
    service_area_label:         "Metro Manila",
    idle_from:                  now.toISOString().slice(0, 16),
    idle_until:                 end.toISOString().slice(0, 16),
  };
}

// ── Shared styles ─────────────────────────────────────────────────────────────

const inputCls =
  "w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 focus:ring-cyan-neon/20 " +
  "transition-all font-mono";

const labelCls = "block text-xs text-white/50 mb-1.5 font-medium";

// ── Component ──────────────────────────────────────────────────────────────────

export function RegisterVehicleModal({ open, onClose, onSuccess }: Props) {
  const [step,    setStep]    = useState<1 | 2 | 3>(1);
  const [loading, setLoading] = useState(false);
  const [error,   setError]   = useState<string | null>(null);

  const [s1, setS1] = useState<Step1Fields>({
    plate_number: "", vehicle_type: "Van",
    make: "", model: "", year: String(THIS_YEAR), color: "",
  });
  const [s2, setS2] = useState<Step2Fields>(defaultStep2);
  const [result, setResult] = useState<RegisterResult | null>(null);

  function reset() {
    setStep(1); setLoading(false); setError(null); setResult(null);
    setS1({ plate_number: "", vehicle_type: "Van", make: "", model: "", year: String(THIS_YEAR), color: "" });
    setS2(defaultStep2());
  }

  function handleClose() { reset(); onClose(); }

  function handleTypeChange(type: VehicleType) {
    setS1((p) => ({ ...p, vehicle_type: type }));
    setS2((p) => ({ ...p, size_class: TYPE_TO_SIZE_CLASS[type] }));
  }

  // ── Step 1: Register vehicle in fleet service ────────────────────────────────
  async function handleStep1(e: React.FormEvent) {
    e.preventDefault();
    const year = parseInt(s1.year, 10);
    if (!s1.plate_number.trim())                                         { setError("Plate number is required.");                                return; }
    if (!s1.make.trim() || !s1.model.trim())                             { setError("Make and model are required.");                             return; }
    if (isNaN(year) || year < 1990 || year > THIS_YEAR + 1)             { setError(`Year must be between 1990 and ${THIS_YEAR + 1}.`);         return; }
    if (!s1.color.trim())                                                { setError("Color is required.");                                       return; }

    setError(null); setLoading(true);
    try {
      const api = createFleetApi();
      const payload: RegisterVehiclePayload = {
        plate_number: s1.plate_number.toUpperCase().trim(),
        vehicle_type: s1.vehicle_type,
        make:         s1.make.trim(),
        model:        s1.model.trim(),
        year,
        color:        s1.color.trim(),
      };
      const res = await api.registerVehicle(payload);
      setResult({ vehicle_id: res.data.id, plate: res.data.plate, listed: false });
      setStep(2);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to register vehicle. Check plate uniqueness.");
    } finally {
      setLoading(false);
    }
  }

  // ── Step 2: Optionally list on marketplace ───────────────────────────────────
  async function handleStep2(e: React.FormEvent) {
    e.preventDefault();
    if (!result) return;

    if (!s2.list_on_marketplace) {
      setStep(3);
      onSuccess();
      return;
    }

    const weightKg  = parseFloat(s2.max_weight_kg);
    const baseCents = Math.round(parseFloat(s2.base_price_php)  * 100);
    const kmCents   = Math.round(parseFloat(s2.per_km_php)      * 100);

    if (isNaN(weightKg)  || weightKg  <= 0) { setError("Max weight must be a positive number.");  return; }
    if (isNaN(baseCents) || baseCents <= 0) { setError("Base price must be a positive number.");   return; }
    if (isNaN(kmCents)   || kmCents   <= 0) { setError("Per-km rate must be a positive number."); return; }
    if (!s2.service_area_label.trim())      { setError("Service area is required.");               return; }
    if (new Date(s2.idle_until) <= new Date(s2.idle_from)) {
      setError("'Available until' must be after 'Available from'."); return;
    }

    setError(null); setLoading(true);
    try {
      const listing: CreateListingPayload = {
        vehicle_plate:                result.plate,
        size_class:                   s2.size_class,
        features:                     [],
        max_weight_kg:                weightKg,
        max_volume_m3:                null,
        base_price_cents:             baseCents,
        per_km_cents:                 kmCents,
        per_kg_cents:                 null,
        service_area_label:           s2.service_area_label.trim(),
        idle_from:                    new Date(s2.idle_from).toISOString(),
        idle_until:                   new Date(s2.idle_until).toISOString(),
        status:                       "active",
        carrier_response_window_mins: 15,
      };
      await createVehicleListing(listing);
      setResult((p) => p ? { ...p, listed: true } : null);
      setStep(3);
      onSuccess();
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(
        e.message?.includes("carrier") || e.message?.includes("401") || e.message?.includes("403")
          ? "Vehicle registered. Marketplace listing requires carrier portal access — list it from the Partner Portal."
          : (e.message ?? "Marketplace listing failed. The vehicle was registered successfully."),
      );
    } finally {
      setLoading(false);
    }
  }

  if (!open) return null;

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ background: "rgba(5,8,16,0.85)", backdropFilter: "blur(8px)" }}
          onClick={(e) => { if (e.target === e.currentTarget) handleClose(); }}
        >
          <motion.div
            initial={{ scale: 0.95, opacity: 0, y: 16 }}
            animate={{ scale: 1, opacity: 1, y: 0 }}
            exit={{ scale: 0.95, opacity: 0, y: 16 }}
            transition={{ type: "spring", duration: 0.4 }}
            className="w-full max-w-lg"
          >
            <GlassCard glow="cyan" className="relative">

              {/* ── Header ───────────────────────────────────────────────── */}
              <div className="flex items-center justify-between mb-6">
                <div>
                  <h2 className="font-heading text-lg font-bold text-white">Register Vehicle</h2>
                  <p className="text-xs text-white/40 mt-0.5">
                    {step < 3
                      ? `Step ${step} of 2 — ${STEP_LABELS[step - 1]}`
                      : "Vehicle registered"}
                  </p>
                </div>
                <button
                  onClick={handleClose}
                  className="rounded-lg border border-glass-border bg-glass-100 p-1.5 text-white/40 hover:text-white transition-colors"
                >
                  <X size={14} />
                </button>
              </div>

              {/* ── Step progress ─────────────────────────────────────────── */}
              {step < 3 && (
                <div className="flex gap-1.5 mb-6">
                  {[1, 2].map((s) => (
                    <div
                      key={s}
                      className={`h-1 flex-1 rounded-full transition-all duration-500 ${
                        s < step ? "bg-cyan-neon" : s === step ? "bg-cyan-neon/60" : "bg-glass-300"
                      }`}
                    />
                  ))}
                </div>
              )}

              {/* ── Step 1: Vehicle Identity ──────────────────────────────── */}
              {step === 1 && (
                <form onSubmit={handleStep1} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <Truck size={14} className="text-cyan-neon" />
                    <span className="text-xs font-semibold text-cyan-neon uppercase tracking-wider">Vehicle Details</span>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Plate Number</label>
                      <input
                        className={inputCls}
                        value={s1.plate_number}
                        onChange={(e) => setS1((p) => ({ ...p, plate_number: e.target.value.toUpperCase() }))}
                        placeholder="ABC-1234"
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Vehicle Type</label>
                      <select
                        className={inputCls}
                        value={s1.vehicle_type}
                        onChange={(e) => handleTypeChange(e.target.value as VehicleType)}
                      >
                        {VEHICLE_TYPES.map((t) => (
                          <option key={t} value={t} className="bg-canvas-100">{t}</option>
                        ))}
                      </select>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Make</label>
                      <input
                        className={inputCls}
                        value={s1.make}
                        onChange={(e) => setS1((p) => ({ ...p, make: e.target.value }))}
                        placeholder="Toyota"
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Model</label>
                      <input
                        className={inputCls}
                        value={s1.model}
                        onChange={(e) => setS1((p) => ({ ...p, model: e.target.value }))}
                        placeholder="Hiace"
                        required
                      />
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Year</label>
                      <input
                        className={inputCls}
                        type="number"
                        min="1990"
                        max={THIS_YEAR + 1}
                        value={s1.year}
                        onChange={(e) => setS1((p) => ({ ...p, year: e.target.value }))}
                        placeholder={String(THIS_YEAR)}
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Color</label>
                      <input
                        className={inputCls}
                        value={s1.color}
                        onChange={(e) => setS1((p) => ({ ...p, color: e.target.value }))}
                        placeholder="White"
                        required
                      />
                    </div>
                  </div>

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400 font-mono">{error}</p>
                  )}

                  <button
                    type="submit"
                    disabled={loading}
                    className="w-full flex items-center justify-center gap-2 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 px-4 py-2.5 text-sm font-semibold text-cyan-neon hover:bg-cyan-neon/20 transition-all disabled:opacity-50"
                  >
                    {loading ? <Loader2 size={14} className="animate-spin" /> : <ChevronRight size={14} />}
                    {loading ? "Registering…" : "Next — Marketplace Listing"}
                  </button>
                </form>
              )}

              {/* ── Step 2: Marketplace Listing ───────────────────────────── */}
              {step === 2 && result && (
                <form onSubmit={handleStep2} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <Store size={14} className="text-purple-plasma" />
                    <span className="text-xs font-semibold text-purple-plasma uppercase tracking-wider">Marketplace Listing</span>
                  </div>

                  {/* Vehicle confirmation pill */}
                  <div className="rounded-lg border border-cyan-neon/20 bg-cyan-neon/5 px-3 py-2 flex items-center gap-2">
                    <Truck size={12} className="text-cyan-neon flex-shrink-0" />
                    <span className="text-xs font-mono text-white/60">
                      <span className="text-cyan-neon font-semibold">{result.plate}</span> registered in fleet
                    </span>
                  </div>

                  {/* Toggle */}
                  <label className="flex items-center gap-3 cursor-pointer group">
                    <button
                      type="button"
                      role="switch"
                      aria-checked={s2.list_on_marketplace}
                      onClick={() => setS2((p) => ({ ...p, list_on_marketplace: !p.list_on_marketplace }))}
                      className={`relative w-10 h-5 rounded-full flex-shrink-0 transition-colors ${
                        s2.list_on_marketplace ? "bg-purple-plasma" : "bg-glass-300"
                      }`}
                    >
                      <span
                        className={`absolute top-0.5 left-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                          s2.list_on_marketplace ? "translate-x-5" : "translate-x-0"
                        }`}
                      />
                    </button>
                    <span className="text-sm text-white/70 group-hover:text-white transition-colors">
                      List this vehicle on the Marketplace now
                    </span>
                  </label>

                  {/* Listing fields — shown only when toggle is on */}
                  {s2.list_on_marketplace && (
                    <div className="space-y-3 pt-1">
                      <div className="rounded-lg border border-purple-plasma/20 bg-purple-plasma/5 px-3 py-2 text-2xs font-mono text-white/40">
                        🏢 Listed under: LogisticOS Platform Fleet · auto-assigned carrier
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className={labelCls}>Size Class</label>
                          <select
                            className={inputCls}
                            value={s2.size_class}
                            onChange={(e) => setS2((p) => ({ ...p, size_class: e.target.value as SizeClass }))}
                          >
                            {SIZE_CLASSES.map((sc) => (
                              <option key={sc.value} value={sc.value} className="bg-canvas-100">
                                {sc.label}
                              </option>
                            ))}
                          </select>
                        </div>
                        <div>
                          <label className={labelCls}>Max Weight (kg)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="1"
                            value={s2.max_weight_kg}
                            onChange={(e) => setS2((p) => ({ ...p, max_weight_kg: e.target.value }))}
                            placeholder="800"
                          />
                        </div>
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className={labelCls}>Base Price (₱)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="1"
                            step="0.01"
                            value={s2.base_price_php}
                            onChange={(e) => setS2((p) => ({ ...p, base_price_php: e.target.value }))}
                            placeholder="900.00"
                          />
                        </div>
                        <div>
                          <label className={labelCls}>Per-km Rate (₱)</label>
                          <input
                            className={inputCls}
                            type="number"
                            min="0.01"
                            step="0.01"
                            value={s2.per_km_php}
                            onChange={(e) => setS2((p) => ({ ...p, per_km_php: e.target.value }))}
                            placeholder="18.00"
                          />
                        </div>
                      </div>

                      <div>
                        <label className={labelCls}>Service Area</label>
                        <input
                          className={inputCls}
                          value={s2.service_area_label}
                          onChange={(e) => setS2((p) => ({ ...p, service_area_label: e.target.value }))}
                          placeholder="Metro Manila"
                        />
                      </div>

                      <div className="grid grid-cols-2 gap-3">
                        <div>
                          <label className={labelCls}>Available From</label>
                          <input
                            className={inputCls}
                            type="datetime-local"
                            value={s2.idle_from}
                            onChange={(e) => setS2((p) => ({ ...p, idle_from: e.target.value }))}
                          />
                        </div>
                        <div>
                          <label className={labelCls}>Available Until</label>
                          <input
                            className={inputCls}
                            type="datetime-local"
                            value={s2.idle_until}
                            onChange={(e) => setS2((p) => ({ ...p, idle_until: e.target.value }))}
                          />
                        </div>
                      </div>
                    </div>
                  )}

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400 font-mono">{error}</p>
                  )}

                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => { setStep(1); setError(null); }}
                      className="flex-1 rounded-lg border border-glass-border bg-glass-100 px-4 py-2.5 text-sm text-white/60 hover:text-white transition-colors"
                    >
                      Back
                    </button>
                    <button
                      type="submit"
                      disabled={loading}
                      className="flex-[2] flex items-center justify-center gap-2 rounded-lg bg-purple-plasma/10 border border-purple-plasma/30 px-4 py-2.5 text-sm font-semibold text-purple-plasma hover:bg-purple-plasma/20 transition-all disabled:opacity-50"
                    >
                      {loading ? (
                        <Loader2 size={14} className="animate-spin" />
                      ) : (
                        <ChevronRight size={14} />
                      )}
                      {loading
                        ? (s2.list_on_marketplace ? "Publishing…" : "Saving…")
                        : (s2.list_on_marketplace ? "Register & List" : "Register Only")}
                    </button>
                  </div>
                </form>
              )}

              {/* ── Step 3: Success ───────────────────────────────────────── */}
              {step === 3 && result && (
                <div className="space-y-4">
                  <div className="flex flex-col items-center text-center gap-2 py-2">
                    <div
                      className={`h-12 w-12 rounded-full flex items-center justify-center border ${
                        result.listed
                          ? "bg-green-signal/10 border-green-signal/30"
                          : "bg-cyan-neon/10   border-cyan-neon/30"
                      }`}
                    >
                      <CheckCircle2
                        size={24}
                        className={result.listed ? "text-green-signal" : "text-cyan-neon"}
                      />
                    </div>
                    <p className="font-heading font-bold text-white">
                      {result.listed ? "Vehicle Registered & Listed!" : "Vehicle Registered"}
                    </p>
                    <p className="text-xs text-white/40">
                      {result.listed
                        ? `${result.plate} is now in your fleet and live on the marketplace.`
                        : `${result.plate} is in your fleet. You can list it on the Marketplace later.`}
                    </p>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100 p-3 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Plate</span>
                      <span className="text-xs text-cyan-neon font-mono font-bold">{result.plate}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Fleet Status</span>
                      <span className="text-xs text-green-signal font-mono font-semibold">Active</span>
                    </div>
                    {result.listed && (
                      <div className="flex justify-between">
                        <span className="text-xs text-white/40 font-mono">Marketplace</span>
                        <span className="text-xs text-purple-plasma font-mono font-semibold">Listed · Active</span>
                      </div>
                    )}
                  </div>

                  <div className="flex gap-2">
                    {result.listed && (
                      <a
                        href="/admin/marketplace"
                        className="flex-1 flex items-center justify-center rounded-lg border border-purple-plasma/30 bg-purple-plasma/10 px-4 py-2.5 text-xs font-semibold text-purple-plasma hover:bg-purple-plasma/20 transition-all"
                      >
                        View in Marketplace
                      </a>
                    )}
                    <button
                      onClick={handleClose}
                      className={`flex-1 rounded-lg border px-4 py-2.5 text-sm font-semibold transition-all ${
                        result.listed
                          ? "border-green-signal/30 bg-green-signal/10 text-green-signal hover:bg-green-signal/20"
                          : "border-cyan-neon/30   bg-cyan-neon/10   text-cyan-neon   hover:bg-cyan-neon/20"
                      }`}
                    >
                      Done
                    </button>
                  </div>
                </div>
              )}

            </GlassCard>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
