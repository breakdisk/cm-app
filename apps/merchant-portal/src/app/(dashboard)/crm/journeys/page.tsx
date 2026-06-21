"use client";
/**
 * CRM → Journeys Page
 * Route: /crm/journeys
 *
 * Journey Builder — multi-step automated workflows:
 *  - List all journeys with status badges and step pills
 *  - Create / edit journeys with send_campaign / wait / condition steps
 *  - Activate / pause journeys
 *  - Enroll customers into active journeys
 *  - View enrollments per journey
 *
 * API: /v1/journeys (marketing service)
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  createJourneysApi,
  type Journey,
  type JourneyStep,
  type JourneyStatus,
  type JourneyEnrollment,
  type CreateJourneyStepPayload,
} from "@/lib/api/journeys";
import {
  GitBranch, Plus, Trash2, Play, Pause, X as XClose,
  ChevronRight, Clock, Send, CheckCircle2,
  Users, BarChart2, RefreshCw, ArrowRight, UserPlus,
} from "lucide-react";

// ── Step type metadata ─────────────────────────────────────────────────────────

const STEP_COLORS: Record<string, string> = {
  send_campaign: "#00FF88",
  wait:          "#FFAB00",
  condition:     "#00E5FF",
};

const STEP_ICONS: Record<string, React.ReactNode> = {
  send_campaign: <Send      size={12} />,
  wait:          <Clock     size={12} />,
  condition:     <GitBranch size={12} />,
};

const STEP_LABELS: Record<string, string> = {
  send_campaign: "Send Campaign",
  wait:          "Wait",
  condition:     "Condition",
};

const STATUS_BADGE: Record<JourneyStatus, { variant: "green" | "amber" | "purple" | "cyan"; label: string }> = {
  draft:    { variant: "purple", label: "Draft"    },
  active:   { variant: "green",  label: "Active"   },
  paused:   { variant: "amber",  label: "Paused"   },
  archived: { variant: "cyan",   label: "Archived" },
};

// ── Step Builder ───────────────────────────────────────────────────────────────

function StepRow({
  step,
  index,
  onRemove,
  onChange,
}: {
  step:     CreateJourneyStepPayload;
  index:    number;
  onRemove: () => void;
  onChange: (s: CreateJourneyStepPayload) => void;
}) {
  const color = STEP_COLORS[step.step_type] ?? "#A855F7";

  return (
    <div
      className="flex flex-col gap-2 rounded-xl border p-3"
      style={{ borderColor: `${color}30`, background: `${color}06` }}
    >
      <div className="flex items-center gap-2">
        <div className="flex h-5 w-5 items-center justify-center rounded text-xs" style={{ background: `${color}20`, color }}>
          {STEP_ICONS[step.step_type]}
        </div>
        <span className="text-xs font-semibold" style={{ color }}>
          Step {index + 1}
        </span>
        <select
          value={step.step_type}
          onChange={(e) => onChange({ ...step, step_type: e.target.value as never, campaign_id: null, wait_days: null, condition_type: null })}
          className="ml-1 flex-1 appearance-none rounded-lg border border-glass-border bg-glass-100 px-2 py-1 text-xs text-white outline-none"
          style={{ background: "#0d1422" }}
        >
          <option value="send_campaign">Send Campaign</option>
          <option value="wait">Wait</option>
          <option value="condition">Condition</option>
        </select>
        <button
          type="button"
          onClick={onRemove}
          className="flex h-5 w-5 items-center justify-center rounded text-white/30 hover:text-red-signal transition-colors"
        >
          <Trash2 size={11} />
        </button>
      </div>

      {step.step_type === "send_campaign" && (
        <input
          value={step.campaign_id ?? ""}
          onChange={(e) => onChange({ ...step, campaign_id: e.target.value || null })}
          placeholder="Campaign UUID"
          className="w-full rounded-lg border border-glass-border bg-glass-50/40 px-3 py-1.5 text-xs text-white placeholder-white/20 outline-none focus:border-green-signal/40 font-mono"
        />
      )}

      {step.step_type === "wait" && (
        <div className="flex items-center gap-2">
          <input
            type="number"
            min={1}
            value={step.wait_days ?? 1}
            onChange={(e) => onChange({ ...step, wait_days: Math.max(1, parseInt(e.target.value) || 1) })}
            className="w-20 rounded-lg border border-glass-border bg-glass-50/40 px-2 py-1.5 text-xs text-white outline-none focus:border-amber-signal/40 font-mono"
          />
          <span className="text-xs text-white/40">days before next step</span>
        </div>
      )}

      {step.step_type === "condition" && (
        <div className="flex flex-col gap-1.5">
          <select
            value={step.condition_type ?? "campaign_opened"}
            onChange={(e) => onChange({ ...step, condition_type: e.target.value })}
            className="w-full appearance-none rounded-lg border border-glass-border bg-glass-100 px-2 py-1.5 text-xs text-white outline-none"
            style={{ background: "#0d1422" }}
          >
            <option value="campaign_opened">Did customer open previous campaign?</option>
            <option value="campaign_clicked">Did customer click previous campaign?</option>
            <option value="clv_above_60">CLV score &gt; 60?</option>
          </select>
          <p className="text-2xs text-white/25 font-mono">
            Yes → step {step.yes_next_order ?? (step.step_order + 1)} · No → end (or configure below)
          </p>
        </div>
      )}
    </div>
  );
}

// ── New / Edit Journey Modal ───────────────────────────────────────────────────

function NewJourneyModal({
  existing,
  onClose,
  onSaved,
}: {
  existing?: Journey;
  onClose:   () => void;
  onSaved:   () => void;
}) {
  const [name,        setName]        = useState(existing?.name ?? "");
  const [description, setDescription] = useState(existing?.description ?? "");
  const [steps,       setSteps]       = useState<CreateJourneyStepPayload[]>(
    existing?.steps.map((s) => ({
      step_order:             s.step_order,
      step_type:              s.step_type,
      campaign_id:            s.campaign_id ?? null,
      wait_days:              s.wait_days ?? null,
      condition_type:         s.condition_type ?? null,
      condition_campaign_id:  s.condition_campaign_id ?? null,
      yes_next_order:         s.yes_next_order ?? null,
      no_next_order:          s.no_next_order ?? null,
    })) ?? []
  );
  const [saving, setSaving] = useState(false);
  const [error,  setError]  = useState<string | null>(null);

  const api = useMemo(() => createJourneysApi(), []);

  function addStep() {
    setSteps((prev) => [
      ...prev,
      {
        step_order: prev.length + 1,
        step_type:  "send_campaign",
        campaign_id: null, wait_days: null, condition_type: null,
        condition_campaign_id: null, yes_next_order: null, no_next_order: null,
      },
    ]);
  }

  function removeStep(i: number) {
    setSteps((prev) =>
      prev.filter((_, idx) => idx !== i).map((s, idx) => ({ ...s, step_order: idx + 1 }))
    );
  }

  function updateStep(i: number, s: CreateJourneyStepPayload) {
    setSteps((prev) => prev.map((old, idx) => idx === i ? s : old));
  }

  async function handleSave() {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const payload = {
        name: name.trim(),
        description: description.trim() || undefined,
        trigger: { type: "manual_enroll" },
        steps,
      };
      if (existing) {
        await api.update(existing.id, payload);
      } else {
        await api.create(payload);
      }
      onSaved();
      onClose();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to save journey");
    } finally {
      setSaving(false);
    }
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.78)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-lg rounded-2xl border border-glass-border shadow-glass flex flex-col"
        style={{ background: "rgba(8,12,28,0.98)", maxHeight: "90vh" }}
      >
        <div className="flex items-center justify-between px-6 pt-6 pb-4 shrink-0 border-b border-glass-border/50">
          <div>
            <h2 className="font-heading text-lg font-bold text-white">
              {existing ? "Edit Journey" : "New Journey"}
            </h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono">Journey Builder · Multi-step automation</p>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all"
          >
            <XClose size={15} />
          </button>
        </div>

        <div className="overflow-y-auto flex-1 px-6 py-4 flex flex-col gap-4">
          <div>
            <label className="mb-1.5 block text-xs font-medium text-white/50">Journey Name</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Post-Delivery Upsell Journey"
              className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/20 outline-none focus:border-cyan-neon/50 transition-colors"
            />
          </div>

          <div>
            <label className="mb-1.5 block text-xs font-medium text-white/50">Description <span className="text-white/25">(optional)</span></label>
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Describe when this journey runs"
              className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-sm text-white placeholder-white/20 outline-none focus:border-cyan-neon/50 transition-colors"
            />
          </div>

          <div>
            <div className="mb-2 flex items-center justify-between">
              <label className="text-xs font-medium text-white/50">Steps</label>
              <button
                type="button"
                onClick={addStep}
                className="flex items-center gap-1 rounded-lg border border-glass-border px-2.5 py-1 text-2xs text-cyan-neon/70 hover:text-cyan-neon transition-colors"
              >
                <Plus size={10} /> Add Step
              </button>
            </div>

            {steps.length === 0 ? (
              <div
                className="flex flex-col items-center gap-2 rounded-xl border border-dashed border-glass-border px-4 py-6 text-center cursor-pointer hover:border-cyan-neon/30 transition-colors"
                onClick={addStep}
              >
                <GitBranch size={20} className="text-white/20" />
                <p className="text-xs text-white/30">No steps yet — click to add a step</p>
              </div>
            ) : (
              <div className="flex flex-col gap-2">
                {steps.map((step, i) => (
                  <div key={i} className="flex items-start gap-2">
                    <div className="flex flex-col items-center gap-1 pt-1.5">
                      {i > 0 && <div className="w-px h-2 bg-white/10" />}
                      <ArrowRight size={11} className="text-white/20" />
                    </div>
                    <div className="flex-1">
                      <StepRow
                        step={step}
                        index={i}
                        onRemove={() => removeStep(i)}
                        onChange={(s) => updateStep(i, s)}
                      />
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {error && (
            <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs text-red-signal">
              {error}
            </p>
          )}
        </div>

        <div className="shrink-0 border-t border-glass-border/50 px-6 py-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={!name.trim() || saving}
            className="flex items-center gap-2 rounded-lg px-5 py-2 text-sm font-semibold text-white transition-all disabled:opacity-40"
            style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
          >
            {saving ? (
              <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" /> Saving…</>
            ) : (
              <><Plus size={14} /> {existing ? "Save Changes" : "Create Journey"}</>
            )}
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

// ── Enroll Customers Modal ─────────────────────────────────────────────────────

function EnrollModal({
  journey,
  onClose,
  onEnrolled,
}: {
  journey:    Journey;
  onClose:    () => void;
  onEnrolled: (count: number) => void;
}) {
  const [rawIds,    setRawIds]    = useState("");
  const [enrolling, setEnrolling] = useState(false);
  const [error,     setError]     = useState<string | null>(null);
  const [done,      setDone]      = useState(false);
  const [enrolled,  setEnrolled]  = useState(0);

  const api = useMemo(() => createJourneysApi(), []);

  async function handleEnroll() {
    const ids = rawIds.split(/[\n,]+/).map((s) => s.trim()).filter(Boolean);
    if (ids.length === 0) {
      setError("Enter at least one customer UUID");
      return;
    }
    setEnrolling(true);
    setError(null);
    try {
      const res = await api.enroll(journey.id, ids);
      setEnrolled(res.enrolled);
      setDone(true);
      onEnrolled(res.enrolled);
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Enrollment failed");
    } finally {
      setEnrolling(false);
    }
  }

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.78)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-md rounded-2xl border border-glass-border shadow-glass"
        style={{ background: "rgba(8,12,28,0.98)" }}
      >
        <div className="flex items-center justify-between px-6 pt-6 pb-4 border-b border-glass-border/50">
          <div>
            <h2 className="font-heading text-base font-bold text-white flex items-center gap-2">
              <UserPlus size={16} className="text-cyan-neon" />
              Enroll Customers
            </h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono truncate max-w-xs">{journey.name}</p>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all"
          >
            <XClose size={15} />
          </button>
        </div>

        <div className="px-6 py-4 flex flex-col gap-4">
          {done ? (
            <div className="flex flex-col items-center gap-3 py-4 text-center">
              <CheckCircle2 size={28} className="text-green-signal" />
              <p className="text-sm font-semibold text-white">{enrolled} customer{enrolled !== 1 ? "s" : ""} enrolled</p>
              <p className="text-xs text-white/40 font-mono">They will start receiving journey steps immediately.</p>
              <button
                onClick={onClose}
                className="mt-2 rounded-lg border border-glass-border px-5 py-2 text-sm text-white/60 hover:text-white transition-colors"
              >
                Close
              </button>
            </div>
          ) : (
            <>
              <div>
                <label className="mb-1.5 block text-xs font-medium text-white/50">
                  Customer UUIDs <span className="text-white/25">(one per line or comma-separated)</span>
                </label>
                <textarea
                  value={rawIds}
                  onChange={(e) => setRawIds(e.target.value)}
                  placeholder={"aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee\nffffffff-1111-2222-3333-444444444444"}
                  rows={5}
                  className="w-full rounded-xl border border-glass-border bg-glass-100 px-3.5 py-2.5 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/50 font-mono resize-none transition-colors"
                />
              </div>
              {error && (
                <p className="rounded-lg border border-red-signal/30 bg-red-signal/10 px-3 py-2 text-xs text-red-signal">
                  {error}
                </p>
              )}
              <div className="flex justify-end gap-2">
                <button
                  onClick={onClose}
                  className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors"
                >
                  Cancel
                </button>
                <button
                  onClick={handleEnroll}
                  disabled={enrolling || !rawIds.trim()}
                  className="flex items-center gap-2 rounded-lg px-5 py-2 text-sm font-semibold text-white transition-all disabled:opacity-40"
                  style={{ background: "linear-gradient(135deg, #00E5FF, #00FF88)" }}
                >
                  {enrolling
                    ? <><span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" /> Enrolling…</>
                    : <><UserPlus size={13} /> Enroll</>}
                </button>
              </div>
            </>
          )}
        </div>
      </motion.div>
    </motion.div>
  );
}

// ── Enrollments Panel ──────────────────────────────────────────────────────────

function EnrollmentsPanel({
  journeyId,
  journeyName,
  onClose,
}: {
  journeyId:   string;
  journeyName: string;
  onClose:     () => void;
}) {
  const [enrollments, setEnrollments] = useState<JourneyEnrollment[]>([]);
  const [loading,     setLoading]     = useState(true);
  const [error,       setError]       = useState<string | null>(null);

  const api = useMemo(() => createJourneysApi(), []);

  useEffect(() => {
    api.listEnrollments(journeyId)
      .then((r) => setEnrollments(r.enrollments ?? []))
      .catch((e) => setError((e as { message?: string })?.message ?? "Failed to load enrollments"))
      .finally(() => setLoading(false));
  }, [api, journeyId]);

  const STATUS_COLOR: Record<string, string> = {
    active:    "#00E5FF",
    completed: "#00FF88",
    exited:    "#FFAB00",
  };

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0,0,0,0.78)", backdropFilter: "blur(6px)" }}
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ opacity: 0, scale: 0.96, y: 16 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        exit={{ opacity: 0, scale: 0.96, y: 8 }}
        transition={{ ease: [0.16, 1, 0.3, 1], duration: 0.3 }}
        className="relative w-full max-w-xl rounded-2xl border border-glass-border shadow-glass flex flex-col"
        style={{ background: "rgba(8,12,28,0.98)", maxHeight: "80vh" }}
      >
        <div className="flex items-center justify-between px-6 pt-6 pb-4 shrink-0 border-b border-glass-border/50">
          <div>
            <h2 className="font-heading text-base font-bold text-white flex items-center gap-2">
              <Users size={16} className="text-cyan-neon" />
              Enrollments
            </h2>
            <p className="text-xs text-white/35 mt-0.5 font-mono truncate max-w-xs">{journeyName}</p>
          </div>
          <button
            onClick={onClose}
            className="flex h-8 w-8 items-center justify-center rounded-lg border border-glass-border text-white/40 hover:text-white/80 transition-all"
          >
            <XClose size={15} />
          </button>
        </div>

        <div className="overflow-y-auto flex-1 px-6 py-4">
          {loading ? (
            <div className="flex justify-center py-8">
              <span className="h-5 w-5 animate-spin rounded-full border-2 border-white/10 border-t-cyan-neon" />
            </div>
          ) : error ? (
            <p className="text-xs text-red-signal font-mono">{error}</p>
          ) : enrollments.length === 0 ? (
            <div className="flex flex-col items-center gap-2 py-10 text-center">
              <Users size={24} className="text-white/15" />
              <p className="text-sm text-white/40">No enrollments yet</p>
              <p className="text-xs text-white/25 max-w-xs">Use the Enroll button to add customers to this journey.</p>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              {enrollments.map((e) => {
                const color = STATUS_COLOR[e.status] ?? "#A855F7";
                return (
                  <div
                    key={e.id}
                    className="flex items-center gap-3 rounded-xl border border-glass-border px-4 py-3"
                    style={{ background: `${color}06` }}
                  >
                    <div
                      className="h-2 w-2 rounded-full shrink-0"
                      style={{ background: color, boxShadow: `0 0 6px ${color}80` }}
                    />
                    <div className="flex-1 min-w-0">
                      <p className="text-xs font-mono text-white/70 truncate">{e.customer_id}</p>
                      <p className="text-2xs font-mono text-white/30 mt-0.5">
                        Step {e.current_step_order ?? "—"} · {e.status}
                        {e.next_action_at && ` · next: ${new Date(e.next_action_at).toLocaleDateString()}`}
                      </p>
                    </div>
                    <span
                      className="rounded-full px-2 py-0.5 text-2xs font-mono shrink-0"
                      style={{ background: `${color}12`, color: `${color}CC`, border: `1px solid ${color}25` }}
                    >
                      {e.status}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        <div className="shrink-0 border-t border-glass-border/50 px-6 py-3 flex items-center justify-between">
          <p className="text-xs text-white/30 font-mono">{enrollments.length} total enrollment{enrollments.length !== 1 ? "s" : ""}</p>
          <button
            onClick={onClose}
            className="rounded-lg border border-glass-border px-4 py-2 text-sm text-white/50 hover:text-white transition-colors"
          >
            Close
          </button>
        </div>
      </motion.div>
    </motion.div>
  );
}

// ── Journey Card ───────────────────────────────────────────────────────────────

function JourneyCard({
  journey,
  onActivate,
  onPause,
  onEdit,
  onDelete,
  onEnroll,
  onViewEnrollments,
  mutating,
}: {
  journey:            Journey;
  onActivate:         (j: Journey) => void;
  onPause:            (j: Journey) => void;
  onEdit:             (j: Journey) => void;
  onDelete:           (id: string) => void;
  onEnroll:           (j: Journey) => void;
  onViewEnrollments:  (j: Journey) => void;
  mutating:           boolean;
}) {
  const badge = STATUS_BADGE[journey.status];

  return (
    <GlassCard>
      <div className="flex items-start gap-3">
        <div
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl"
          style={{ background: "rgba(0,229,255,0.08)", color: "#00E5FF" }}
        >
          <GitBranch size={16} />
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-sm font-semibold text-white truncate">{journey.name}</span>
            <NeonBadge variant={badge.variant}>{badge.label}</NeonBadge>
          </div>
          {journey.description && (
            <p className="mt-0.5 text-2xs text-white/35 truncate">{journey.description}</p>
          )}

          {/* Step pills */}
          {journey.steps.length > 0 ? (
            <div className="mt-2 flex items-center gap-1 flex-wrap">
              {journey.steps.map((step, i) => {
                const color = STEP_COLORS[step.step_type] ?? "#A855F7";
                return (
                  <span key={i} className="flex items-center gap-1">
                    {i > 0 && <ChevronRight size={9} className="text-white/20" />}
                    <span
                      className="flex items-center gap-1 rounded-full px-2 py-0.5 text-2xs font-mono"
                      style={{ background: `${color}12`, color: `${color}CC`, border: `1px solid ${color}25` }}
                    >
                      {STEP_ICONS[step.step_type]}
                      {step.step_type === "wait"
                        ? `${step.wait_days ?? 1}d`
                        : STEP_LABELS[step.step_type]}
                    </span>
                  </span>
                );
              })}
            </div>
          ) : (
            <p className="mt-1.5 text-2xs text-white/25 font-mono">No steps — edit to add steps</p>
          )}

          <div className="mt-2 flex items-center gap-2 text-2xs font-mono text-white/25">
            <span>{journey.steps.length} step{journey.steps.length !== 1 ? "s" : ""}</span>
            <span>·</span>
            <span>Updated {new Date(journey.updated_at).toLocaleDateString()}</span>
            {journey.status === "active" && (
              <>
                <span>·</span>
                <button
                  onClick={() => onViewEnrollments(journey)}
                  className="flex items-center gap-0.5 text-cyan-neon/50 hover:text-cyan-neon transition-colors"
                >
                  <Users size={10} /> Enrollments
                </button>
              </>
            )}
          </div>
        </div>

        <div className="flex items-center gap-1 shrink-0">
          {/* Activate: draft with steps */}
          {journey.status === "draft" && journey.steps.length > 0 && (
            <button
              onClick={() => onActivate(journey)}
              disabled={mutating}
              title="Activate"
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-green-signal/25 text-green-signal/60 hover:text-green-signal hover:bg-green-signal/10 transition-all disabled:opacity-40"
            >
              <Play size={12} />
            </button>
          )}

          {/* Pause: active */}
          {journey.status === "active" && (
            <button
              onClick={() => onPause(journey)}
              disabled={mutating}
              title="Pause"
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-amber-signal/25 text-amber-signal/60 hover:text-amber-signal hover:bg-amber-signal/10 transition-all disabled:opacity-40"
            >
              <Pause size={12} />
            </button>
          )}

          {/* Enroll: active */}
          {journey.status === "active" && (
            <button
              onClick={() => onEnroll(journey)}
              disabled={mutating}
              title="Enroll Customers"
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-cyan-neon/25 text-cyan-neon/60 hover:text-cyan-neon hover:bg-cyan-neon/10 transition-all disabled:opacity-40"
            >
              <UserPlus size={12} />
            </button>
          )}

          {/* Re-activate: paused */}
          {journey.status === "paused" && (
            <button
              onClick={() => onActivate(journey)}
              disabled={mutating}
              title="Resume"
              className="flex h-7 w-7 items-center justify-center rounded-lg border border-green-signal/25 text-green-signal/60 hover:text-green-signal hover:bg-green-signal/10 transition-all disabled:opacity-40"
            >
              <Play size={12} />
            </button>
          )}

          <button
            onClick={() => onEdit(journey)}
            title="Edit"
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-glass-border text-white/35 hover:text-white transition-all"
          >
            <BarChart2 size={12} />
          </button>
          <button
            onClick={() => onDelete(journey.id)}
            disabled={mutating}
            title="Delete"
            className="flex h-7 w-7 items-center justify-center rounded-lg border border-glass-border text-white/25 hover:text-red-signal hover:border-red-signal/30 transition-all disabled:opacity-40"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>
    </GlassCard>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────────

