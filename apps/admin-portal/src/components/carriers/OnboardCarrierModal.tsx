"use client";

import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  X, GitBranch, UserPlus, Zap, CheckCircle2,
  ChevronRight, Loader2, Eye, EyeOff,
} from "lucide-react";
import { GlassCard } from "@/components/ui/glass-card";
import { createCarriersApi } from "@/lib/api/carriers";
import { createIdentityApi } from "@/lib/api/identity";

// ── Types ──────────────────────────────────────────────────────────────────────

interface Step1Fields {
  name: string;
  code: string;
  contact_email: string;
  sla_target: string;
  max_delivery_days: string;
}

interface Step2Fields {
  first_name: string;
  last_name: string;
  partner_email: string;
}

interface OnboardResult {
  carrier_id: string;
  carrier_name: string;
  carrier_code: string;
  user_id: string;
  partner_email: string;
  temp_password: string;
  activated: boolean;
}

interface Props {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const inputCls =
  "w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5 text-sm text-white " +
  "placeholder:text-white/25 outline-none focus:border-cyan-neon/50 focus:ring-1 focus:ring-cyan-neon/20 " +
  "transition-all font-mono";

const labelCls = "block text-xs text-white/50 mb-1.5 font-medium";

const STEP_META = [
  { icon: GitBranch, color: "text-cyan-neon",    label: "Carrier Entity" },
  { icon: UserPlus,  color: "text-purple-plasma", label: "Partner Account" },
  { icon: Zap,       color: "text-amber-signal",  label: "Activate" },
];

// ── Component ──────────────────────────────────────────────────────────────────

export function OnboardCarrierModal({ open, onClose, onSuccess }: Props) {
  const [step,    setStep]    = useState<1 | 2 | 3 | 4>(1);
  const [loading, setLoading] = useState(false);
  const [error,   setError]   = useState<string | null>(null);
  const [showPw,  setShowPw]  = useState(false);

  const [s1, setS1] = useState<Step1Fields>({
    name: "", code: "", contact_email: "", sla_target: "90", max_delivery_days: "3",
  });
  const [s2, setS2] = useState<Step2Fields>({
    first_name: "", last_name: "", partner_email: "",
  });
  const [result, setResult] = useState<OnboardResult | null>(null);

  function reset() {
    setStep(1); setLoading(false); setError(null); setResult(null); setShowPw(false);
    setS1({ name: "", code: "", contact_email: "", sla_target: "90", max_delivery_days: "3" });
    setS2({ first_name: "", last_name: "", partner_email: "" });
  }

  function handleClose() { reset(); onClose(); }

  // ── Step 1: Create carrier entity ──────────────────────────────────────────
  async function handleStep1(e: React.FormEvent) {
    e.preventDefault();
    if (!s1.name || !s1.code || !s1.contact_email) { setError("Name, code, and contact email are required."); return; }
    const slaTarget = parseFloat(s1.sla_target);
    const maxDays   = parseInt(s1.max_delivery_days, 10);
    if (isNaN(slaTarget) || slaTarget < 0 || slaTarget > 100) { setError("SLA target must be 0–100."); return; }
    if (isNaN(maxDays) || maxDays < 1) { setError("Max delivery days must be at least 1."); return; }

    setError(null); setLoading(true);
    try {
      const api = createCarriersApi();
      const res = await api.onboardCarrier({
        name:              s1.name,
        code:              s1.code.toUpperCase(),
        contact_email:     s1.contact_email,
        sla_target:        slaTarget,
        max_delivery_days: maxDays,
      });

      const carrierId = typeof res.data.id === "string"
        ? res.data.id
        : (res.data.id as unknown as { 0: string })[0] ?? "";

      setResult({
        carrier_id:    carrierId,
        carrier_name:  res.data.name,
        carrier_code:  res.data.code,
        user_id:       "",
        partner_email: "",
        temp_password: "",
        activated:     false,
      });
      // Pre-fill partner email from contact email
      setS2((p) => ({ ...p, partner_email: s1.contact_email }));
      setStep(2);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to create carrier. Check code uniqueness.");
    } finally {
      setLoading(false);
    }
  }

  // ── Step 2: Invite partner user account ───────────────────────────────────
  async function handleStep2(e: React.FormEvent) {
    e.preventDefault();
    if (!s2.first_name || !s2.last_name || !s2.partner_email) {
      setError("All account fields are required."); return;
    }
    setError(null); setLoading(true);
    try {
      const api = createIdentityApi();
      const res = await api.inviteUser({
        first_name: s2.first_name,
        last_name:  s2.last_name,
        email:      s2.partner_email,
        roles:      ["partner"],
      });
      setResult((prev) => prev ? {
        ...prev,
        user_id:       res.data.user_id,
        partner_email: res.data.email,
        temp_password: res.data.temp_password,
      } : null);
      setStep(3);
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to create partner account.");
    } finally {
      setLoading(false);
    }
  }

  // ── Step 3: Activate (optional) ────────────────────────────────────────────
  async function handleActivate() {
    if (!result?.carrier_id) return;
    setError(null); setLoading(true);
    try {
      const api = createCarriersApi();
      await api.activateCarrier(result.carrier_id);
      setResult((prev) => prev ? { ...prev, activated: true } : null);
      setStep(4);
      onSuccess();
    } catch (err: unknown) {
      const e = err as { message?: string };
      setError(e.message ?? "Failed to activate carrier.");
    } finally {
      setLoading(false);
    }
  }

  function handleSkipActivation() {
    setStep(4);
    onSuccess();
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
              {/* Header */}
              <div className="flex items-center justify-between mb-6">
                <div>
                  <h2 className="font-heading text-lg font-bold text-white">Onboard Carrier</h2>
                  <p className="text-xs text-white/40 mt-0.5">
                    {step < 4
                      ? `Step ${step} of 3 — ${STEP_META[step - 1]?.label ?? ""}`
                      : "Carrier onboarded"}
                  </p>
                </div>
                <button
                  onClick={handleClose}
                  className="rounded-lg border border-glass-border bg-glass-100 p-1.5 text-white/40 hover:text-white transition-colors"
                >
                  <X size={14} />
                </button>
              </div>

              {/* Step progress bar */}
              {step < 4 && (
                <div className="flex gap-1.5 mb-6">
                  {[1, 2, 3].map((s) => (
                    <div
                      key={s}
                      className={`h-1 flex-1 rounded-full transition-all duration-500 ${
                        s < step ? "bg-cyan-neon" : s === step ? "bg-cyan-neon/60" : "bg-glass-300"
                      }`}
                    />
                  ))}
                </div>
              )}

              {/* ── Step 1: Carrier entity ──────────────────────────────────── */}
              {step === 1 && (
                <form onSubmit={handleStep1} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <GitBranch size={14} className="text-cyan-neon" />
                    <span className="text-xs font-semibold text-cyan-neon uppercase tracking-wider">Carrier Details</span>
                  </div>

                  <div>
                    <label className={labelCls}>Carrier Name</label>
                    <input
                      className={inputCls}
                      value={s1.name}
                      onChange={(e) => setS1((p) => ({ ...p, name: e.target.value }))}
                      placeholder="FastLine Couriers"
                      required
                    />
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>Carrier Code <span className="text-white/25">(unique short ID)</span></label>
                      <input
                        className={inputCls}
                        value={s1.code}
                        onChange={(e) => setS1((p) => ({ ...p, code: e.target.value.toUpperCase() }))}
                        placeholder="FAST"
                        maxLength={10}
                        required
                      />
                      <p className="text-2xs text-white/25 mt-1 font-mono">3–10 uppercase chars, unique per tenant</p>
                    </div>
                    <div>
                      <label className={labelCls}>Contact Email</label>
                      <input
                        className={inputCls}
                        type="email"
                        value={s1.contact_email}
                        onChange={(e) => setS1((p) => ({ ...p, contact_email: e.target.value }))}
                        placeholder="ops@carrier.com"
                        required
                      />
                    </div>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100/50 p-4 space-y-3">
                    <p className="text-xs font-semibold text-white/60 uppercase tracking-wider">SLA Commitment</p>
                    <div className="grid grid-cols-2 gap-3">
                      <div>
                        <label className={labelCls}>On-Time Target (%)</label>
                        <input
                          className={inputCls}
                          type="number"
                          min="0"
                          max="100"
                          step="0.1"
                          value={s1.sla_target}
                          onChange={(e) => setS1((p) => ({ ...p, sla_target: e.target.value }))}
                          placeholder="90"
                        />
                      </div>
                      <div>
                        <label className={labelCls}>Max Delivery Days</label>
                        <input
                          className={inputCls}
                          type="number"
                          min="1"
                          max="30"
                          value={s1.max_delivery_days}
                          onChange={(e) => setS1((p) => ({ ...p, max_delivery_days: e.target.value }))}
                          placeholder="3"
                        />
                      </div>
                    </div>
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
                    {loading ? "Creating carrier…" : "Next — Partner Account"}
                  </button>
                </form>
              )}

              {/* ── Step 2: Partner user account ────────────────────────────── */}
              {step === 2 && (
                <form onSubmit={handleStep2} className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <UserPlus size={14} className="text-purple-plasma" />
                    <span className="text-xs font-semibold text-purple-plasma uppercase tracking-wider">Partner Account</span>
                  </div>

                  {result && (
                    <div className="rounded-lg border border-cyan-neon/20 bg-cyan-neon/5 px-3 py-2 flex items-center gap-2">
                      <GitBranch size={12} className="text-cyan-neon flex-shrink-0" />
                      <span className="text-xs font-mono text-white/60">
                        Carrier <span className="text-cyan-neon font-semibold">{result.carrier_code}</span> — {result.carrier_name} created (pending verification)
                      </span>
                    </div>
                  )}

                  <p className="text-xs text-white/40">
                    Create the partner portal login for the contact managing this carrier. They will use the Partner Portal to edit rates, view SLA, and manage drivers.
                  </p>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className={labelCls}>First Name</label>
                      <input
                        className={inputCls}
                        value={s2.first_name}
                        onChange={(e) => setS2((p) => ({ ...p, first_name: e.target.value }))}
                        placeholder="Maria"
                        required
                      />
                    </div>
                    <div>
                      <label className={labelCls}>Last Name</label>
                      <input
                        className={inputCls}
                        value={s2.last_name}
                        onChange={(e) => setS2((p) => ({ ...p, last_name: e.target.value }))}
                        placeholder="Santos"
                        required
                      />
                    </div>
                  </div>

                  <div>
                    <label className={labelCls}>Partner Login Email</label>
                    <input
                      className={inputCls}
                      type="email"
                      value={s2.partner_email}
                      onChange={(e) => setS2((p) => ({ ...p, partner_email: e.target.value }))}
                      placeholder="ops@carrier.com"
                      required
                    />
                    <p className="text-2xs text-white/25 mt-1 font-mono">
                      Partner portal matches this email to the carrier via /v1/carriers/me
                    </p>
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
                      {loading ? <Loader2 size={14} className="animate-spin" /> : <ChevronRight size={14} />}
                      {loading ? "Creating account…" : "Next — Activate"}
                    </button>
                  </div>
                </form>
              )}

              {/* ── Step 3: Activate ────────────────────────────────────────── */}
              {step === 3 && result && (
                <div className="space-y-4">
                  <div className="flex items-center gap-2 mb-2">
                    <Zap size={14} className="text-amber-signal" />
                    <span className="text-xs font-semibold text-amber-signal uppercase tracking-wider">Activate Carrier</span>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100/50 p-4 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Carrier</span>
                      <span className="text-xs text-white font-mono font-semibold">{result.carrier_name}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Code</span>
                      <span className="text-xs text-cyan-neon font-mono font-semibold">{result.carrier_code}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Partner Email</span>
                      <span className="text-xs text-white/70 font-mono">{result.partner_email}</span>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-xs text-white/40 font-mono">Temp Password</span>
                      <div className="flex items-center gap-2">
                        <span className="text-xs text-amber-signal font-mono font-semibold">
                          {showPw ? result.temp_password : "••••••••••••"}
                        </span>
                        <button onClick={() => setShowPw((p) => !p)} className="text-white/30 hover:text-white/60 transition-colors">
                          {showPw ? <EyeOff size={12} /> : <Eye size={12} />}
                        </button>
                      </div>
                    </div>
                  </div>

                  <div className="rounded-lg border border-amber-signal/20 bg-amber-signal/5 px-3 py-3 space-y-1">
                    <p className="text-xs font-semibold text-amber-signal">Activating makes this carrier live</p>
                    <p className="text-2xs text-white/40 font-mono">
                      The dispatch engine will include this carrier in auto-allocation rate shopping immediately. Leave pending if you need to verify credentials or configure rate cards first.
                    </p>
                  </div>

                  {error && (
                    <p className="rounded-lg bg-red-500/10 border border-red-500/20 px-3 py-2 text-xs text-red-400">{error}</p>
                  )}

                  <div className="flex gap-2">
                    <button
                      type="button"
                      onClick={handleSkipActivation}
                      disabled={loading}
                      className="flex-1 rounded-lg border border-glass-border bg-glass-100 px-4 py-2.5 text-sm text-white/60 hover:text-white transition-colors disabled:opacity-50"
                    >
                      Leave Pending
                    </button>
                    <button
                      type="button"
                      onClick={handleActivate}
                      disabled={loading}
                      className="flex-[2] flex items-center justify-center gap-2 rounded-lg bg-amber-signal/10 border border-amber-signal/30 px-4 py-2.5 text-sm font-semibold text-amber-signal hover:bg-amber-signal/20 transition-all disabled:opacity-50"
                    >
                      {loading ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
                      {loading ? "Activating…" : "Activate Now"}
                    </button>
                  </div>
                </div>
              )}

              {/* ── Step 4: Success ──────────────────────────────────────────── */}
              {step === 4 && result && (
                <div className="space-y-4">
                  <div className="flex flex-col items-center text-center gap-2 py-2">
                    <div className={`h-12 w-12 rounded-full flex items-center justify-center border ${
                      result.activated
                        ? "bg-green-signal/10 border-green-signal/30"
                        : "bg-amber-signal/10 border-amber-signal/30"
                    }`}>
                      <CheckCircle2 size={24} className={result.activated ? "text-green-signal" : "text-amber-signal"} />
                    </div>
                    <p className="font-heading font-bold text-white">
                      {result.activated ? "Carrier Activated!" : "Carrier Onboarded — Pending Verification"}
                    </p>
                    <p className="text-xs text-white/40">
                      {result.activated
                        ? `${result.carrier_name} is live and eligible for dispatch allocation.`
                        : `${result.carrier_name} is registered. Activate when ready to route shipments.`}
                    </p>
                  </div>

                  <div className="rounded-lg border border-glass-border bg-glass-100 p-3 space-y-2">
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Carrier Code</span>
                      <span className="text-xs text-cyan-neon font-mono font-bold">{result.carrier_code}</span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Status</span>
                      <span className={`text-xs font-mono font-semibold ${result.activated ? "text-green-signal" : "text-amber-signal"}`}>
                        {result.activated ? "Active" : "Pending Verification"}
                      </span>
                    </div>
                    <div className="flex justify-between">
                      <span className="text-xs text-white/40 font-mono">Partner Login</span>
                      <span className="text-xs text-white/70 font-mono">{result.partner_email}</span>
                    </div>
                    <div className="flex justify-between items-center">
                      <span className="text-xs text-white/40 font-mono">Temp Password</span>
                      <div className="flex items-center gap-2">
                        <span className="text-xs text-amber-signal font-mono font-semibold">
                          {showPw ? result.temp_password : "••••••••••••"}
                        </span>
                        <button onClick={() => setShowPw((p) => !p)} className="text-white/30 hover:text-white/60 transition-colors">
                          {showPw ? <EyeOff size={12} /> : <Eye size={12} />}
                        </button>
                      </div>
                    </div>
                  </div>

                  <p className="text-2xs text-white/30 text-center font-mono">
                    Share credentials securely. Partner must change password on first login.
                  </p>

                  <div className="flex gap-2">
                    <a
                      href={`/partner/settings`}
                      className="flex-1 flex items-center justify-center rounded-lg border border-purple-plasma/30 bg-purple-plasma/10 px-4 py-2.5 text-xs font-semibold text-purple-plasma hover:bg-purple-plasma/20 transition-all"
                    >
                      Open Partner Portal
                    </a>
                    <button
                      onClick={handleClose}
                      className={`flex-1 rounded-lg border px-4 py-2.5 text-sm font-semibold transition-all ${
                        result.activated
                          ? "border-green-signal/30 bg-green-signal/10 text-green-signal hover:bg-green-signal/20"
                          : "border-glass-border bg-glass-100 text-white/60 hover:text-white"
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
