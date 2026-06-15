"use client";
/**
 * CRM → Automations Page
 * Route: /crm/automations
 *
 * Displays the business-logic rules engine configuration:
 *  - List all automation rules with enable/disable toggle
 *  - Create / edit / delete rules via modal builder
 *  - View recent execution history per rule
 *
 * API: GET/POST/PUT/DELETE/PATCH /v1/rules  (business-logic service)
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  getAutomationsApi,
  type AutomationRule,
  type RuleCondition,
  type CreateRulePayload,
  type RuleExecution,
  triggerLabel,
  actionLabel,
  conditionLabel,
} from "@/lib/api/automations";
import { createSegmentsApi, type Segment } from "@/lib/api/segments";
import {
  Zap, Plus, Trash2, Edit2, X, ChevronRight,
  CheckCircle2, AlertCircle, Clock, ToggleLeft, ToggleRight,
  Activity, Code2, ChevronDown, Filter,
} from "lucide-react";

// ── constants ──────────────────────────────────────────────────────────────────

const TRIGGER_OPTIONS = [
  { value: "DeliveryFailed",    label: "Delivery Failed" },
  { value: "DeliveryCompleted", label: "Delivery Completed" },
  { value: "ShipmentCreated",   label: "Shipment Created" },
  { value: JSON.stringify({ DeliveryAttempted: { attempts: 3 } }), label: "Delivery Attempted (3×)" },
  { value: JSON.stringify({ CustomerInactive: { days: 30 } }),     label: "Customer Inactive 30d" },
  { value: JSON.stringify({ PaymentOverdue: { days: 7 } }),        label: "Payment Overdue 7d" },
];

const ACTION_TEMPLATES = [
  { label: "Notify Customer (WhatsApp)",  value: JSON.stringify({ NotifyCustomer: { channel: "whatsapp", template_id: "delivery_failed_reschedule" } }) },
  { label: "Notify Customer (Email)",     value: JSON.stringify({ NotifyCustomer: { channel: "email",    template_id: "delivery_confirmed" } }) },
  { label: "Notify Customer (SMS)",       value: JSON.stringify({ NotifyCustomer: { channel: "sms",      template_id: "delivery_failed_reschedule" } }) },
  { label: "Reschedule Delivery (+24h)",  value: JSON.stringify({ RescheduleDelivery: { delay_hours: 24 } }) },
  { label: "Trigger Campaign",            value: "__trigger_campaign__" },
  { label: "Escalate to Support (Tier 1)",value: JSON.stringify({ EscalateToSupport: { tier: 1 } }) },
  { label: "Alert Dispatcher",            value: JSON.stringify({ AlertDispatcher: { message: "Rule triggered", priority: "normal" } }) },
  { label: "Run AI Dispatch",             value: JSON.stringify("RunAiDispatch") },
  { label: "Log Audit Event",             value: JSON.stringify({ LogAuditEvent: { event_type: "rule.fired" } }) },
];

// Sentinel value used while the user is filling in the campaign_id.
const TRIGGER_CAMPAIGN_SENTINEL = "__trigger_campaign__";

// ── helpers ────────────────────────────────────────────────────────────────────

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function parseTrigger(raw: string) {
  try {
    return JSON.parse(raw);
  } catch {
    return raw;
  }
}

function parseActions(raws: string[]) {
  return raws.map((r) => {
    try {
      return JSON.parse(r);
    } catch {
      return r;
    }
  });
}

// ── components ─────────────────────────────────────────────────────────────────

function RuleRow({
  rule,
  onToggle,
  onEdit,
  onDelete,
  onViewExecutions,
}: {
  rule: AutomationRule;
  onToggle: (id: string, active: boolean) => void;
  onEdit: (rule: AutomationRule) => void;
  onDelete: (id: string) => void;
  onViewExecutions: (rule: AutomationRule) => void;
}) {
  const [toggling, setToggling] = useState(false);

  async function handleToggle() {
    setToggling(true);
    try {
      await onToggle(rule.id, !rule.is_active);
    } finally {
      setToggling(false);
    }
  }

  return (
    <motion.div
      layout
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -8 }}
      className={`group flex items-start gap-4 rounded-xl border p-4 transition-all ${
        rule.is_active
          ? "border-cyan-neon/20 bg-cyan-neon/5"
          : "border-glass-border bg-glass-100/30"
      }`}
    >
      {/* Active indicator */}
      <div className={`mt-0.5 h-2 w-2 flex-shrink-0 rounded-full ${rule.is_active ? "bg-green-signal shadow-[0_0_6px_#00FF8880]" : "bg-white/10"}`} />

      {/* Content */}
      <div className="min-w-0 flex-1">
        <div className="flex items-start justify-between gap-2">
          <div>
            <p className="text-sm font-semibold text-white">{rule.name}</p>
            {rule.description && (
              <p className="mt-0.5 text-2xs font-mono text-white/40 line-clamp-1">{rule.description}</p>
            )}
          </div>
          <NeonBadge variant={rule.priority <= 20 ? "amber" : "muted"} className="flex-shrink-0 text-[9px]">
            P{rule.priority}
          </NeonBadge>
        </div>

        {/* Trigger + Actions */}
        <div className="mt-2 flex flex-wrap items-center gap-1.5 text-2xs">
          <span className="font-mono text-white/30">WHEN</span>
          <NeonBadge variant="cyan">{triggerLabel(rule.trigger)}</NeonBadge>
          {rule.conditions.length > 0 && (
            <>
              <span className="font-mono text-white/30">IF</span>
              {rule.conditions.map((c, i) => (
                <NeonBadge key={i} variant="purple">{conditionLabel(c)}</NeonBadge>
              ))}
            </>
          )}
          <span className="font-mono text-white/30">THEN</span>
          {rule.actions.map((a, i) => (
            <NeonBadge key={i} variant="green">{actionLabel(a)}</NeonBadge>
          ))}
        </div>

        {/* Meta */}
        <p className="mt-2 text-2xs font-mono text-white/20">
          Created {relativeTime(rule.created_at)}
        </p>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={() => onViewExecutions(rule)}
          title="View executions"
          className="flex h-7 w-7 items-center justify-center rounded-lg text-white/30 hover:bg-glass-200 hover:text-white/70 transition-all"
        >
          <Activity size={12} />
        </button>
        <button
          onClick={() => onEdit(rule)}
          title="Edit rule"
          className="flex h-7 w-7 items-center justify-center rounded-lg text-white/30 hover:bg-glass-200 hover:text-cyan-neon transition-all"
        >
          <Edit2 size={12} />
        </button>
        <button
          onClick={() => onDelete(rule.id)}
          title="Delete rule"
          className="flex h-7 w-7 items-center justify-center rounded-lg text-white/30 hover:bg-glass-200 hover:text-red-signal transition-all"
        >
          <Trash2 size={12} />
        </button>
        <button
          onClick={handleToggle}
          disabled={toggling}
          title={rule.is_active ? "Disable rule" : "Enable rule"}
          className="flex h-7 w-7 items-center justify-center rounded-lg text-white/30 hover:bg-glass-200 hover:text-white/70 transition-all disabled:opacity-50"
        >
          {rule.is_active
            ? <ToggleRight size={14} className="text-green-signal" />
            : <ToggleLeft size={14} className="text-white/20" />}
        </button>
      </div>
    </motion.div>
  );
}

