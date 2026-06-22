"use client";
/**
 * Admin Portal — Carrier Detail
 * Full profile, SLA metrics, rate cards, compliance status management,
 * API key generation, and suspend/activate controls for a single carrier.
 *
 * Route: /admin/carriers/:id
 * Data: GET /v1/carriers/:id  +  GET /v1/carriers/:id/sla-summary
 */
import { Suspense, useCallback, useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  ArrowLeft, RefreshCw, Power, PauseCircle, Key, Copy, Check,
  AlertTriangle, CheckCircle2, Clock, Package, Mail, Phone,
  Globe, Shield, Calendar, ChevronRight, X, UserPlus, Users,
} from "lucide-react";
import { createCarriersApi, type Carrier, type ZoneSlaRow, type ComplianceStatus } from "@/lib/api/carriers";
import { createIdentityApi, type InviteUserResult } from "@/lib/api/identity";
import { usePermissions } from "@/hooks/usePermissions";

const COMPLIANCE_OPTIONS: { value: ComplianceStatus; label: string; variant: "green" | "cyan" | "amber" | "red" | "muted" }[] = [
  { value: "pending_submission", label: "Pending Submission", variant: "amber"  },
  { value: "under_review",       label: "Under Review",       variant: "cyan"   },
  { value: "compliant",          label: "Compliant",          variant: "green"  },
  { value: "expiring_soon",      label: "Expiring Soon",      variant: "amber"  },
  { value: "expired",            label: "Expired",            variant: "red"    },
  { value: "rejected",           label: "Rejected",           variant: "red"    },
  { value: "suspended",          label: "Suspended",          variant: "red"    },
];

function complianceVariant(s: ComplianceStatus): "green" | "cyan" | "amber" | "red" | "muted" {
  return COMPLIANCE_OPTIONS.find((o) => o.value === s)?.variant ?? "muted";
}
function complianceLabel(s: ComplianceStatus): string {
  return COMPLIANCE_OPTIONS.find((o) => o.value === s)?.label ?? s;
}

function statusVariant(s: string): "green" | "amber" | "red" | "muted" {
  if (s === "active")               return "green";
  if (s === "pending_verification") return "amber";
  if (s === "suspended")            return "red";
  return "muted";
}

function gradeColor(g: string) {
  if (g === "excellent") return "text-green-signal";
  if (g === "good")      return "text-cyan-neon";
  if (g === "fair")      return "text-amber-signal";
  return "text-red-signal";
}

function daysAgoStr(n: number) {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return d.toISOString();
}

