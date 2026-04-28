"use client";
/**
 * Partner Portal — Settings
 * Carrier profile + SLA commitment editor backed by PUT /v1/carriers/:id.
 * Server clamps SLA target to [0, 100] and floors max_delivery_days at 1.
 */
import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { RefreshCw, Save } from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { carriersApi, carrierIdOf, fmtPhp, type Carrier } from "@/lib/api/carriers";

function gradeBadge(grade: string): "green" | "cyan" | "amber" | "red" {
  switch (grade) {
    case "excellent": return "green";
    case "good":      return "cyan";
    case "fair":      return "amber";
    default:          return "red";
  }
}

function statusBadge(status: string): "green" | "amber" | "red" | "muted" {
  switch (status) {
    case "active":               return "green";
    case "pending_verification": return "amber";
    case "suspended":            return "red";
    default:                     return "muted";
  }
}

export default function PartnerSettingsPage() {
  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [saving, setSaving]   = useState(false);
  const [saved, setSaved]     = useState(false);

  // Editable form state — kept separate from `carrier` so a failed save
  // doesn't trash the user's in-progress input. Reset every time fresh
  // server data arrives via load().
  const [form, setForm] = useState<{
    name:               string;
    contact_email:      string;
    contact_phone:      string;
    api_endpoint:       string;
    on_time_target_pct: number;
    max_delivery_days:  number;
    penalty_per_breach: number;
  } | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const c = await carriersApi.me();
      setCarrier(c);
      setForm({
        name:               c.name,
        contact_email:      c.contact_email,
        contact_phone:      c.contact_phone ?? "",
        api_endpoint:       c.api_endpoint  ?? "",
        on_time_target_pct: c.sla.on_time_target_pct,
        max_delivery_days:  c.sla.max_delivery_days,
        penalty_per_breach: c.sla.penalty_per_breach,
      });
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load carrier profile");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function handleSave() {
    if (!carrier || !form) return;
    setSaving(true);
    setError(null);
    try {
      const updated = await carriersApi.update(carrierIdOf(carrier), {
        name:          form.name,
        contact_email: form.contact_email,
        contact_phone: form.contact_phone || undefined,
        api_endpoint:  form.api_endpoint  || undefined,
        sla: {
          on_time_target_pct: form.on_time_target_pct,
          max_delivery_days:  form.max_delivery_days,
          penalty_per_breach: form.penalty_per_breach,
        },
      });
      setCarrier(updated);
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Save failed");
    } finally {
      setSaving(false);
    }
  }

  // Flatten all coverage_zones across rate cards; dedupe.
  const coverageZones: string[] = carrier
    ? Array.from(new Set(carrier.rate_cards.flatMap((r) => r.coverage_zones)))
    : [];

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="p-6 space-y-6"
    >
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white font-space-grotesk">Settings</h1>
          <p className="text-white/40 text-sm mt-1">Carrier profile and SLA commitment</p>
        </div>
        <div className="flex items-center gap-2">
          {saved && (
            <span className="text-xs text-green-signal font-mono">✓ Saved</span>
          )}
          <button
            onClick={handleSave}
            disabled={saving || !form}
            className="flex items-center gap-1.5 rounded-lg border border-green-signal/30 bg-green-surface px-3 py-2 text-xs font-medium text-green-signal hover:border-green-signal/60 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            <Save size={12} />
            {saving ? "Saving…" : "Save Changes"}
          </button>
          <button
            onClick={load}
            className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
        </div>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-red-signal font-mono">{error}</p>
          </GlassCard>
        </motion.div>
      )}

      {loading && !carrier ? (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard>
            <p className="text-xs text-white/40 font-mono text-center py-6">loading carrier profile…</p>
          </GlassCard>
        </motion.div>
      ) : carrier && form ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Carrier Profile — editable */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="Carrier Profile">
              <div className="space-y-3">
                <Field label="Carrier Name">
                  <TextInput
                    value={form.name}
                    onChange={(v) => setForm({ ...form, name: v })}
                  />
                </Field>
                {/* Code is immutable — used as a stable carrier reference
                    across services + rate cards. Surface as a read-only row. */}
                <ReadOnlyRow label="Code" mono value={carrier.code} />
                <Field label="Email">
                  <TextInput
                    type="email"
                    value={form.contact_email}
                    onChange={(v) => setForm({ ...form, contact_email: v })}
                  />
                </Field>
                <Field label="Phone">
                  <TextInput
                    placeholder="+63 9XX XXX XXXX"
                    value={form.contact_phone}
                    onChange={(v) => setForm({ ...form, contact_phone: v })}
                  />
                </Field>
                <Field label="API Endpoint">
                  <TextInput
                    placeholder="https://carrier.example.com/webhook"
                    mono
                    value={form.api_endpoint}
                    onChange={(v) => setForm({ ...form, api_endpoint: v })}
                  />
                </Field>
                <ReadOnlyRow label="Grade" badge={gradeBadge(carrier.performance_grade)} value={carrier.performance_grade} />
                <ReadOnlyRow label="Status" badge={statusBadge(carrier.status)} value={carrier.status} />
              </div>
            </GlassCard>
          </motion.div>

          {/* SLA Commitment — editable */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="SLA Commitment">
              <div className="space-y-3">
                <Field label="On-Time Target (%)">
                  <NumberInput
                    min={0} max={100} step={0.1}
                    value={form.on_time_target_pct}
                    onChange={(v) => setForm({ ...form, on_time_target_pct: v })}
                  />
                </Field>
                <Field label="Max Delivery Days">
                  <NumberInput
                    min={1} max={30} step={1}
                    value={form.max_delivery_days}
                    onChange={(v) => setForm({ ...form, max_delivery_days: Math.round(v) })}
                  />
                </Field>
                <Field label="Breach Penalty (₱)">
                  <NumberInput
                    min={0} step={10}
                    value={form.penalty_per_breach / 100}
                    onChange={(v) => setForm({ ...form, penalty_per_breach: Math.round(v * 100) })}
                  />
                </Field>
                <ReadOnlyRow label="Total Shipments"   value={carrier.total_shipments.toLocaleString()} />
                <ReadOnlyRow label="On-Time Completed" value={carrier.on_time_count.toLocaleString()} />
                <ReadOnlyRow label="Failed"            value={carrier.failed_count.toLocaleString()} />
                <p className="text-2xs text-white/30 font-mono pt-2">
                  Onboarded {new Date(carrier.onboarded_at).toLocaleDateString()} · updated {new Date(carrier.updated_at).toLocaleDateString()}
                </p>
                <p className="text-2xs text-white/30 font-mono">
                  Current penalty equivalent: {carrier.sla.penalty_per_breach > 0 ? fmtPhp(carrier.sla.penalty_per_breach) : "None"}
                </p>
              </div>
            </GlassCard>
          </motion.div>

          {/* Coverage Zones */}
          <motion.div variants={variants.fadeInUp} className="lg:col-span-2">
            <GlassCard title="Coverage Zones">
              {coverageZones.length === 0 ? (
                <p className="text-xs text-white/40 font-mono py-2">
                  No zones configured across your rate cards. Contact ops to add coverage.
                </p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {coverageZones.map((z) => (
                    <NeonBadge key={z} variant="cyan">{z}</NeonBadge>
                  ))}
                </div>
              )}
            </GlassCard>
          </motion.div>
        </div>
      ) : null}
    </motion.div>
  );
}

