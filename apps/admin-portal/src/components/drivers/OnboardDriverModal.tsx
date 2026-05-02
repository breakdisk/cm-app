"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { X, UserPlus, Truck, CheckCircle2, ChevronRight, Loader2, Eye, EyeOff } from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { createIdentityApi } from "@/lib/api/identity";
import { createDriversApi } from "@/lib/api/drivers";

// ── Types ─────────────────────────────────────────────────────────────────────

interface Step1Fields {
  first_name: string;
  last_name: string;
  email: string;
  phone: string;
}

interface Step2Fields {
  driver_type: "full_time" | "part_time";
  vehicle_type: string;
  zone: string;
  per_delivery_rate: string;
  cod_commission_bps: string;
}

interface OnboardResult {
  user_id: string;
  driver_id: string;
  email: string;
  temp_password: string;
}

interface Props {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const VEHICLE_TYPES = ["Motorcycle", "Van", "Truck", "Bicycle", "Car"];
const ZONES = ["Makati", "BGC", "Quezon City", "Pasig", "Mandaluyong", "Caloocan", "Parañaque", "Las Piñas", "Valenzuela", "Other"];

const inputCls =
  "w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 focus:ring-cyan-neon/20 " +
  "transition-all font-mono";

const labelCls = "block text-xs text-white/50 mb-1.5 font-medium";

// ── Component ─────────────────────────────────────────────────────────────────

export function OnboardDriverModal({ open, onClose, onSuccess }: Props) {
  const [step, setStep] = useState<1 | 2 | 3>(1);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showPassword, setShowPassword] = useState(false);

  // Step 1 state — identity/invite
  const [s1, setS1] = useState<Step1Fields>({
    first_name: "", last_name: "", email: "", phone: "",
  });

  // Step 2 state — driver profile
  const [s2, setS2] = useState<Step2Fields>({
    driver_type: "full_time", vehicle_type: "Motorcycle",
    zone: "", per_delivery_rate: "0", cod_commission_bps: "0",
  });

  // Result after both steps complete
  const [result, setResult] = useState<OnboardResult | null>(null);

  function reset() {
    setStep(1);
    setLoading(false);
    setError(null);
    setResult(null);
    setShowPassword(false);
    setS1({ first_name: "", last_name: "", email: "", phone: "" });
    setS2({ driver_type: "full_time", vehicle_type: "Motorcycle", zone: "", per_delivery_rate: "0", cod_commission_bps: "0" });
  }

  function handleClose() {
    reset();
    onClose();
  }

  async function handleStep1Submit(e: React.FormEvent) {
    e.preventDefault();
    if (!s1.first_name || !s1.last_name || !s1.email || !s1.phone) {
      setError("All fields are required.");
      return;
    }
    setError(null);
    setLoading(true);
    try {
      const api = createIdentityApi();
      const res = await api.inviteUser({
        first_name:   s1.first_name,
        last_name:    s1.last_name,
        email:        s1.email,
        roles:        ["driver"],
        // Phone is required so OTP login finds this pre-registered record
        // instead of falling through to the synthetic-email create path,
        // which would assign a new UUID on every fresh install.
        phone_number: s1.phone,
      });
      // Store user_id + temp_password for the result screen
      setResult({
        user_id:       res.data.user_id,
        driver_id:     "",
        email:         res.data.email,
        temp_password: res.data.temp_password,
      });
      setStep(2);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to invite user. Please try again.");
    } finally {
      setLoading(false);
    }
  }

