"use client";
/**
 * Partner Portal — Settings
 * Carrier profile + SLA commitment editor backed by PUT /v1/carriers/:id.
 * Server clamps SLA target to [0, 100] and floors max_delivery_days at 1.
 *
 * Sections:
 *  • Carrier Profile  — name, email, phone, webhook URL (editable)
 *  • SLA Commitment   — on-time target, max days, penalty (editable)
 *  • Coverage Zones   — derived from rate cards (read-only)
 *  • API Credentials  — generate/rotate integration API key (B)
 *  • Compliance       — live compliance_status from carrier entity (admin-managed)
 */
import React, { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { ComplianceBadge } from "@/components/compliance/compliance-badge";
import {
  RefreshCw, Save, Key, Copy, Check, AlertTriangle, RotateCcw, X, Upload, FileText, Loader2,
} from "lucide-react";
import { variants } from "@/lib/design-system/tokens";
import { carriersApi, carrierIdOf, fmtPhp, type Carrier } from "@/lib/api/carriers";
import { authFetch } from "@/lib/auth/auth-fetch";

const CARRIER_SVC_URL = process.env.NEXT_PUBLIC_CARRIER_URL ?? "http://localhost:8010";

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

// ── API Key Reveal Modal ───────────────────────────────────────────────────────

function ApiKeyRevealModal({
  apiKey,
  onClose,
}: {
  apiKey: string;
  onClose: () => void;
}) {
  const [copied, setCopied] = useState(false);

  async function handleCopy() {
    await navigator.clipboard.writeText(apiKey);
    setCopied(true);
    setTimeout(() => setCopied(false), 2500);
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(5,8,16,0.90)", backdropFilter: "blur(8px)" }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      {/* Inner AnimatePresence so the card's exit spring animation fires.
          Without this, the card and backdrop unmount together and the card
          exit transition is skipped (matching the PickupModal pattern). */}
      <AnimatePresence>
      <motion.div
        initial={{ scale: 0.95, opacity: 0, y: 12 }}
        animate={{ scale: 1,    opacity: 1, y: 0 }}
        exit={{ scale: 0.95,    opacity: 0, y: 12 }}
        transition={{ type: "spring", duration: 0.35 }}
        className="w-full max-w-md"
      >
        <GlassCard glow="amber" className="relative space-y-4">
          <div className="flex items-start justify-between">
            <div className="flex items-center gap-2">
              <Key size={16} className="text-amber-signal" />
              <h3 className="font-heading font-bold text-white text-sm">API Key Generated</h3>
            </div>
            <button onClick={onClose} className="text-white/30 hover:text-white/70 transition-colors">
              <X size={14} />
            </button>
          </div>

          <div className="rounded-lg border border-amber-signal/30 bg-amber-signal/5 px-3 py-3 space-y-1">
            <div className="flex items-center gap-1.5 mb-1">
              <AlertTriangle size={11} className="text-amber-signal flex-shrink-0" />
              <span className="text-xs font-semibold text-amber-signal">This key will not be shown again</span>
            </div>
            <p className="text-2xs text-white/40 font-mono">
              Copy it now and store it securely. Generating a new key will immediately
              invalidate this one.
            </p>
          </div>

          <div className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2.5">
            <p className="text-xs text-white/40 font-mono mb-1.5 uppercase tracking-widest">API Key</p>
            <div className="flex items-center gap-2">
              <span className="flex-1 font-mono text-xs text-cyan-neon break-all">{apiKey}</span>
            </div>
          </div>

          <button
            onClick={handleCopy}
            className={`w-full flex items-center justify-center gap-2 rounded-lg border px-4 py-2.5 text-sm font-semibold transition-all ${
              copied
                ? "border-green-signal/40 bg-green-signal/10 text-green-signal"
                : "border-amber-signal/30 bg-amber-signal/10 text-amber-signal hover:bg-amber-signal/20"
            }`}
          >
            {copied ? <Check size={14} /> : <Copy size={14} />}
            {copied ? "Copied!" : "Copy to Clipboard"}
          </button>

          <button
            onClick={onClose}
            className="w-full rounded-lg border border-glass-border bg-glass-100 px-4 py-2 text-xs text-white/50 hover:text-white transition-colors"
          >
            I&apos;ve copied the key — Close
          </button>
        </GlassCard>
      </motion.div>
      </AnimatePresence>
    </motion.div>

  );
}

// ── Compliance upload card ────────────────────────────────────────────────────

const COMPLIANCE_DOC_TYPES = [
  { value: "business_registration", label: "Business Registration (SEC/DTI)" },
  { value: "insurance_certificate", label: "Insurance Certificate" },
  { value: "ltfrb_permit",          label: "LTFRB / Transport Permit" },
  { value: "bir_certificate",       label: "BIR Certificate of Registration" },
  { value: "other",                 label: "Other Supporting Document" },
] as const;