// ── Rule Builder Modal ─────────────────────────────────────────────────────────

function RuleBuilderModal({
  initial,
  onSave,
  onClose,
}: {
  initial: AutomationRule | null;
  onSave: (payload: CreateRulePayload) => Promise<void>;
  onClose: () => void;
}) {
  const [name,        setName]        = useState(initial?.name ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [trigger,     setTrigger]     = useState<string>(
    initial ? JSON.stringify(initial.trigger) : "DeliveryFailed"
  );
  const [conditions,  setConditions]  = useState<RuleCondition[]>(initial?.conditions ?? []);
  const [pendingType, setPendingType] = useState<string>("");
  const [pendingInput,setPendingInput]= useState<string>("");
  const [segments,    setSegments]    = useState<Segment[]>([]);
  const [loadingSegs, setLoadingSegs] = useState(false);
  const [actionRaws,  setActionRaws]  = useState<string[]>(
    initial ? initial.actions.map((a) => JSON.stringify(a)) : []
  );
  const [campaignId,  setCampaignId]  = useState<string>(() => {
    if (!initial) return "";
    const tc = initial.actions.find((a) => typeof a === "object" && "TriggerCampaign" in (a as object));
    return tc ? (tc as { TriggerCampaign: { campaign_id: string } }).TriggerCampaign.campaign_id : "";
  });
  const [priority,    setPriority]    = useState<number>(initial?.priority ?? 100);
  const [saving,      setSaving]      = useState(false);
  const [error,       setError]       = useState<string | null>(null);

  useEffect(() => {
    setLoadingSegs(true);
    createSegmentsApi().list()
      .then((res) => setSegments(res.segments))
      .catch(() => {})
      .finally(() => setLoadingSegs(false));
  }, []);

  const hasTriggerCampaign = actionRaws.includes(TRIGGER_CAMPAIGN_SENTINEL)
    || actionRaws.some((r) => r.includes("TriggerCampaign"));

  async function handleSave() {
    if (!name.trim()) { setError("Name is required"); return; }
    if (actionRaws.length === 0) { setError("At least one action is required"); return; }
    if (hasTriggerCampaign && !campaignId.trim()) {
      setError("Campaign ID is required for the Trigger Campaign action");
      return;
    }

    // Replace sentinel with the real TriggerCampaign action including the entered campaign_id.
    const resolvedRaws = actionRaws.map((r) =>
      r === TRIGGER_CAMPAIGN_SENTINEL
        ? JSON.stringify({ TriggerCampaign: { campaign_id: campaignId.trim() } })
        : r.includes('"TriggerCampaign"') && campaignId.trim()
          ? JSON.stringify({ TriggerCampaign: { campaign_id: campaignId.trim() } })
          : r
    );

    setSaving(true);
    setError(null);
    try {
      await onSave({
        name: name.trim(),
        description: description.trim() || undefined,
        trigger: parseTrigger(trigger),
        conditions: conditions.length > 0 ? conditions : undefined,
        actions: parseActions(resolvedRaws),
        priority,
      });
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to save rule");
      setSaving(false);
    }
  }

  function addCondition() {
    if (!pendingType || !pendingInput.trim()) return;
    let cond: RuleCondition | null = null;
    switch (pendingType) {
      case "CustomerSegment":
        cond = { CustomerSegment: { segment_id: pendingInput } };
        break;
      case "ServiceType":
        cond = { ServiceType: { equals: pendingInput.trim() } };
        break;
      case "ShipmentValue":
        cond = { ShipmentValue: { greater_than: Number(pendingInput) } };
        break;
      case "AttemptCount":
        cond = { AttemptCount: { lte: Number(pendingInput) } };
        break;
      case "Zone":
        cond = { Zone: { in_zones: pendingInput.split(",").map((z) => z.trim()).filter(Boolean) } };
        break;
    }
    if (cond) {
      setConditions((prev) => [...prev, cond!]);
      setPendingType("");
      setPendingInput("");
    }
  }

  function removeCondition(idx: number) {
    setConditions((prev) => prev.filter((_, i) => i !== idx));
  }

  function addAction(value: string) {
    if (!actionRaws.includes(value)) setActionRaws((prev) => [...prev, value]);
  }

  function removeAction(idx: number) {
    setActionRaws((prev) => prev.filter((_, i) => i !== idx));
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        className="w-full max-w-lg max-h-[90vh] overflow-y-auto"
      >
        <GlassCard>
          {/* Header */}
          <div className="flex items-center justify-between mb-5">
            <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
              <Zap size={14} className="text-cyan-neon" />
              {initial ? "Edit Automation Rule" : "New Automation Rule"}
            </h2>
            <button onClick={onClose} className="text-white/40 hover:text-white/70">
              <X size={16} />
            </button>
          </div>

          <div className="space-y-4">
            {/* Name */}
            <div className="flex flex-col gap-1">
              <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Rule Name</span>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. Notify on Failed Delivery"
                className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/40 transition-colors"
              />
            </div>

            {/* Description */}
            <div className="flex flex-col gap-1">
              <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Description (optional)</span>
              <input
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder="What does this rule do?"
                className="rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-cyan-neon/40 transition-colors"
              />
            </div>

            {/* Trigger */}
            <div className="flex flex-col gap-1">
              <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Trigger Event</span>
              <div className="relative">
                <select
                  value={trigger}
                  onChange={(e) => setTrigger(e.target.value)}
                  className="w-full appearance-none rounded-lg border border-glass-border bg-glass-100 px-3 py-2 pr-8 text-xs text-white outline-none focus:border-cyan-neon/40 transition-colors"
                >
                  {TRIGGER_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value} className="bg-canvas text-white">
                      {opt.label}
                    </option>
                  ))}
                </select>
                <ChevronDown size={12} className="pointer-events-none absolute right-2.5 top-2.5 text-white/30" />
              </div>
            </div>

            {/* Conditions */}
            <div className="flex flex-col gap-2">
              <div className="flex items-center justify-between">
                <span className="text-2xs font-mono text-white/30 uppercase tracking-wider flex items-center gap-1.5">
                  <Filter size={10} className="text-purple-400" />
                  Conditions
                  <span className="text-white/20 normal-case">— optional, all must be true</span>
                </span>
              </div>

              {/* Added conditions list */}
              {conditions.length > 0 && (
                <div className="space-y-1.5">
                  {conditions.map((cond, i) => {
                    const segName = "CustomerSegment" in (cond as object)
                      ? segments.find((s) => s.id === (cond as { CustomerSegment: { segment_id: string } }).CustomerSegment.segment_id)?.name
                      : undefined;
                    return (
                      <div
                        key={i}
                        className="flex items-center justify-between rounded-lg border border-purple-500/20 bg-purple-500/5 px-3 py-2 text-xs"
                      >
                        <div className="flex items-center gap-2 text-white/70">
                          <Filter size={11} className="text-purple-400 flex-shrink-0" />
                          {segName
                            ? `In segment: ${segName}`
                            : conditionLabel(cond)}
                        </div>
                        <button
                          onClick={() => removeCondition(i)}
                          className="text-white/20 hover:text-red-signal transition-colors"
                        >
                          <X size={11} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}

              {/* Condition builder */}
              <div className="space-y-2 rounded-lg border border-dashed border-glass-border p-3">
                <div className="relative">
                  <select
                    value={pendingType}
                    onChange={(e) => { setPendingType(e.target.value); setPendingInput(""); }}
                    className="w-full appearance-none rounded-lg border border-glass-border bg-glass-100 px-3 py-2 pr-8 text-xs text-white outline-none focus:border-purple-400/40 transition-colors"
                  >
                    <option value="" className="bg-canvas">+ Add condition…</option>
                    <option value="CustomerSegment" className="bg-canvas text-white">Customer is in segment</option>
                    <option value="ServiceType" className="bg-canvas text-white">Service type equals</option>
                    <option value="ShipmentValue" className="bg-canvas text-white">Shipment value greater than</option>
                    <option value="AttemptCount" className="bg-canvas text-white">Attempt count ≤</option>
                    <option value="Zone" className="bg-canvas text-white">Delivery zone in list</option>
                  </select>
                  <ChevronDown size={12} className="pointer-events-none absolute right-2.5 top-2.5 text-white/30" />
                </div>

                {pendingType === "CustomerSegment" && (
                  <div className="relative">
                    <select
                      value={pendingInput}
                      onChange={(e) => setPendingInput(e.target.value)}
                      className="w-full appearance-none rounded-lg border border-glass-border bg-glass-100 px-3 py-2 pr-8 text-xs text-white outline-none focus:border-purple-400/40 transition-colors"
                    >
                      <option value="" className="bg-canvas">
                        {loadingSegs ? "Loading segments…" : "Select a segment…"}
                      </option>
                      {segments.map((s) => (
                        <option key={s.id} value={s.id} className="bg-canvas text-white">
                          {s.name} ({s.customer_count} customers)
                        </option>
                      ))}
                    </select>
                    <ChevronDown size={12} className="pointer-events-none absolute right-2.5 top-2.5 text-white/30" />
                  </div>
                )}

                {pendingType === "ServiceType" && (
                  <input
                    value={pendingInput}
                    onChange={(e) => setPendingInput(e.target.value)}
                    placeholder="e.g. balikbayan, standard, cod"
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-purple-400/40 transition-colors"
                  />
                )}

                {pendingType === "ShipmentValue" && (
                  <input
                    type="number"
                    min={0}
                    value={pendingInput}
                    onChange={(e) => setPendingInput(e.target.value)}
                    placeholder="e.g. 5000 (minimum value in pesos)"
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-purple-400/40 transition-colors"
                  />
                )}

                {pendingType === "AttemptCount" && (
                  <input
                    type="number"
                    min={1}
                    max={10}
                    value={pendingInput}
                    onChange={(e) => setPendingInput(e.target.value)}
                    placeholder="e.g. 3 (max attempts before triggering)"
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-purple-400/40 transition-colors"
                  />
                )}

                {pendingType === "Zone" && (
                  <input
                    value={pendingInput}
                    onChange={(e) => setPendingInput(e.target.value)}
                    placeholder="e.g. manila, cebu, davao (comma-separated)"
                    className="w-full rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white placeholder-white/20 outline-none focus:border-purple-400/40 transition-colors"
                  />
                )}

                {pendingType && (
                  <button
                    onClick={addCondition}
                    disabled={!pendingInput.trim()}
                    className="w-full rounded-lg border border-purple-400/30 bg-purple-400/10 py-1.5 text-xs text-purple-400 hover:bg-purple-400/20 transition-colors disabled:opacity-40"
                  >
                    Add Condition
                  </button>
                )}
              </div>
            </div>

            {/* Actions */}
            <div className="flex flex-col gap-2">
              <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Actions</span>

              {actionRaws.length > 0 && (
                <div className="space-y-1.5">
                  {actionRaws.map((raw, i) => {
                    const parsed = parseTrigger(raw);
                    return (
                      <div
                        key={i}
                        className="flex items-center justify-between rounded-lg border border-glass-border bg-glass-100/50 px-3 py-2 text-xs"
                      >
                        <div className="flex items-center gap-2 text-white/70">
                          <ChevronRight size={11} className="text-green-signal flex-shrink-0" />
                          {actionLabel(parsed)}
                        </div>
                        <button
                          onClick={() => removeAction(i)}
                          className="text-white/20 hover:text-red-signal transition-colors"
                        >
                          <X size={11} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}

              <div className="relative">
                <select
                  value=""
                  onChange={(e) => { if (e.target.value) addAction(e.target.value); }}
                  className="w-full appearance-none rounded-lg border border-dashed border-glass-border bg-transparent px-3 py-2 pr-8 text-xs text-white/40 outline-none hover:border-cyan-neon/30 transition-colors"
                >
                  <option value="" className="bg-canvas">+ Add action…</option>
                  {ACTION_TEMPLATES.map((opt) => (
                    <option key={opt.value} value={opt.value} className="bg-canvas text-white">
                      {opt.label}
                    </option>
                  ))}
                </select>
                <ChevronDown size={12} className="pointer-events-none absolute right-2.5 top-2.5 text-white/30" />
              </div>
            </div>

            {/* Campaign ID input — shown when TriggerCampaign action is selected */}
            {hasTriggerCampaign && (
              <div className="flex flex-col gap-1 rounded-lg border border-cyan-neon/20 bg-cyan-neon/5 p-3">
                <span className="text-2xs font-mono text-cyan-neon/70 uppercase tracking-wider">
                  Campaign ID <span className="text-red-signal">*</span>
                </span>
                <input
                  value={campaignId}
                  onChange={(e) => setCampaignId(e.target.value)}
                  placeholder="e.g. 550e8400-e29b-41d4-a716-446655440000"
                  className="rounded-lg border border-cyan-neon/20 bg-glass-100 px-3 py-2 text-xs text-white font-mono placeholder-white/20 outline-none focus:border-cyan-neon/40 transition-colors"
                />
                <p className="text-2xs font-mono text-white/30">
                  Copy the campaign UUID from the Campaigns page.
                </p>
              </div>
            )}

            {/* Priority */}
            <div className="flex flex-col gap-1">
              <div className="flex items-center justify-between">
                <span className="text-2xs font-mono text-white/30 uppercase tracking-wider">Priority</span>
                <span className="text-2xs font-mono text-white/50">{priority} (lower = higher priority)</span>
              </div>
              <input
                type="range"
                min={1}
                max={200}
                step={10}
                value={priority}
                onChange={(e) => setPriority(Number(e.target.value))}
                className="w-full accent-cyan-neon"
              />
            </div>
          </div>

          {error && (
            <p className="mt-3 text-xs text-red-signal font-mono">{error}</p>
          )}

          <div className="flex gap-3 mt-5">
            <button
              onClick={onClose}
              className="flex-1 rounded-lg border border-glass-border py-2 text-xs text-white/50 hover:text-white transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleSave}
              disabled={saving}
              className="flex-1 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 py-2 text-xs text-cyan-neon hover:bg-cyan-neon/20 transition-colors disabled:opacity-50"
            >
              {saving ? "Saving…" : initial ? "Update Rule" : "Create Rule"}
            </button>
          </div>
        </GlassCard>
      </motion.div>
    </div>
  );
}

// ── Execution History Panel ────────────────────────────────────────────────────

function ExecutionPanel({
  rule,
  onClose,
}: {
  rule: AutomationRule;
  onClose: () => void;
}) {
  const api = useMemo(() => getAutomationsApi(), []);
  const [executions, setExecutions] = useState<RuleExecution[]>([]);
  const [loading,    setLoading]    = useState(true);

  useEffect(() => {
    api.executions(rule.id, 30).then((res) => {
      setExecutions(res.data);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, [api, rule.id]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-end bg-black/40 backdrop-blur-sm">
      <motion.div
        initial={{ x: "100%" }}
        animate={{ x: 0 }}
        exit={{ x: "100%" }}
        transition={{ type: "spring", stiffness: 300, damping: 30 }}
        className="h-full w-full max-w-md overflow-y-auto bg-canvas-100 border-l border-glass-border"
      >
        <div className="p-5">
          <div className="flex items-center justify-between mb-5">
            <div>
              <h2 className="font-heading text-sm font-semibold text-white flex items-center gap-2">
                <Activity size={14} className="text-cyan-neon" />
                Execution History
              </h2>
              <p className="text-2xs font-mono text-white/30 mt-0.5 line-clamp-1">{rule.name}</p>
            </div>
            <button onClick={onClose} className="text-white/40 hover:text-white/70">
              <X size={16} />
            </button>
          </div>

          {loading ? (
            <p className="text-xs font-mono text-white/30 text-center py-10">Loading…</p>
          ) : executions.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-10 gap-2">
              <Clock size={24} className="text-white/20" />
              <p className="text-xs font-mono text-white/30">No executions yet</p>
              <p className="text-2xs font-mono text-white/20 text-center">
                This rule will appear here once an event triggers it.
              </p>
            </div>
          ) : (
            <div className="space-y-2">
              {executions.map((exec) => (
                <div
                  key={exec.id}
                  className={`rounded-lg border p-3 text-xs ${
                    exec.outcome === "success"
                      ? "border-green-signal/20 bg-green-signal/5"
                      : "border-red-signal/20 bg-red-signal/5"
                  }`}
                >
                  <div className="flex items-center justify-between gap-2 mb-1.5">
                    <div className="flex items-center gap-1.5">
                      {exec.outcome === "success"
                        ? <CheckCircle2 size={11} className="text-green-signal flex-shrink-0" />
                        : <AlertCircle  size={11} className="text-red-signal flex-shrink-0" />}
                      <span className={`font-mono font-semibold ${exec.outcome === "success" ? "text-green-signal" : "text-red-signal"}`}>
                        {exec.outcome}
                      </span>
                    </div>
                    <span className="text-2xs font-mono text-white/30">{relativeTime(exec.fired_at)}</span>
                  </div>
                  <p className="text-2xs font-mono text-white/40">Topic: {exec.event_topic}</p>
                  {exec.shipment_id && (
                    <p className="text-2xs font-mono text-white/30 truncate">Shipment: {exec.shipment_id}</p>
                  )}
                  {exec.error_message && (
                    <p className="text-2xs font-mono text-red-signal/70 mt-1 truncate">{exec.error_message}</p>
                  )}
                  {exec.actions_taken.length > 0 && (
                    <div className="mt-1.5 flex flex-wrap gap-1">
                      {exec.actions_taken.map((a, i) => (
                        <NeonBadge key={i} variant="muted" className="text-[9px]">{a}</NeonBadge>
                      ))}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </motion.div>
    </div>
  );
}

// ── Page ───────────────────────────────────────────────────────────────────────

export default function AutomationsPage() {
  const api = useMemo(() => getAutomationsApi(), []);

  const [rules,   setRules]   = useState<AutomationRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState<string | null>(null);

  const [builderOpen,  setBuilderOpen]  = useState(false);
  const [editingRule,  setEditingRule]  = useState<AutomationRule | null>(null);
  const [execPanel,    setExecPanel]    = useState<AutomationRule | null>(null);
  const [filterActive, setFilterActive] = useState<boolean | undefined>(undefined);
  const [deleting,     setDeleting]     = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await api.list(1, 100, filterActive);
      setRules(res.data);
    } catch (e) {
      const err = e as { message?: string };
      setError(err?.message ?? "Failed to load automation rules");
    } finally {
      setLoading(false);
    }
  }, [api, filterActive]);

  useEffect(() => { load(); }, [load]);

  async function handleSave(payload: CreateRulePayload) {
    if (editingRule) {
      const updated = await api.update(editingRule.id, payload);
      setRules((prev) => prev.map((r) => r.id === updated.id ? updated : r));
    } else {
      const created = await api.create(payload);
      setRules((prev) => [created, ...prev]);
    }
    setBuilderOpen(false);
    setEditingRule(null);
  }

  async function handleToggle(id: string, active: boolean) {
    const updated = await api.toggle(id, active);
    setRules((prev) => prev.map((r) => r.id === updated.id ? updated : r));
  }

  async function handleDelete(id: string) {
    if (!confirm("Delete this automation rule? This cannot be undone.")) return;
    setDeleting(id);
    try {
      await api.remove(id);
      setRules((prev) => prev.filter((r) => r.id !== id));
    } finally {
      setDeleting(null);
    }
  }

  function openEdit(rule: AutomationRule) {
    setEditingRule(rule);
    setBuilderOpen(true);
  }

  const activeCount   = rules.filter((r) => r.is_active).length;
  const inactiveCount = rules.length - activeCount;

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      {/* Header */}
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between gap-4">
        <div>
          <h1 className="font-heading text-2xl font-bold text-white flex items-center gap-2">
            <Zap size={20} className="text-cyan-neon" />
            Automation Rules
          </h1>
          <p className="text-xs font-mono text-white/30 mt-0.5">
            Event-driven rules that automatically trigger actions when conditions are met
          </p>
        </div>
        <button
          onClick={() => { setEditingRule(null); setBuilderOpen(true); }}
          className="flex items-center gap-2 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 px-4 py-2 text-xs text-cyan-neon hover:bg-cyan-neon/20 transition-all"
        >
          <Plus size={14} />
          New Rule
        </button>
      </motion.div>

      {/* KPI strip */}
      <motion.div variants={variants.fadeInUp} className="grid grid-cols-3 gap-3">
        {[
          { label: "Total Rules",    value: rules.length.toString(),    color: "#00E5FF" },
          { label: "Active",         value: activeCount.toString(),      color: "#00FF88" },
          { label: "Inactive",       value: inactiveCount.toString(),    color: "#A855F7" },
        ].map(({ label, value, color }) => (
          <GlassCard key={label} size="sm">
            <p className="text-2xs font-mono text-white/40 uppercase tracking-wider">{label}</p>
            <p className="font-heading text-xl font-bold mt-1" style={{ color }}>{value}</p>
          </GlassCard>
        ))}
      </motion.div>

      {/* Filters */}
      <motion.div variants={variants.fadeInUp} className="flex items-center gap-2">
        {[
          { label: "All",      value: undefined },
          { label: "Active",   value: true },
          { label: "Inactive", value: false },
        ].map(({ label, value }) => (
          <button
            key={label}
            onClick={() => setFilterActive(value)}
            className={`rounded-lg px-3 py-1.5 text-xs font-mono transition-all ${
              filterActive === value
                ? "border border-cyan-neon/40 bg-cyan-neon/10 text-cyan-neon"
                : "border border-glass-border text-white/40 hover:text-white/70"
            }`}
          >
            {label}
          </button>
        ))}
        <button
          onClick={load}
          className="ml-auto rounded-lg border border-glass-border px-3 py-1.5 text-xs text-white/40 hover:text-white/70 transition-colors"
        >
          Refresh
        </button>
      </motion.div>

      {/* Rule list */}
      <motion.div variants={variants.fadeInUp}>
        {loading ? (
          <GlassCard>
            <div className="flex items-center justify-center py-10">
              <p className="text-xs font-mono text-white/30">Loading rules…</p>
            </div>
          </GlassCard>
        ) : error ? (
          <GlassCard className="border-red-signal/20">
            <div className="flex flex-col items-center justify-center py-8 gap-2">
              <AlertCircle size={20} className="text-red-signal" />
              <p className="text-xs font-mono text-red-signal">{error}</p>
              <button onClick={load} className="text-xs text-cyan-neon hover:underline">Try again</button>
            </div>
          </GlassCard>
        ) : rules.length === 0 ? (
          <GlassCard>
            <div className="flex flex-col items-center justify-center py-12 gap-3">
              <div className="flex h-12 w-12 items-center justify-center rounded-xl border border-glass-border bg-glass-100">
                <Code2 size={20} className="text-white/20" />
              </div>
              <p className="text-sm font-semibold text-white/60">No automation rules yet</p>
              <p className="text-xs font-mono text-white/30 text-center max-w-xs">
                Create a rule to automatically send campaigns, notify customers, or
                reschedule deliveries when events occur.
              </p>
              <button
                onClick={() => { setEditingRule(null); setBuilderOpen(true); }}
                className="mt-1 flex items-center gap-2 rounded-lg bg-cyan-neon/10 border border-cyan-neon/30 px-4 py-2 text-xs text-cyan-neon hover:bg-cyan-neon/20 transition-all"
              >
                <Plus size={12} />
                Create First Rule
              </button>
            </div>
          </GlassCard>
        ) : (
          <div className="flex flex-col gap-2">
            <AnimatePresence mode="popLayout">
              {rules.map((rule) => (
                <RuleRow
                  key={rule.id}
                  rule={deleting === rule.id ? { ...rule, is_active: false } : rule}
                  onToggle={handleToggle}
                  onEdit={openEdit}
                  onDelete={handleDelete}
                  onViewExecutions={setExecPanel}
                />
              ))}
            </AnimatePresence>
          </div>
        )}
      </motion.div>

      {/* Rule Builder Modal */}
      <AnimatePresence>
        {builderOpen && (
          <RuleBuilderModal
            initial={editingRule}
            onSave={handleSave}
            onClose={() => { setBuilderOpen(false); setEditingRule(null); }}
          />
        )}
      </AnimatePresence>

      {/* Execution History Drawer */}
      <AnimatePresence>
        {execPanel && (
          <ExecutionPanel
            rule={execPanel}
            onClose={() => setExecPanel(null)}
          />
        )}
      </AnimatePresence>
    </motion.div>
  );
}