function CarrierDetailInner() {
  const params = useParams<{ id: string }>();
  const router = useRouter();
  const carrierId = params.id;
  const api = createCarriersApi();
  const { hasPermission } = usePermissions();
  const canManageCarriers  = hasPermission("carriers:manage");
  const canManageApiKeys   = hasPermission("api_keys:manage");
  const canManageCompliance = hasPermission("compliance:admin");

  const [carrier,      setCarrier]      = useState<Carrier | null>(null);
  const [zones,        setZones]        = useState<ZoneSlaRow[]>([]);
  const [loading,      setLoading]      = useState(true);
  const [error,        setError]        = useState<string | null>(null);

  // Suspend modal
  const [suspendOpen,    setSuspendOpen]    = useState(false);
  const [suspendReason,  setSuspendReason]  = useState("");
  const [suspending,     setSuspending]     = useState(false);

  // API key reveal
  const [generatingKey,  setGeneratingKey]  = useState(false);
  const [revealedKey,    setRevealedKey]    = useState<string | null>(null);
  const [copied,         setCopied]         = useState(false);

  // Compliance edit
  const [complianceEdit, setComplianceEdit] = useState(false);
  const [pendingCompliance, setPendingCompliance] = useState<ComplianceStatus | null>(null);
  const [savingCompliance,  setSavingCompliance]  = useState(false);

  const [actionMsg, setActionMsg] = useState<string | null>(null);

  // Partner portal invite
  const [inviteOpen,      setInviteOpen]      = useState(false);
  const [inviteEmail,     setInviteEmail]     = useState("");
  const [inviteFirstName, setInviteFirstName] = useState("");
  const [inviteLastName,  setInviteLastName]  = useState("");
  const [inviting,        setInviting]        = useState(false);
  const [inviteResult,    setInviteResult]    = useState<InviteUserResult | null>(null);
  const [inviteCopied,    setInviteCopied]    = useState(false);

  // Carrier approval
  const [approveOpen,  setApproveOpen]  = useState(false);
  const [approveNotes, setApproveNotes] = useState("");
  const [approving,    setApproving]    = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const c = await api.getCarrier(carrierId);
      setCarrier(c);
      // Zone summary — last 30 days
      try {
        const zoneData = await api.getSlaZoneSummary(
          carrierId,
          daysAgoStr(30),
          new Date().toISOString(),
        );
        setZones(zoneData.zones ?? []);
      } catch {
        setZones([]);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load carrier");
    } finally {
      setLoading(false);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [carrierId]);

  useEffect(() => { load(); }, [load]);

  const handleActivate = async () => {
    if (!carrier) return;
    try {
      await api.activateCarrier(carrierId);
      setActionMsg("Carrier activated successfully.");
      await load();
    } catch (e) {
      setActionMsg(`Failed: ${e instanceof Error ? e.message : "unknown error"}`);
    }
  };

  const handleSuspend = async () => {
    if (!suspendReason.trim()) return;
    setSuspending(true);
    try {
      await api.suspendCarrier(carrierId, suspendReason.trim());
      setSuspendOpen(false);
      setSuspendReason("");
      setActionMsg("Carrier suspended.");
      await load();
    } catch (e) {
      setActionMsg(`Failed: ${e instanceof Error ? e.message : "unknown error"}`);
    } finally {
      setSuspending(false);
    }
  };

  const handleGenerateKey = async () => {
    setGeneratingKey(true);
    try {
      const result = await api.generateApiKey(carrierId);
      setRevealedKey(result.api_key);
      await load();
    } catch (e) {
      setActionMsg(`Key generation failed: ${e instanceof Error ? e.message : "unknown error"}`);
    } finally {
      setGeneratingKey(false);
    }
  };

  const handleCopy = () => {
    if (!revealedKey) return;
    navigator.clipboard.writeText(revealedKey);
    setCopied(true);
    setTimeout(() => setCopied(false), 2500);
  };

  const handleSaveCompliance = async () => {
    if (!pendingCompliance) return;
    setSavingCompliance(true);
    try {
      await api.updateCarrier(carrierId, { compliance_status: pendingCompliance });
      setComplianceEdit(false);
      setPendingCompliance(null);
      setActionMsg("Compliance status updated.");
      await load();
    } catch (e) {
      setActionMsg(`Failed: ${e instanceof Error ? e.message : "unknown error"}`);
    } finally {
      setSavingCompliance(false);
    }
  };

  const identityApi = createIdentityApi();

  const handleInvite = async () => {
    if (!inviteEmail.trim() || !inviteFirstName.trim() || !inviteLastName.trim()) return;
    setInviting(true);
    try {
      const result = await identityApi.inviteUser({
        email:      inviteEmail.trim().toLowerCase(),
        first_name: inviteFirstName.trim(),
        last_name:  inviteLastName.trim(),
        roles:      ["partner"],
      });
      setInviteResult(result.data);
      setInviteOpen(false);
      setActionMsg(`Partner user ${result.data.email} created — share the temporary password.`);
    } catch (e) {
      setActionMsg(`Invite failed: ${e instanceof Error ? e.message : "unknown error"}`);
    } finally {
      setInviting(false);
    }
  };

  const handleCopyInvite = () => {
    if (!inviteResult) return;
    navigator.clipboard.writeText(inviteResult.temp_password);
    setInviteCopied(true);
    setTimeout(() => setInviteCopied(false), 2500);
  };

  const handleApprove = async () => {
    setApproving(true);
    try {
      await api.activateCarrier(carrierId);
      setApproveOpen(false);
      setApproveNotes("");
      setActionMsg("Carrier approved and activated — now eligible for dispatch allocation.");
      await load();
    } catch (e) {
      setActionMsg(`Approval failed: ${e instanceof Error ? e.message : "unknown error"}`);
    } finally {
      setApproving(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <p className="text-xs font-mono text-white/40 animate-pulse">Loading carrier…</p>
      </div>
    );
  }

  if (error || !carrier) {
    return (
      <div className="p-6">
        <GlassCard glow="red">
          <p className="text-xs font-mono text-red-signal">{error ?? "Carrier not found"}</p>
        </GlassCard>
      </div>
    );
  }

  const isActive    = carrier.status === "active";
  const isSuspended = carrier.status === "suspended";
  const isPending   = carrier.status === "pending_verification";
  const onTimeRate  = carrier.total_shipments > 0
    ? Math.round((carrier.on_time_count / carrier.total_shipments) * 1000) / 10
    : null;

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Back + header */}
      <motion.div variants={variants.fadeInUp} className="flex items-start justify-between gap-4">
        <div className="flex items-start gap-3">
          <button
            onClick={() => router.back()}
            className="mt-0.5 flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white transition-colors"
          >
            <ArrowLeft size={14} />
          </button>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="font-heading text-2xl font-bold text-white">{carrier.name}</h1>
              <span className="text-sm font-mono text-cyan-neon/60">{carrier.code}</span>
              <NeonBadge variant={statusVariant(carrier.status)} dot={isActive}>
                {carrier.status.replace(/_/g, " ")}
              </NeonBadge>
            </div>
            <p className="text-xs text-white/30 font-mono mt-0.5">
              ID: {carrier.id} · Onboarded {new Date(carrier.onboarded_at).toLocaleDateString("en-PH")}
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2 flex-shrink-0">
          <button
            onClick={load}
            className="flex items-center gap-1 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
            title="Refresh"
          >
            <RefreshCw size={12} />
          </button>
          {canManageCarriers && isSuspended && (
            <button
              onClick={handleActivate}
              className="flex items-center gap-1.5 rounded-lg border border-green-signal/40 bg-green-signal/10 px-3 py-2 text-xs font-semibold text-green-signal hover:bg-green-signal/20 transition-colors"
            >
              <Power size={12} /> Re-activate
            </button>
          )}
          {canManageCarriers && isPending && (
            <button
              onClick={() => { setApproveOpen(true); setApproveNotes(""); }}
              className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-amber-signal/80 to-green-signal/80 px-3 py-2 text-xs font-bold text-canvas hover:opacity-90 transition-opacity"
            >
              <CheckCircle2 size={12} /> Review &amp; Approve
            </button>
          )}
          {canManageCarriers && isActive && (
            <button
              onClick={() => { setSuspendOpen(true); setSuspendReason(""); }}
              className="flex items-center gap-1.5 rounded-lg border border-amber-signal/40 bg-amber-signal/10 px-3 py-2 text-xs font-semibold text-amber-signal hover:bg-amber-signal/20 transition-colors"
            >
              <PauseCircle size={12} /> Suspend
            </button>
          )}
        </div>
      </motion.div>

      {/* Action message toast */}
      <AnimatePresence>
        {actionMsg && (
          <motion.div
            initial={{ opacity: 0, y: -8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            className="rounded-lg border border-cyan-neon/20 bg-cyan-surface px-3 py-2 flex items-center gap-2"
          >
            <span className="text-xs font-mono text-cyan-neon">{actionMsg}</span>
            <button onClick={() => setActionMsg(null)} className="ml-auto text-white/30 hover:text-white">
              <X size={11} />
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Awaiting Approval banner — only for pending_verification carriers */}
      {isPending && (
        <motion.div variants={variants.fadeInUp}>
          <div className="flex items-center gap-4 rounded-xl border border-amber-signal/30 bg-amber-signal/5 px-5 py-4">
            <div className="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-xl bg-amber-signal/15 border border-amber-signal/25">
              <Clock size={16} className="text-amber-signal" />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-semibold text-amber-signal">Awaiting Ops Approval</p>
              <p className="text-xs text-white/40 mt-0.5">
                This carrier is registered but not yet live. Review the profile below, then approve to enable dispatch allocation.
              </p>
            </div>
            {canManageCarriers && (
              <button
                onClick={() => { setApproveOpen(true); setApproveNotes(""); }}
                className="flex-shrink-0 flex items-center gap-1.5 rounded-xl bg-gradient-to-r from-amber-signal to-green-signal px-5 py-2.5 text-xs font-bold text-canvas hover:opacity-90 transition-opacity"
              >
                <CheckCircle2 size={13} /> Review &amp; Approve
              </button>
            )}
          </div>
        </motion.div>
      )}

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-2 gap-3 lg:grid-cols-4">
        {[
          { label: "Total Shipments", value: carrier.total_shipments.toLocaleString(), color: "text-cyan-neon"    },
          { label: "On-Time",         value: carrier.on_time_count.toLocaleString(),   color: "text-green-signal" },
          { label: "Failed",          value: carrier.failed_count.toLocaleString(),    color: "text-red-signal"   },
          { label: "SLA Rate",        value: onTimeRate != null ? `${onTimeRate}%` : "—", color: onTimeRate != null && onTimeRate >= carrier.sla.on_time_target_pct ? "text-green-signal" : "text-red-signal" },
        ].map((m) => (
          <GlassCard key={m.label} size="sm">
            <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{m.label}</p>
            <p className={`text-xl font-bold font-mono mt-1 ${m.color}`}>{m.value}</p>
          </GlassCard>
        ))}
      </motion.div>

      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        {/* Profile */}
        <motion.div variants={variants.fadeInUp}>
          <GlassCard title="Carrier Profile">
            <div className="space-y-3">
              {[
                { icon: <Mail size={13} className="text-white/30" />,  label: "Contact Email",  value: carrier.contact_email },
                { icon: <Phone size={13} className="text-white/30" />, label: "Contact Phone",  value: carrier.contact_phone ?? "—" },
                { icon: <Globe size={13} className="text-white/30" />, label: "API Endpoint",   value: carrier.api_endpoint ?? "—" },
                { icon: <Shield size={13} className="text-white/30" />,label: "Performance",    value: carrier.performance_grade, valueClass: gradeColor(carrier.performance_grade) },
                { icon: <Calendar size={13} className="text-white/30" />, label: "Updated",     value: new Date(carrier.updated_at).toLocaleString("en-PH") },
              ].map((row) => (
                <div key={row.label} className="flex items-start gap-2.5 rounded-lg bg-glass-100/50 px-3 py-2.5">
                  {row.icon}
                  <div className="flex-1 min-w-0">
                    <p className="text-2xs font-mono text-white/30 uppercase tracking-wider">{row.label}</p>
                    <p className={`text-xs font-mono mt-0.5 truncate ${row.valueClass ?? "text-white/70"}`}>{row.value}</p>
                  </div>
                </div>
              ))}
            </div>
          </GlassCard>
        </motion.div>

        {/* SLA Commitment + Compliance */}
        <div className="flex flex-col gap-5">
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="SLA Commitment">
              <div className="space-y-2">
                {[
                  { label: "On-Time Target",       value: `${carrier.sla.on_time_target_pct}%` },
                  { label: "Max Delivery Days",    value: `${carrier.sla.max_delivery_days} days` },
                  { label: "Penalty per Breach",   value: carrier.sla.penalty_per_breach > 0 ? `₱${(carrier.sla.penalty_per_breach / 100).toFixed(2)}` : "None" },
                ].map((row) => (
                  <div key={row.label} className="flex items-center justify-between rounded-lg bg-glass-100/50 px-3 py-2.5">
                    <span className="text-xs font-mono text-white/40">{row.label}</span>
                    <span className="text-xs font-mono font-semibold text-white">{row.value}</span>
                  </div>
                ))}
              </div>
            </GlassCard>
          </motion.div>

          {/* Compliance */}
          <motion.div variants={variants.fadeInUp}>
            <GlassCard title="Compliance / KYB">
              <div className="space-y-3">
                <div className="flex items-center justify-between rounded-lg bg-glass-100/50 px-3 py-2.5">
                  <span className="text-xs font-mono text-white/40">Verification Status</span>
                  <NeonBadge variant={complianceVariant(carrier.compliance_status)}>
                    {complianceLabel(carrier.compliance_status)}
                  </NeonBadge>
                </div>

                {canManageCompliance && (!complianceEdit ? (
                  <button
                    onClick={() => { setComplianceEdit(true); setPendingCompliance(carrier.compliance_status); }}
                    className="w-full rounded-lg border border-glass-border py-2 text-xs text-white/50 hover:text-white hover:border-cyan-neon/30 transition-colors font-mono"
                  >
                    Change Status
                  </button>
                ) : (
                  <div className="space-y-2">
                    <div className="grid grid-cols-2 gap-1.5">
                      {COMPLIANCE_OPTIONS.map((opt) => (
                        <button
                          key={opt.value}
                          onClick={() => setPendingCompliance(opt.value)}
                          className={`rounded-lg border px-2 py-1.5 text-2xs font-mono transition-colors text-left ${
                            pendingCompliance === opt.value
                              ? "border-cyan-neon/50 bg-cyan-surface text-cyan-neon"
                              : "border-glass-border text-white/40 hover:text-white"
                          }`}
                        >
                          {opt.label}
                        </button>
                      ))}
                    </div>
                    <div className="flex gap-2">
                      <button
                        onClick={() => { setComplianceEdit(false); setPendingCompliance(null); }}
                        className="flex-1 rounded-lg border border-glass-border py-2 text-xs text-white/50 hover:text-white transition-colors"
                      >
                        Cancel
                      </button>
                      <button
                        onClick={handleSaveCompliance}
                        disabled={savingCompliance || pendingCompliance === carrier.compliance_status}
                        className="flex-1 rounded-lg border border-cyan-neon/40 bg-cyan-surface py-2 text-xs font-semibold text-cyan-neon hover:bg-cyan-neon/20 disabled:opacity-40 transition-colors"
                      >
                        {savingCompliance ? "Saving…" : "Save"}
                      </button>
                    </div>
                  </div>
                ))}
              </div>
            </GlassCard>
          </motion.div>
        </div>
      </div>

      {/* API Credentials */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard title="API Credentials">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-xs text-white/40">
                {carrier.has_api_key
                  ? "An integration API key is configured. You can rotate it below — the existing key is invalidated immediately."
                  : "No API key configured. Generate one to enable webhook / polling integration."}
              </p>
            </div>
            <div className="flex items-center gap-2">
              {carrier.has_api_key && (
                <NeonBadge variant="green" dot>Active</NeonBadge>
              )}
              {canManageApiKeys && (
                <button
                  onClick={handleGenerateKey}
                  disabled={generatingKey}
                  className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs font-semibold text-white/70 hover:text-white hover:border-cyan-neon/30 disabled:opacity-40 transition-colors"
                >
                  <Key size={11} />
                  {generatingKey ? "Generating…" : carrier.has_api_key ? "Rotate Key" : "Generate Key"}
                </button>
              )}
            </div>
          </div>

          {/* Reveal modal (inline) */}
          <AnimatePresence>
            {revealedKey && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="mt-4 rounded-lg border border-amber-signal/30 bg-amber-signal/5 p-4 space-y-3">
                  <div className="flex items-center gap-2">
                    <AlertTriangle size={13} className="text-amber-signal flex-shrink-0" />
                    <p className="text-2xs font-mono text-amber-signal">
                      This key will not be shown again — copy it now.
                    </p>
                  </div>
                  <div className="flex items-center gap-2 rounded-lg bg-black/40 border border-glass-border px-3 py-2.5">
                    <code className="flex-1 text-2xs font-mono text-cyan-neon break-all select-all">
                      {revealedKey}
                    </code>
                    <button onClick={handleCopy} className="flex-shrink-0 text-white/40 hover:text-white transition-colors">
                      {copied ? <Check size={14} className="text-green-signal" /> : <Copy size={14} />}
                    </button>
                  </div>
                  <button
                    onClick={() => setRevealedKey(null)}
                    className="w-full text-2xs font-mono text-white/30 hover:text-white/60 transition-colors"
                  >
                    I have copied the key — dismiss
                  </button>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </GlassCard>
      </motion.div>

      {/* Partner Portal Access */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          {/* Section header */}
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <div className="flex items-center gap-3">
              <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-purple-plasma/10 border border-purple-plasma/20">
                <Users size={15} className="text-purple-plasma" />
              </div>
              <div>
                <h2 className="font-heading text-sm font-semibold text-white">Partner Portal Access</h2>
                <p className="text-2xs font-mono text-white/30 mt-0.5">
                  SLA dashboard · Payout statements · Compliance docs
                </p>
              </div>
            </div>
            <div className="flex items-center gap-2">
              {inviteResult && <NeonBadge variant="green" dot>User Created</NeonBadge>}
              {!inviteOpen && !inviteResult && canManageCarriers && (
                <button
                  onClick={() => { setInviteEmail(carrier.contact_email); setInviteOpen(true); }}
                  className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-purple-plasma to-cyan-neon px-4 py-2 text-xs font-bold text-canvas hover:opacity-90 transition-opacity"
                >
                  <UserPlus size={12} />
                  Invite Portal User
                </button>
              )}
            </div>
          </div>

          {/* Idle state */}
          {!inviteOpen && !inviteResult && (
            <div className="px-5 py-4">
              <div className="flex items-start gap-3 rounded-xl border border-glass-border bg-glass-100/40 px-4 py-3.5">
                <Shield size={14} className="text-white/20 mt-0.5 flex-shrink-0" />
                <p className="text-xs text-white/40 leading-relaxed">
                  No Partner Portal users are set up for this carrier yet.
                  Inviting a user creates a{" "}
                  <span className="text-purple-plasma/80 font-medium">partner role</span>{" "}
                  account — they sign in to view their SLA reports, payout summaries, and compliance status.
                </p>
              </div>
            </div>
          )}

          {/* Invite form */}
          <AnimatePresence>
            {inviteOpen && !inviteResult && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="px-5 py-5 space-y-4">
                  {/* Info banner */}
                  <div className="flex items-start gap-3 rounded-xl border border-cyan-neon/15 bg-cyan-neon/5 px-4 py-3">
                    <Shield size={13} className="text-cyan-neon/60 flex-shrink-0 mt-0.5" />
                    <p className="text-2xs font-mono text-white/40 leading-relaxed">
                      A temporary password will be generated. Share it with the user — they must sign in via the{" "}
                      <span className="text-cyan-neon/70">Partner Portal</span> and update it on first login.
                      Access is scoped to this tenant only.
                    </p>
                  </div>

                  {/* Name row */}
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-1.5">First Name</p>
                      <input
                        type="text"
                        value={inviteFirstName}
                        onChange={(e) => setInviteFirstName(e.target.value)}
                        placeholder="Jane"
                        autoComplete="given-name"
                        className="w-full rounded-xl border border-glass-border bg-glass-200/50 px-3.5 py-2.5 text-sm text-white placeholder:text-white/20 outline-none focus:border-purple-plasma/50 focus:ring-1 focus:ring-purple-plasma/20 transition-all font-mono"
                      />
                    </div>
                    <div>
                      <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-1.5">Last Name</p>
                      <input
                        type="text"
                        value={inviteLastName}
                        onChange={(e) => setInviteLastName(e.target.value)}
                        placeholder="Reyes"
                        autoComplete="family-name"
                        className="w-full rounded-xl border border-glass-border bg-glass-200/50 px-3.5 py-2.5 text-sm text-white placeholder:text-white/20 outline-none focus:border-purple-plasma/50 focus:ring-1 focus:ring-purple-plasma/20 transition-all font-mono"
                      />
                    </div>
                  </div>

                  {/* Email */}
                  <div>
                    <p className="text-2xs font-mono text-white/40 uppercase tracking-widest mb-1.5">Work Email</p>
                    <div className="relative">
                      <Mail size={13} className="absolute left-3.5 top-1/2 -translate-y-1/2 text-white/25 pointer-events-none" />
                      <input
                        type="email"
                        value={inviteEmail}
                        onChange={(e) => setInviteEmail(e.target.value)}
                        placeholder="partner@carrier.com"
                        autoComplete="email"
                        className="w-full rounded-xl border border-glass-border bg-glass-200/50 pl-9 pr-4 py-2.5 text-sm text-white placeholder:text-white/20 outline-none focus:border-purple-plasma/50 focus:ring-1 focus:ring-purple-plasma/20 transition-all font-mono"
                      />
                    </div>
                  </div>

                  {/* Role pill */}
                  <div className="flex items-center gap-3 rounded-xl border border-glass-border bg-glass-100/40 px-4 py-3">
                    <span className="text-2xs font-mono text-white/30 uppercase tracking-widest">Role Assigned</span>
                    <NeonBadge variant="purple">partner</NeonBadge>
                    <span className="ml-auto text-2xs font-mono text-white/20">SLA · Payouts · Compliance</span>
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-3 pt-1">
                    <button
                      onClick={() => { setInviteOpen(false); setInviteFirstName(""); setInviteLastName(""); setInviteEmail(""); }}
                      className="rounded-xl border border-glass-border px-5 py-2.5 text-xs font-semibold text-white/50 hover:text-white hover:border-white/20 transition-colors"
                    >
                      Cancel
                    </button>
                    <button
                      onClick={handleInvite}
                      disabled={inviting || !inviteEmail.trim() || !inviteFirstName.trim() || !inviteLastName.trim()}
                      className="flex-1 flex items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-purple-plasma to-cyan-neon py-2.5 text-xs font-bold text-canvas hover:opacity-90 disabled:opacity-40 transition-opacity"
                    >
                      {inviting ? (
                        <><RefreshCw size={12} className="animate-spin" /> Creating account…</>
                      ) : (
                        <><UserPlus size={12} /> Send Invite &amp; Create Account</>
                      )}
                    </button>
                  </div>
                </div>
              </motion.div>
            )}
          </AnimatePresence>

          {/* Temp password reveal */}
          <AnimatePresence>
            {inviteResult && (
              <motion.div
                initial={{ opacity: 0, height: 0 }}
                animate={{ opacity: 1, height: "auto" }}
                exit={{ opacity: 0, height: 0 }}
                className="overflow-hidden"
              >
                <div className="px-5 py-4">
                  <div className="rounded-xl border border-amber-signal/30 bg-amber-signal/5 p-5 space-y-4">
                    <div className="flex items-start gap-3">
                      <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-amber-signal/10 flex-shrink-0">
                        <AlertTriangle size={14} className="text-amber-signal" />
                      </div>
                      <div>
                        <p className="text-xs font-semibold text-amber-signal">Copy this password now</p>
                        <p className="text-2xs font-mono text-white/40 mt-0.5">
                          Share with <span className="text-white/70">{inviteResult.email}</span> — it is not stored and cannot be recovered after you dismiss this.
                        </p>
                      </div>
                    </div>
                    <div className="flex items-center gap-3 rounded-xl bg-black/50 border border-white/10 px-4 py-3.5">
                      <code className="flex-1 text-base font-mono font-bold text-cyan-neon break-all select-all tracking-wider">
                        {inviteResult.temp_password}
                      </code>
                      <button
                        onClick={handleCopyInvite}
                        title="Copy password"
                        className="flex-shrink-0 flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-1.5 text-2xs font-mono text-white/40 hover:text-white hover:border-cyan-neon/30 transition-colors"
                      >
                        {inviteCopied
                          ? <><Check size={12} className="text-green-signal" /> Copied</>
                          : <><Copy size={12} /> Copy</>}
                      </button>
                    </div>
                    <button
                      onClick={() => setInviteResult(null)}
                      className="w-full rounded-xl border border-glass-border py-2.5 text-2xs font-mono text-white/30 hover:text-white/60 hover:border-white/20 transition-colors"
                    >
                      I have copied the password — dismiss
                    </button>
                  </div>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </GlassCard>
      </motion.div>

      {/* Rate Cards */}
      {carrier.rate_cards.length > 0 && (
        <motion.div variants={variants.fadeInUp}>
          <GlassCard padding="none">
            <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
              <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                <Package size={14} className="text-cyan-neon" />
                Rate Cards
              </h2>
              <span className="text-2xs font-mono text-white/30">{carrier.rate_cards.length} service type{carrier.rate_cards.length !== 1 ? "s" : ""}</span>
            </div>
            <div className="grid grid-cols-[1fr_90px_90px_80px_1fr] gap-3 px-5 py-2 border-b border-glass-border">
              {["Service", "Base Rate", "Per KG", "Max KG", "Zones"].map((h) => (
                <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
              ))}
            </div>
            {carrier.rate_cards.map((rc, i) => (
              <div key={i} className="grid grid-cols-[1fr_90px_90px_80px_1fr] gap-3 items-center px-5 py-3 border-b border-glass-border/40 hover:bg-glass-100 transition-colors">
                <NeonBadge variant={rc.service_type === "same_day" ? "cyan" : rc.service_type === "next_day" ? "purple" : "amber"}>
                  {rc.service_type.replace("_", " ")}
                </NeonBadge>
                <span className="text-xs font-mono text-white/70">₱{(rc.base_rate_cents / 100).toFixed(2)}</span>
                <span className="text-xs font-mono text-white/70">₱{(rc.per_kg_cents / 100).toFixed(2)}</span>
                <span className="text-xs font-mono text-white/70">{rc.max_weight_kg} kg</span>
                <div className="flex flex-wrap gap-0.5">
                  {rc.coverage_zones.slice(0, 3).map((z) => (
                    <span key={z} className="text-2xs font-mono text-white/40 bg-glass-200 rounded px-1">{z}</span>
                  ))}
                  {rc.coverage_zones.length > 3 && (
                    <span className="text-2xs font-mono text-white/30">+{rc.coverage_zones.length - 3}</span>
                  )}
                </div>
              </div>
            ))}
          </GlassCard>
        </motion.div>
      )}

      {/* Zone SLA Summary */}
      <motion.div variants={variants.fadeInUp}>
        <GlassCard padding="none">
          <div className="flex items-center justify-between px-5 py-4 border-b border-glass-border">
            <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
              <CheckCircle2 size={14} className="text-green-signal" />
              SLA by Zone — Last 30 Days
            </h2>
            {zones.length > 0 && (
              <a
                href={`/partner/sla?carrier=${encodeURIComponent(carrier.id)}`}
                className="flex items-center gap-1 text-2xs font-mono text-cyan-neon/60 hover:text-cyan-neon transition-colors"
              >
                Full SLA dashboard <ChevronRight size={10} />
              </a>
            )}
          </div>

          {zones.length === 0 ? (
            <p className="px-5 py-8 text-xs font-mono text-white/30 text-center">
              No completed SLA records in the last 30 days.
            </p>
          ) : (
            <>
              <div className="grid grid-cols-[1fr_72px_72px_72px_88px] gap-3 px-5 py-2 border-b border-glass-border">
                {["Zone", "Total", "On-Time", "Failed", "Rate"].map((h) => (
                  <span key={h} className="text-2xs font-mono text-white/30 uppercase tracking-wider">{h}</span>
                ))}
              </div>
              {zones.map((z) => (
                <div key={z.zone} className="grid grid-cols-[1fr_72px_72px_72px_88px] gap-3 items-center px-5 py-3 border-b border-glass-border/40 hover:bg-glass-100 transition-colors">
                  <span className="text-xs font-mono text-white/70">{z.zone}</span>
                  <span className="text-xs font-mono text-white/50">{z.total}</span>
                  <span className="text-xs font-mono text-green-signal">{z.on_time}</span>
                  <span className="text-xs font-mono text-red-signal">{z.failed}</span>
                  <span className={`text-xs font-mono font-semibold ${
                    z.on_time_rate >= carrier.sla.on_time_target_pct ? "text-green-signal" :
                    z.on_time_rate >= carrier.sla.on_time_target_pct - 3 ? "text-amber-signal" : "text-red-signal"
                  }`}>
                    {z.on_time_rate.toFixed(1)}%
                  </span>
                </div>
              ))}
            </>
          )}
        </GlassCard>
      </motion.div>

      {/* Approve modal */}
      <AnimatePresence>
        {canManageCarriers && approveOpen && isPending && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
            <motion.div
              initial={{ opacity: 0, scale: 0.95, y: 12 }}
              animate={{ opacity: 1, scale: 1, y: 0 }}
              exit={{ opacity: 0, scale: 0.95, y: 12 }}
              transition={{ type: "spring", duration: 0.35 }}
              className="w-full max-w-lg"
            >
              <GlassCard glow="cyan">
                <div className="flex items-start justify-between mb-5">
                  <div>
                    <h3 className="font-heading text-lg font-bold text-white">Review &amp; Approve Carrier</h3>
                    <p className="text-xs font-mono text-white/40 mt-0.5">{carrier.name} · {carrier.code}</p>
                  </div>
                  <button onClick={() => setApproveOpen(false)} className="text-white/40 hover:text-white transition-colors">
                    <X size={16} />
                  </button>
                </div>

                {/* Summary row */}
                <div className="rounded-xl border border-glass-border bg-glass-100/50 divide-y divide-glass-border mb-4">
                  {[
                    { label: "Contact Email",    value: carrier.contact_email },
                    { label: "On-Time Target",   value: `${carrier.sla.on_time_target_pct}%` },
                    { label: "Max Delivery Days",value: `${carrier.sla.max_delivery_days} days` },
                    { label: "Compliance",       value: carrier.compliance_status.replace(/_/g, " ") },
                  ].map((row) => (
                    <div key={row.label} className="flex items-center justify-between px-4 py-2.5">
                      <span className="text-xs font-mono text-white/40">{row.label}</span>
                      <span className="text-xs font-mono font-semibold text-white/80">{row.value}</span>
                    </div>
                  ))}
                </div>

                {/* Verification checklist (visual) */}
                <div className="mb-4 space-y-1.5">
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-widest mb-2">Pre-Approval Checklist</p>
                  {[
                    "Business registration / KYB documents received",
                    "Contact details confirmed with carrier ops team",
                    "SLA commitment reviewed and agreed",
                    "Partner portal access set up (optional)",
                  ].map((item) => (
                    <div key={item} className="flex items-center gap-2.5 rounded-lg border border-glass-border bg-glass-100/30 px-3 py-2">
                      <div className="h-4 w-4 flex-shrink-0 rounded border border-green-signal/40 bg-green-signal/10 flex items-center justify-center">
                        <Check size={10} className="text-green-signal" />
                      </div>
                      <span className="text-xs font-mono text-white/50">{item}</span>
                    </div>
                  ))}
                </div>

                {/* Notes */}
                <div className="mb-5">
                  <p className="text-2xs font-mono text-white/30 uppercase tracking-widest mb-1.5">Approval Notes <span className="normal-case text-white/20">(optional — logged in audit trail)</span></p>
                  <textarea
                    value={approveNotes}
                    onChange={(e) => setApproveNotes(e.target.value)}
                    rows={2}
                    placeholder="KYB verified via document upload dated 22 Jun 2026…"
                    className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-xs text-white placeholder:text-white/20 outline-none focus:border-cyan-neon/40 focus:ring-1 focus:ring-cyan-neon/15 resize-none font-mono transition-all"
                  />
                </div>

                <div className="flex items-center gap-3">
                  <button
                    onClick={() => setApproveOpen(false)}
                    className="rounded-xl border border-glass-border px-5 py-2.5 text-xs font-semibold text-white/50 hover:text-white transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleApprove}
                    disabled={approving}
                    className="flex-1 flex items-center justify-center gap-2 rounded-xl bg-gradient-to-r from-amber-signal to-green-signal py-2.5 text-xs font-bold text-canvas hover:opacity-90 disabled:opacity-40 transition-opacity"
                  >
                    {approving
                      ? <><RefreshCw size={12} className="animate-spin" /> Approving…</>
                      : <><CheckCircle2 size={12} /> Approve &amp; Activate Carrier</>}
                  </button>
                </div>
              </GlassCard>
            </motion.div>
          </div>
        )}
      </AnimatePresence>

      {/* Suspend modal */}
      <AnimatePresence>
        {canManageCarriers && suspendOpen && (
          <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
            <motion.div
              initial={{ opacity: 0, scale: 0.95 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.95 }}
              className="w-full max-w-md px-4"
            >
              <GlassCard glow="red">
                <div className="flex items-center justify-between mb-4">
                  <div>
                    <h3 className="font-heading text-base font-bold text-white">Suspend Carrier</h3>
                    <p className="text-xs font-mono text-white/40 mt-0.5">{carrier.name}</p>
                  </div>
                  <button onClick={() => setSuspendOpen(false)} className="text-white/40 hover:text-white">
                    <X size={16} />
                  </button>
                </div>
                <p className="text-xs text-white/50 mb-3">
                  Suspended carriers are removed from rate-shop and dispatch immediately.
                  This reason is logged in the audit trail.
                </p>
                <label className="block mb-1">
                  <span className="text-2xs font-mono text-white/40 uppercase tracking-wider">Reason</span>
                  <textarea
                    value={suspendReason}
                    onChange={(e) => setSuspendReason(e.target.value)}
                    rows={3}
                    placeholder="SLA breach / regulatory hold / contract dispute…"
                    className="mt-1 w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder:text-white/20 outline-none focus:border-red-signal/50 resize-none font-mono"
                  />
                </label>
                <div className="flex items-center gap-2 mt-4">
                  <button
                    onClick={() => setSuspendOpen(false)}
                    className="flex-1 rounded-lg border border-glass-border py-2 text-xs text-white/60 hover:text-white transition-colors"
                  >
                    Cancel
                  </button>
                  <button
                    onClick={handleSuspend}
                    disabled={suspending || !suspendReason.trim()}
                    className="flex-1 rounded-lg bg-red-signal/20 border border-red-signal/40 py-2 text-xs font-semibold text-red-signal hover:bg-red-signal/30 disabled:opacity-40 transition-colors"
                  >
                    {suspending ? "Suspending…" : "Confirm Suspend"}
                  </button>
                </div>
              </GlassCard>
            </motion.div>
          </div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}

export default function CarrierDetailPage() {
  return (
    <Suspense fallback={null}>
      <CarrierDetailInner />
    </Suspense>
  );
}