type ComplianceDocType = typeof COMPLIANCE_DOC_TYPES[number]["value"];

function ComplianceUploadCard({
  carrier,
  carrierId,
}: {
  carrier: Carrier | null;
  carrierId: string | null;
}) {
  const [docType,     setDocType]     = useState<ComplianceDocType>("business_registration");
  const [file,        setFile]        = useState<File | null>(null);
  const [uploading,   setUploading]   = useState(false);
  const [uploadError, setUploadError] = useState<string | null>(null);
  const [uploadDone,  setUploadDone]  = useState(false);
  const inputRef                      = React.useRef<HTMLInputElement>(null);

  function handleFileChange(e: React.ChangeEvent<HTMLInputElement>) {
    const f = e.target.files?.[0] ?? null;
    setFile(f);
    setUploadError(null);
    setUploadDone(false);
  }

  async function handleUpload() {
    if (!file || !carrierId) return;
    setUploading(true);
    setUploadError(null);
    try {
      const body = new FormData();
      body.append("document", file);
      body.append("doc_type", docType);
      const res = await authFetch(
        `${CARRIER_SVC_URL}/v1/carriers/${carrierId}/compliance-documents`,
        { method: "POST", body },
      );
      if (!res.ok) {
        const text = await res.text().catch(() => "");
        throw new Error(text || `Upload failed (${res.status})`);
      }
      setUploadDone(true);
      setFile(null);
      if (inputRef.current) inputRef.current.value = "";
    } catch (e) {
      setUploadError((e as Error).message ?? "Upload failed");
    } finally {
      setUploading(false);
    }
  }

  return (
    <GlassCard title="Compliance">
      <div className="space-y-3">
        <p className="text-xs text-white/40">
          Document verification is required to unlock all platform features.
          Submit your business registration, insurance, and permits to begin the
          review process.
        </p>

        <div className="flex items-center justify-between rounded-lg border border-glass-border bg-glass-100/50 px-3 py-2.5">
          <span className="text-xs font-mono text-white/50">Current Status</span>
          <ComplianceBadge status={carrier?.compliance_status ?? "pending_submission"} />
        </div>

        {/* Document type selector */}
        <div>
          <label className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">
            Document Type
          </label>
          <select
            value={docType}
            onChange={(e) => setDocType(e.target.value as ComplianceDocType)}
            className="w-full rounded-md border border-white/10 bg-white/[0.03] px-3 py-1.5 text-sm text-white focus:border-cyan-neon/50 focus:outline-none"
          >
            {COMPLIANCE_DOC_TYPES.map((t) => (
              <option key={t.value} value={t.value} className="bg-canvas-100">
                {t.label}
              </option>
            ))}
          </select>
        </div>

        {/* File picker */}
        <div>
          <label className="text-xs text-white/40 uppercase tracking-widest font-mono block mb-1">
            File (PDF, JPG, PNG — max 10 MB)
          </label>
          <div
            onClick={() => inputRef.current?.click()}
            className="flex cursor-pointer items-center gap-3 rounded-lg border border-dashed border-white/15 bg-white/[0.02] px-4 py-3 transition-colors hover:border-cyan-neon/30 hover:bg-cyan-neon/5"
          >
            <FileText size={16} className="flex-shrink-0 text-white/40" />
            <span className="flex-1 truncate text-sm text-white/50">
              {file ? file.name : "Click to select a file…"}
            </span>
            {file && (
              <span className="flex-shrink-0 font-mono text-2xs text-white/30">
                {(file.size / 1024).toFixed(0)} KB
              </span>
            )}
          </div>
          <input
            ref={inputRef}
            type="file"
            accept=".pdf,.jpg,.jpeg,.png"
            className="hidden"
            onChange={handleFileChange}
          />
        </div>

        {uploadError && (
          <p className="text-xs text-red-signal font-mono">{uploadError}</p>
        )}

        {uploadDone && (
          <div className="flex items-center gap-2 rounded-lg border border-green-signal/30 bg-green-signal/10 px-3 py-2">
            <Check size={12} className="text-green-signal" />
            <p className="text-xs text-green-signal font-mono">
              Document submitted — ops will review within 1–2 business days.
            </p>
          </div>
        )}

        <button
          onClick={handleUpload}
          disabled={!file || !carrierId || uploading}
          className="flex w-full items-center justify-center gap-2 rounded-lg border border-cyan-neon/30 bg-cyan-surface px-4 py-2.5 text-sm font-semibold text-cyan-neon transition-all hover:border-cyan-neon/60 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          {uploading ? (
            <Loader2 size={13} className="animate-spin" />
          ) : (
            <Upload size={13} />
          )}
          {uploading ? "Uploading…" : "Submit Document"}
        </button>

        <p className="text-2xs text-white/25 font-mono text-center">
          Files are encrypted at rest. Compliance status is updated by the ops team after review.
        </p>
      </div>
    </GlassCard>
  );
}