export default function JourneysPage() {
  const [journeys,    setJourneys]    = useState<Journey[]>([]);
  const [loading,     setLoading]     = useState(true);
  const [error,       setError]       = useState<string | null>(null);
  const [showNew,     setShowNew]     = useState(false);
  const [editing,     setEditing]     = useState<Journey | null>(null);
  const [enrolling,   setEnrolling]   = useState<Journey | null>(null);
  const [viewingEnrollments, setViewingEnrollments] = useState<Journey | null>(null);
  const [mutatingId,  setMutatingId]  = useState<string | null>(null);

  const api = useMemo(() => createJourneysApi(), []);

  const load = useCallback(async () => {
    setError(null);
    try {
      const res = await api.list();
      setJourneys(res.journeys ?? []);
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to load journeys");
    } finally {
      setLoading(false);
    }
  }, [api]);

  useEffect(() => { load(); }, [load]);

  async function handleActivate(journey: Journey) {
    setMutatingId(journey.id);
    try {
      await api.activate(journey.id);
      await load();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to activate journey");
    } finally {
      setMutatingId(null);
    }
  }

  async function handlePause(journey: Journey) {
    setMutatingId(journey.id);
    try {
      await api.pause(journey.id);
      await load();
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to pause journey");
    } finally {
      setMutatingId(null);
    }
  }

  async function handleDelete(id: string) {
    setMutatingId(id);
    try {
      await api.delete(id);
      setJourneys((prev) => prev.filter((j) => j.id !== id));
    } catch (e) {
      setError((e as { message?: string })?.message ?? "Failed to delete journey");
    } finally {
      setMutatingId(null);
    }
  }

  // Fetch full journey (with steps) before opening the edit modal — the list
  // response now includes steps, but fetch-on-edit ensures freshness.
  async function handleEdit(journey: Journey) {
    try {
      const full = await api.get(journey.id);
      setEditing(full);
    } catch {
      setEditing(journey);
    }
  }

  const active  = journeys.filter((j) => j.status === "active").length;
  const drafted = journeys.filter((j) => j.status === "draft").length;
  const paused  = journeys.filter((j) => j.status === "paused").length;

  return (
    <>
      <motion.div
        variants={variants.staggerContainer}
        initial="hidden"
        animate="visible"
        className="flex flex-col gap-5 p-6"
      >
        {/* Header */}
        <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
          <div>
            <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
              <GitBranch size={22} className="text-cyan-neon" />
              Journey Builder
            </h1>
            <p className="text-sm text-white/40 font-mono mt-0.5">
              Multi-step automation · {active} active{paused > 0 ? `, ${paused} paused` : ""}, {drafted} draft
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={load}
              className="flex items-center gap-1.5 rounded-lg border border-glass-border px-3 py-2 text-xs text-white/60 hover:text-white transition-colors"
              title="Refresh"
            >
              <RefreshCw size={13} />
            </button>
            <button
              onClick={() => setShowNew(true)}
              className="flex items-center gap-1.5 rounded-lg bg-gradient-to-r from-cyan-neon to-purple-plasma px-4 py-2 text-xs font-semibold text-white hover:opacity-90 transition-opacity"
            >
              <Plus size={13} /> New Journey
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

        {/* KPI row */}
        <motion.div variants={variants.fadeInUp} className="grid grid-cols-4 gap-3">
          {[
            { label: "Total",   value: journeys.length, color: "cyan"   as const },
            { label: "Active",  value: active,          color: "green"  as const },
            { label: "Paused",  value: paused,          color: "amber"  as const },
            { label: "Draft",   value: drafted,         color: "purple" as const },
          ].map((m) => (
            <GlassCard key={m.label} size="sm" glow={m.color} accent>
              <div className="flex flex-col gap-1">
                <p className="text-2xs font-mono text-white/35">{m.label}</p>
                <p className="font-heading text-xl font-bold text-white">{m.value}</p>
              </div>
            </GlassCard>
          ))}
        </motion.div>

        {/* Journey list */}
        <motion.div variants={variants.fadeInUp} className="flex flex-col gap-3">
          {loading ? (
            <GlassCard>
              <p className="text-center text-xs text-white/30 font-mono py-8">Loading journeys…</p>
            </GlassCard>
          ) : journeys.length === 0 ? (
            <GlassCard>
              <div className="flex flex-col items-center gap-3 py-12 text-center">
                <GitBranch size={32} className="text-white/15" />
                <p className="text-sm font-semibold text-white/50">No journeys yet</p>
                <p className="text-xs text-white/25 max-w-xs">
                  Build multi-step automation workflows. Send campaigns, add wait periods,
                  and branch on conditions to deliver the right message at the right time.
                </p>
                <button
                  onClick={() => setShowNew(true)}
                  className="mt-2 flex items-center gap-1.5 rounded-lg px-4 py-2 text-xs font-semibold text-white"
                  style={{ background: "linear-gradient(135deg, #00E5FF, #A855F7)" }}
                >
                  <Plus size={12} /> Create First Journey
                </button>
              </div>
            </GlassCard>
          ) : (
            journeys.map((journey) => (
              <motion.div key={journey.id} variants={variants.fadeInUp}>
                <JourneyCard
                  journey={journey}
                  onActivate={handleActivate}
                  onPause={handlePause}
                  onEdit={handleEdit}
                  onDelete={handleDelete}
                  onEnroll={(j) => setEnrolling(j)}
                  onViewEnrollments={(j) => setViewingEnrollments(j)}
                  mutating={mutatingId === journey.id}
                />
              </motion.div>
            ))
          )}
        </motion.div>
      </motion.div>

      <AnimatePresence>
        {(showNew || editing) && (
          <NewJourneyModal
            existing={editing ?? undefined}
            onClose={() => { setShowNew(false); setEditing(null); }}
            onSaved={load}
          />
        )}
        {enrolling && (
          <EnrollModal
            journey={enrolling}
            onClose={() => setEnrolling(null)}
            onEnrolled={() => { setEnrolling(null); }}
          />
        )}
        {viewingEnrollments && (
          <EnrollmentsPanel
            journeyId={viewingEnrollments.id}
            journeyName={viewingEnrollments.name}
            onClose={() => setViewingEnrollments(null)}
          />
        )}
      </AnimatePresence>
    </>
  );
}