  async function handleStep2Submit(e: React.FormEvent) {
    e.preventDefault();
    if (!result?.user_id) return;
    setError(null);
    setLoading(true);
    try {
      const api = createDriversApi();
      const reg = await api.registerDriver({
        user_id:    result.user_id,
        first_name: s1.first_name,
        last_name:  s1.last_name,
        phone:      s1.phone,
      });
      const driverId = reg.data.driver_id;

      // Apply profile fields that aren't in RegisterDriverCommand via PATCH
      await api.updateDriver(driverId, {
        driver_type:             s2.driver_type,
        vehicle_type:            s2.vehicle_type || undefined,
        zone:                    s2.zone || undefined,
        per_delivery_rate_cents: Math.round(parseFloat(s2.per_delivery_rate || "0") * 100),
        cod_commission_rate_bps: parseInt(s2.cod_commission_bps || "0", 10),
      });

      setResult((prev) => prev ? { ...prev, driver_id: driverId } : null);
      setStep(3);
      onSuccess();
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to register driver profile. Please try again.");
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
            className="w-full max-w-md"
          >
            <GlassCard glow="cyan" className="relative">
              {/* Header */}
              <div className="flex items-center justify-between mb-6">
                <div>
                  <h2 className="font-heading text-lg font-bold text-white">Onboard Driver</h2>
                  <p className="text-xs text-white/40 mt-0.5">
                    {step === 1 ? "Step 1 of 2 — Account" : step === 2 ? "Step 2 of 2 — Profile" : "Driver onboarded"}
                  </p>
                </div>
                <button
                  onClick={handleClose}
                  className="rounded-lg border border-glass-border bg-glass-100 p-1.5 text-white/40 hover:text-white transition-colors"
                >
                  <X size={14} />
                </button>
              </div>

              {/* Step indicator */}
              {step < 3 && (
                <div className="flex items-center gap-2 mb-6">
                  {[1, 2].map((s) => (
                    <div key={s} className="flex items-center gap-2 flex-1">
                      <div
                        className={`h-1.5 flex-1 rounded-full transition-all duration-500 ${
                          s <= step ? "bg-cyan-neon" : "bg-glass-300"
                        }`}
                      />
                    </div>
                  ))}
                </div>
              )}

              {/* ── Step 1: Invite User ── */}
              {step === 1 && (
                <form onSubmit={handleStep1Submit} className="space-y-4">
                  <div className="flex items-center gap-2 mb-1">
                    <UserPlus size={14} className="text-cyan-neon" />
                    <span className="text-xs font-semibold text-cyan-neon uppercase tracking-wider">Account Details</span>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>First Name</label>
                      <input
                        className={inputCls}
                        value={s1.first_name}
                        onChange={(e) => setS1((p) => ({ ...p, first_name: e.target.value }))}
                        placeholder="Juan"
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Last Name</label>
                      <input
                        className={inputCls}
                        value={s1.last_name}
                        onChange={(e) => setS1((p) => ({ ...p, last_name: e.target.value }))}
                        placeholder="Dela Cruz"
                        required
                      />
                    </div>
                  </div>

                  <div>
                    <label className={labelCls}>Email Address</label>
                    <input
                      className={inputCls}
                      type="email"
                      value={s1.email}
                      onChange={(e) => setS1((p) => ({ ...p, email: e.target.value }))}
                      placeholder="juan@example.com"
                      required
                    />
                  </div>

                  <div>
                    <label className={labelCls}>Phone Number</label>
                    <input
                      className={inputCls}
                      type="tel"
                      value={s1.phone}
                      onChange={(e) => setS1((p) => ({ ...p, phone: e.target.value }))}
                      placeholder="+639171234567"
                      required
                    />
                    <p className="text-2xs text-white/30 mt-1 font-mono">Used for dispatch notifications and OTP verification</p>
                  </div>

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">{error}</p>
                  )}

                  <button
                    type="submit"
                    disabled={loading}
                    className="w-full flex items-center justify-center gap-2 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 px-4 py-2.5 text-sm font-semibold text-cyan-neon hover:bg-cyan-neon/20 transition-all disabled:opacity-50"
                  >
                    {loading ? <Loader2 size={14} className="animate-spin" /> : <ChevronRight size={14} />}
                    {loading ? "Creating account…" : "Next — Driver Profile"}
                  </button>
                </form>
              )}