// ── Settings page ─────────────────────────────────────────────────────────────

export default function PartnerSettingsPage() {
  // Settings is the single source of truth for its own carrier data — it owns
  // its own carriersApi.me() fetch and local state. carrierId is derived from
  // the local carrier record, not from context, to avoid dual-state sync issues.

  const [carrier, setCarrier] = useState<Carrier | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError]     = useState<string | null>(null);
  const [saving, setSaving]   = useState(false);
  const [saved, setSaved]     = useState(false);

  // API key state
  const [generatingKey, setGeneratingKey] = useState(false);
  const [revealedKey,   setRevealedKey]   = useState<string | null>(null);
  const [keyError,      setKeyError]      = useState<string | null>(null);

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

  async function handleGenerateKey() {
    const id = carrier ? carrierIdOf(carrier) : null;
    if (!id) return;
    setGeneratingKey(true);
    setKeyError(null);
    try {
      const result = await carriersApi.generateApiKey(id);
      // Update local carrier has_api_key flag without a full reload.
      if (carrier) setCarrier({ ...carrier, has_api_key: true });
      setRevealedKey(result.api_key);
    } catch (e) {
      const err = e as { message?: string };
      setKeyError(err?.message ?? "Failed to generate API key");
    } finally {
      setGeneratingKey(false);
    }
  }

  // Flatten all coverage_zones across rate cards; dedupe.
  const coverageZones: string[] = carrier
    ? Array.from(new Set(carrier.rate_cards.flatMap((r) => r.coverage_zones)))
    : [];

  return (
    <>
      {/* API Key Reveal Modal (shown once after generation) */}
      <AnimatePresence>
        {revealedKey && (
          <ApiKeyRevealModal
            apiKey={revealedKey}
            onClose={() => setRevealedKey(null)}
          />
        )}
      </AnimatePresence>

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

          {/* API Credentials */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="API Credentials">
              <div className="space-y-4">
                <p className="text-xs text-white/40">
                  Use this key to authenticate inbound webhook callbacks from your
                  systems to the LogisticOS platform. Generate once, store securely —
                  the key is not recoverable after it is dismissed.
                </p>

                <div className="flex items-center justify-between rounded-lg border border-glass-border bg-glass-100/50 px-3 py-2.5">
                  <div className="flex items-center gap-2">
                    <Key size={13} className={carrier.has_api_key ? "text-green-signal" : "text-white/30"} />
                    <span className="text-xs font-mono text-white/60">
                      {carrier.has_api_key ? "API key configured" : "No API key generated"}
                    </span>
                  </div>
                  <NeonBadge variant={carrier.has_api_key ? "green" : "amber"}>
                    {carrier.has_api_key ? "Active" : "None"}
                  </NeonBadge>
                </div>

                {keyError && (
                  <p className="text-xs text-red-signal font-mono">{keyError}</p>
                )}

                <button
                  onClick={handleGenerateKey}
                  disabled={generatingKey}
                  className={`flex w-full items-center justify-center gap-2 rounded-lg border px-4 py-2.5 text-sm font-semibold transition-all disabled:opacity-50 disabled:cursor-not-allowed ${
                    carrier.has_api_key
                      ? "border-amber-signal/30 bg-amber-surface text-amber-signal hover:border-amber-signal/60"
                      : "border-cyan-neon/30 bg-cyan-surface text-cyan-neon hover:border-cyan-neon/60"
                  }`}
                >
                  {generatingKey ? (
                    <RotateCcw size={13} className="animate-spin" />
                  ) : carrier.has_api_key ? (
                    <RotateCcw size={13} />
                  ) : (
                    <Key size={13} />
                  )}
                  {generatingKey
                    ? "Generating…"
                    : carrier.has_api_key
                    ? "Rotate API Key"
                    : "Generate API Key"}
                </button>

                {carrier.has_api_key && (
                  <p className="text-2xs text-white/25 font-mono text-center">
                    Rotating generates a new key and immediately invalidates the previous one.
                  </p>
                )}
              </div>
            </GlassCard>
          </motion.div>

          {/* Compliance Status */}
          <motion.div variants={variants.fadeInUp}>
            <ComplianceUploadCard
              carrier={carrier}
              carrierId={carrier ? carrierIdOf(carrier) : null}
            />
          </motion.div>

        </div>
      ) : null}
    </motion.div>
    </>
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