// ── Local form primitives (kept inline; Settings is the only consumer) ──

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">{label}</span>
      {children}
    </label>
  );
}

function TextInput({
  value, onChange, type = "text", placeholder, mono = false,
}: {
  value: string;
  onChange: (v: string) => void;
  type?: "text" | "email" | "url";
  placeholder?: string;
  mono?: boolean;
}) {
  return (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange(e.target.value)}
      className={`w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white placeholder-white/20 focus:border-cyan-neon/50 focus:outline-none ${mono ? "font-mono text-xs" : ""}`}
    />
  );
}

function NumberInput({
  value, onChange, min, max, step,
}: {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
}) {
  return (
    <input
      type="number"
      value={value}
      min={min}
      max={max}
      step={step}
      onChange={(e) => {
        const n = parseFloat(e.target.value);
        if (!Number.isNaN(n)) onChange(n);
      }}
      className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white font-mono focus:border-cyan-neon/50 focus:outline-none"
    />
  );
}

function ReadOnlyRow({
  label, value, mono = false, badge,
}: {
  label: string;
  value: string;
  mono?: boolean;
  badge?: "green" | "amber" | "red" | "muted" | "cyan";
}) {
  return (
    <div className="flex justify-between items-center py-1.5 border-b border-white/[0.06]">
      <span className="text-xs text-white/40 uppercase tracking-widest font-mono">{label}</span>
      {badge ? (
        <NeonBadge variant={badge}>{value}</NeonBadge>
      ) : (
        <span className={`text-sm text-white ${mono ? "font-mono text-white/70" : ""}`}>{value}</span>
      )}
    </div>
  );
}