              {/* ── Step 2: Driver Profile ── */}
              {step === 2 && (
                <form onSubmit={handleStep2Submit} className="space-y-4">
                  <div className="flex items-center gap-2 mb-1">
                    <Truck size={14} className="text-purple-plasma" />
                    <span className="text-xs font-semibold text-purple-plasma uppercase tracking-wider">Driver Profile</span>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Driver Type</label>
                      <select
                        className={inputCls}
                        value={s2.driver_type}
                        onChange={(e) => setS2((p) => ({ ...p, driver_type: e.target.value as "full_time" | "part_time" }))}
                      >
                        <option value="full_time">Full Time</option>
                        <option value="part_time">Part Time</option>
                      </select>
                    </div>
                    <div>
                      <label className={labelCls}>Vehicle Type</label>
                      <select
                        className={inputCls}
                        value={s2.vehicle_type}
                        onChange={(e) => setS2((p) => ({ ...p, vehicle_type: e.target.value }))}
                      >
                        {VEHICLE_TYPES.map((v) => <option key={v} value={v}>{v}</option>)}
                      </select>
                    </div>
                  </div>

                  <div>
                    <label className={labelCls}>Zone / Territory <span className="text-white/25">(optional)</span></label>
                    <select
                      className={inputCls}
                      value={s2.zone}
                      onChange={(e) => setS2((p) => ({ ...p, zone: e.target.value }))}
                    >
                      <option value="">— Unassigned —</option>
                      {ZONES.map((z) => <option key={z} value={z}>{z}</option>)}
                    </select>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Rate per Delivery (₱)</label>
                      <input
                        className={inputCls}
                        type="number"
                        min="0"
                        step="0.01"
                        value={s2.per_delivery_rate}
                        onChange={(e) => setS2((p) => ({ ...p, per_delivery_rate: e.target.value }))}
                        placeholder="0.00"
                      />
                    </div>
                    <div>
                      <label className={labelCls}>COD Commission (bps)</label>
                      <input
                        className={inputCls}
                        type="number"
                        min="0"
                        max="10000"
                        value={s2.cod_commission_bps}
                        onChange={(e) => setS2((p) => ({ ...p, cod_commission_bps: e.target.value }))}
                        placeholder="0"
                      />
                      <p className="text-2xs text-white/30 mt-1 font-mono">100 bps = 1%</p>
                    </div>
                  </div>

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">{error}</p>
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
                      {loading ? <Loader2 size={14} className="animate-spin" /> : <UserPlus size={14} />}
                      {loading ? "Registering…" : "Register Driver"}
                    </button>
                  </div>
                </form>
              )}

              {/* ── Step 3: Success ── */}
              {step === 3 && result && (
                <div className="space-y-4">
                  <div className="flex flex-col items-center text-center gap-2 py-2">
                    <div className="h-12 w-12 rounded-full bg-green-signal/10 border border-green-signal/30 flex items-center justify-center">
                      <CheckCircle2 size={24} className="text-green-signal" />
                    </div>
                    <p className="font-heading font-bold text-white">Driver Onboarded!</p>
                    <p className="text-xs text-white/40">
                      {s1.first_name} {s1.last_name} is registered and offline. They can log in with the credentials below.
                    </p>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100 p-3 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Email</span>
                      <span className="text-xs text-white font-mono">{result.email}</span>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-xs text-white/40 font-mono">Temp Password</span>
                      <div className="flex items-center gap-2">
                        <span className="text-xs text-amber-signal font-mono font-semibold">
                          {showPassword ? result.temp_password : "••••••••••••"}
                        </span>
                        <button onClick={() => setShowPassword((p) => !p)} className="text-white/30 hover:text-white/60 transition-colors">
                          {showPassword ? <EyeOff size={12} /> : <Eye size={12} />}
                        </button>
                      </div>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Driver ID</span>
                      <span className="text-xs text-white/60 font-mono">{result.driver_id.slice(0, 8)}…</span>
                    </div>
                  </div>

                  <p className="text-2xs text-white/30 text-center font-mono">
                    Share credentials securely. Driver must change password on first login.
                  </p>

                  <button
                    onClick={handleClose}
                    className="w-full rounded-lg bg-green-signal/10 border border-green-signal/30 px-4 py-2.5 text-sm font-semibold text-green-signal hover:bg-green-signal/20 transition-all"
                  >
                    Done
                  </button>
                </div>
              )}
            </GlassCard>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
