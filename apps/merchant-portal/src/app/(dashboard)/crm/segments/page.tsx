"use client";
/**
 * CRM → Segments Page
 * Route: /crm/segments
 *
 * Manage dynamic customer segments (High CLV, At Risk, Inactive, VIP, custom).
 * Each segment is a named, saveable SegmentFilter that queries customer_profiles
 * in real-time. Live preview counts are returned by the CDP preview endpoint.
 */
import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import {
  createSegmentsApi,
  type Segment,
  type SegmentFilter,
  type SegmentMember,
  type SegmentPreview,
} from "@/lib/api/segments";
import {
  Users2, Plus, Trash2, Edit2, X, Eye, RefreshCcw,
  ChevronRight, TrendingUp, AlertTriangle, Clock, Star,
  Loader2, UserCheck, ChevronDown, ChevronUp,
} from "lucide-react";

// ── helpers ────────────────────────────────────────────────────────────────────

const api = createSegmentsApi();

function clvTierColor(score: number) {
  if (score >= 80) return "text-[#FFD700]";
  if (score >= 60) return "text-[#00E5FF]";
  if (score >= 40) return "text-[#00FF88]";
  return "text-gray-400";
}

function segmentIcon(name: string) {
  const n = name.toLowerCase();
  if (n.includes("vip") || n.includes("high clv")) return <Star className="w-4 h-4 text-[#FFD700]" />;
  if (n.includes("risk") || n.includes("churn"))   return <AlertTriangle className="w-4 h-4 text-[#FF4444]" />;
  if (n.includes("inact"))                          return <Clock className="w-4 h-4 text-[#FFAB00]" />;
  return <Users2 className="w-4 h-4 text-[#00E5FF]" />;
}

const CHURN_TIERS = [
  { value: "", label: "Any" },
  { value: "high",    label: "High" },
  { value: "medium",  label: "Medium" },
  { value: "low",     label: "Low" },
  { value: "healthy", label: "Healthy" },
];

const PROFILE_TYPES = [
  { value: "", label: "Any" },
  { value: "sender",   label: "Sender" },
  { value: "receiver", label: "Receiver" },
  { value: "unknown",  label: "Unknown" },
];

// ── SegmentBuilderModal ────────────────────────────────────────────────────────

interface BuilderProps {
  initial?: Segment | null;
  onClose: () => void;
  onSaved: (seg: Segment) => void;
}

function SegmentBuilderModal({ initial, onClose, onSaved }: BuilderProps) {
  const [name, setName]         = useState(initial?.name ?? "");
  const [desc, setDesc]         = useState(initial?.description ?? "");
  const [filter, setFilter]     = useState<SegmentFilter>(initial?.filter ?? {});
  const [preview, setPreview]   = useState<SegmentPreview | null>(null);
  const [prevLoading, setPrevL] = useState(false);
  const [saving, setSaving]     = useState(false);
  const [error, setError]       = useState<string | null>(null);

  const runPreview = useCallback(async () => {
    setPrevL(true);
    try {
      const p = await api.previewFilter(filter);
      setPreview(p);
    } finally {
      setPrevL(false);
    }
  }, [filter]);

  const handleSave = async () => {
    if (!name.trim()) { setError("Segment name is required."); return; }
    setSaving(true);
    setError(null);
    try {
      const seg = initial
        ? await api.update(initial.id, { name, description: desc, filter })
        : await api.create({ name, description: desc, filter });
      onSaved(seg);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const setF = <K extends keyof SegmentFilter>(k: K, v: SegmentFilter[K]) =>
    setFilter((f) => ({ ...f, [k]: (v as unknown) === "" || v === undefined ? null : v }));

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/70 backdrop-blur-sm">
      <motion.div
        initial={{ opacity: 0, scale: 0.95 }}
        animate={{ opacity: 1, scale: 1 }}
        exit={{ opacity: 0, scale: 0.95 }}
        className="w-full max-w-2xl max-h-[90vh] overflow-y-auto rounded-2xl border border-white/10 bg-[#0a0f1e]/95 backdrop-blur-xl shadow-2xl shadow-[#00E5FF]/10 p-6"
      >
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-lg font-semibold text-white font-['Space_Grotesk']">
            {initial ? "Edit Segment" : "New Segment"}
          </h2>
          <button onClick={onClose} className="text-gray-400 hover:text-white transition-colors">
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Name + Description */}
        <div className="space-y-4 mb-6">
          <div>
            <label className="block text-xs text-gray-400 mb-1.5 font-['Geist']">Segment Name *</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. High CLV Senders"
              className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50 font-['Geist']"
            />
          </div>
          <div>
            <label className="block text-xs text-gray-400 mb-1.5 font-['Geist']">Description</label>
            <input
              value={desc}
              onChange={(e) => setDesc(e.target.value)}
              placeholder="Optional description…"
              className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50 font-['Geist']"
            />
          </div>
        </div>

        {/* Filter rules */}
        <div className="mb-6">
          <h3 className="text-sm font-medium text-white/80 mb-3 font-['Space_Grotesk']">Filter Criteria</h3>
          <div className="grid grid-cols-2 gap-3">
            {/* CLV range */}
            <div className="col-span-2 grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Min CLV Score (0-100)</label>
                <input
                  type="number" min={0} max={100} step={5}
                  value={filter.min_clv ?? ""}
                  onChange={(e) => setF("min_clv", e.target.value ? parseFloat(e.target.value) : null)}
                  placeholder="None"
                  className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
                />
              </div>
              <div>
                <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Max CLV Score</label>
                <input
                  type="number" min={0} max={100} step={5}
                  value={filter.max_clv ?? ""}
                  onChange={(e) => setF("max_clv", e.target.value ? parseFloat(e.target.value) : null)}
                  placeholder="None"
                  className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
                />
              </div>
            </div>

            {/* Engagement */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Min Engagement (0-100)</label>
              <input
                type="number" min={0} max={100} step={5}
                value={filter.min_engagement ?? ""}
                onChange={(e) => setF("min_engagement", e.target.value ? parseFloat(e.target.value) : null)}
                placeholder="None"
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
              />
            </div>

            {/* Inactive days */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Inactive Days ≥</label>
              <input
                type="number" min={1}
                value={filter.inactive_days ?? ""}
                onChange={(e) => setF("inactive_days", e.target.value ? parseInt(e.target.value) : null)}
                placeholder="None"
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
              />
            </div>

            {/* Churn tier */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Churn Tier</label>
              <select
                value={filter.churn_tier ?? ""}
                onChange={(e) => setF("churn_tier", (e.target.value || null) as SegmentFilter["churn_tier"])}
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-[#00E5FF]/50"
              >
                {CHURN_TIERS.map((t) => <option key={t.value} value={t.value}>{t.label}</option>)}
              </select>
            </div>

            {/* Max churn score */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Max Churn Score (0-1)</label>
              <input
                type="number" min={0} max={1} step={0.05}
                value={filter.max_churn_score ?? ""}
                onChange={(e) => setF("max_churn_score", e.target.value ? parseFloat(e.target.value) : null)}
                placeholder="None"
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
              />
            </div>

            {/* Profile type */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Profile Type</label>
              <select
                value={filter.profile_type ?? ""}
                onChange={(e) => setF("profile_type", (e.target.value || null) as SegmentFilter["profile_type"])}
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm focus:outline-none focus:border-[#00E5FF]/50"
              >
                {PROFILE_TYPES.map((t) => <option key={t.value} value={t.value}>{t.label}</option>)}
              </select>
            </div>

            {/* Min shipments */}
            <div>
              <label className="block text-xs text-gray-400 mb-1 font-['Geist']">Min Shipments</label>
              <input
                type="number" min={0}
                value={filter.min_shipments ?? ""}
                onChange={(e) => setF("min_shipments", e.target.value ? parseInt(e.target.value) : null)}
                placeholder="None"
                className="w-full px-3 py-2 rounded-lg bg-white/5 border border-white/10 text-white text-sm placeholder-gray-500 focus:outline-none focus:border-[#00E5FF]/50"
              />
            </div>
          </div>
        </div>

        {/* Live preview */}
        <div className="mb-6 p-4 rounded-xl border border-white/5 bg-white/[0.02]">
          <div className="flex items-center justify-between mb-3">
            <span className="text-xs text-gray-400 font-['Geist']">Live Preview</span>
            <button
              onClick={runPreview}
              disabled={prevLoading}
              className="flex items-center gap-1.5 text-xs text-[#00E5FF] hover:text-[#00E5FF]/70 transition-colors disabled:opacity-50"
            >
              {prevLoading ? <Loader2 className="w-3 h-3 animate-spin" /> : <RefreshCcw className="w-3 h-3" />}
              Preview audience
            </button>
          </div>
          {preview ? (
            <div>
              <p className="text-2xl font-bold text-white font-['Space_Grotesk'] mb-2">
                {preview.estimated_count.toLocaleString()}
                <span className="text-sm text-gray-400 font-normal ml-2">customers match</span>
              </p>
              {preview.sample_members.length > 0 && (
                <div className="space-y-1.5 mt-3">
                  <p className="text-xs text-gray-500">Sample members:</p>
                  {preview.sample_members.slice(0, 3).map((m) => (
                    <div key={m.external_customer_id} className="flex items-center justify-between text-xs">
                      <span className="text-gray-300">{m.name ?? m.email ?? m.external_customer_id.slice(0, 8) + "…"}</span>
                      <span className={clvTierColor(m.clv_score)}>CLV {m.clv_score.toFixed(0)}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : (
            <p className="text-xs text-gray-500">Click &quot;Preview audience&quot; to see estimated matches.</p>
          )}
        </div>

        {error && (
          <p className="text-xs text-red-400 mb-4 font-['Geist']">{error}</p>
        )}

        {/* Actions */}
        <div className="flex items-center justify-end gap-3">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-gray-400 hover:text-white transition-colors font-['Geist']"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={saving}
            className="px-5 py-2 rounded-lg bg-[#00E5FF]/10 border border-[#00E5FF]/30 text-[#00E5FF] text-sm hover:bg-[#00E5FF]/20 transition-all font-['Geist'] disabled:opacity-50 flex items-center gap-2"
          >
            {saving && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
            {initial ? "Update Segment" : "Create Segment"}
          </button>
        </div>
      </motion.div>
    </div>
  );
}

// ── MembersDrawer ──────────────────────────────────────────────────────────────

function MembersDrawer({ segment, onClose }: { segment: Segment; onClose: () => void }) {
  const [members, setMembers] = useState<SegmentMember[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.members(segment.id).then((r) => { setMembers(r.members); setLoading(false); });
  }, [segment.id]);

  return (
    <div className="fixed inset-0 z-50 flex items-end sm:items-center justify-center p-0 sm:p-4 bg-black/60 backdrop-blur-sm">
      <motion.div
        initial={{ y: "100%", opacity: 0 }}
        animate={{ y: 0, opacity: 1 }}
        exit={{ y: "100%", opacity: 0 }}
        className="w-full sm:max-w-xl max-h-[80vh] overflow-y-auto rounded-t-2xl sm:rounded-2xl border border-white/10 bg-[#0a0f1e]/95 backdrop-blur-xl shadow-2xl p-6"
      >
        <div className="flex items-center justify-between mb-5">
          <div className="flex items-center gap-2">
            <UserCheck className="w-5 h-5 text-[#00E5FF]" />
            <h2 className="text-lg font-semibold text-white font-['Space_Grotesk']">{segment.name}</h2>
            <span className="text-xs text-gray-400 font-['JetBrains_Mono']">
              {segment.customer_count.toLocaleString()} members
            </span>
          </div>
          <button onClick={onClose} className="text-gray-400 hover:text-white transition-colors">
            <X className="w-5 h-5" />
          </button>
        </div>

        {loading ? (
          <div className="flex items-center justify-center h-32">
            <Loader2 className="w-6 h-6 text-[#00E5FF] animate-spin" />
          </div>
        ) : members.length === 0 ? (
          <p className="text-sm text-gray-400 text-center py-12 font-['Geist']">No members match this segment yet.</p>
        ) : (
          <div className="space-y-2">
            {members.map((m) => (
              <div
                key={m.external_customer_id}
                className="flex items-center justify-between p-3 rounded-lg bg-white/[0.03] border border-white/5 hover:border-white/10 transition-colors"
              >
                <div className="min-w-0">
                  <p className="text-sm text-white truncate font-['Geist']">{m.name ?? "—"}</p>
                  <p className="text-xs text-gray-400 truncate font-['Geist']">{m.email ?? m.phone ?? m.external_customer_id.slice(0, 12) + "…"}</p>
                </div>
                <div className="flex items-center gap-3 ml-4 shrink-0">
                  <div className="text-right">
                    <p className={`text-sm font-semibold font-['JetBrains_Mono'] ${clvTierColor(m.clv_score)}`}>{m.clv_score.toFixed(0)}</p>
                    <p className="text-xs text-gray-500">CLV</p>
                  </div>
                  <div className="text-right">
                    <p className="text-sm font-semibold text-[#00FF88] font-['JetBrains_Mono']">{m.engagement_score.toFixed(0)}</p>
                    <p className="text-xs text-gray-500">Eng.</p>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </motion.div>
    </div>
  );
}

// ── SegmentCard ────────────────────────────────────────────────────────────────

interface CardProps {
  segment: Segment;
  onEdit: () => void;
  onDelete: () => void;
  onViewMembers: () => void;
}

function SegmentCard({ segment, onEdit, onDelete, onViewMembers }: CardProps) {
  const [expanded, setExpanded] = useState(false);

  const filterSummary = (() => {
    const f = segment.filter;
    const parts: string[] = [];
    if (f.min_clv != null)       parts.push(`CLV ≥ ${f.min_clv}`);
    if (f.max_clv != null)       parts.push(`CLV ≤ ${f.max_clv}`);
    if (f.min_engagement != null) parts.push(`Eng ≥ ${f.min_engagement}`);
    if (f.churn_tier)            parts.push(`Churn: ${f.churn_tier}`);
    if (f.inactive_days != null) parts.push(`Inactive ${f.inactive_days}d+`);
    if (f.profile_type)          parts.push(f.profile_type);
    if (f.min_shipments != null) parts.push(`≥${f.min_shipments} shipments`);
    return parts.length ? parts.join(" · ") : "All customers";
  })();

  return (
    <motion.div layout variants={variants.fadeInUp} initial="hidden" animate="visible">
      <GlassCard className="p-5 hover:border-white/15 transition-all group">
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3 min-w-0">
            <div className="w-9 h-9 rounded-xl bg-white/5 border border-white/10 flex items-center justify-center shrink-0">
              {segmentIcon(segment.name)}
            </div>
            <div className="min-w-0">
              <h3 className="text-sm font-semibold text-white font-['Space_Grotesk'] truncate">{segment.name}</h3>
              {segment.description && (
                <p className="text-xs text-gray-400 truncate font-['Geist'] mt-0.5">{segment.description}</p>
              )}
            </div>
          </div>

          <div className="flex items-center gap-2 shrink-0">
            <NeonBadge variant="cyan">
              <span className="font-['JetBrains_Mono'] text-xs">{segment.customer_count.toLocaleString()}</span>
            </NeonBadge>
          </div>
        </div>

        {/* Filter summary */}
        <p className="text-xs text-gray-500 mt-3 font-['JetBrains_Mono'] truncate">{filterSummary}</p>

        {/* Expand rules detail */}
        <button
          onClick={() => setExpanded((p) => !p)}
          className="flex items-center gap-1 text-xs text-gray-500 hover:text-gray-300 mt-2 transition-colors"
        >
          {expanded ? <ChevronUp className="w-3 h-3" /> : <ChevronDown className="w-3 h-3" />}
          {expanded ? "Hide" : "Show"} filter details
        </button>

        <AnimatePresence>
          {expanded && (
            <motion.div
              initial={{ height: 0, opacity: 0 }}
              animate={{ height: "auto", opacity: 1 }}
              exit={{ height: 0, opacity: 0 }}
              className="overflow-hidden"
            >
              <div className="mt-3 p-3 rounded-lg bg-white/[0.02] border border-white/5 space-y-1.5">
                {Object.entries(segment.filter).map(([k, v]) => {
                  if (v === null || v === undefined) return null;
                  return (
                    <div key={k} className="flex items-center justify-between text-xs font-['JetBrains_Mono']">
                      <span className="text-gray-500">{k}</span>
                      <span className="text-[#00E5FF]">{String(v)}</span>
                    </div>
                  );
                })}
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* Actions */}
        <div className="flex items-center gap-2 mt-4 pt-3 border-t border-white/5">
          <button
            onClick={onViewMembers}
            className="flex items-center gap-1.5 text-xs text-gray-400 hover:text-[#00E5FF] transition-colors"
          >
            <Eye className="w-3.5 h-3.5" />
            Members
          </button>
          <div className="flex-1" />
          <button
            onClick={onEdit}
            className="p-1.5 rounded-lg text-gray-500 hover:text-[#00E5FF] hover:bg-[#00E5FF]/10 transition-all"
          >
            <Edit2 className="w-3.5 h-3.5" />
          </button>
          <button
            onClick={onDelete}
            className="p-1.5 rounded-lg text-gray-500 hover:text-red-400 hover:bg-red-400/10 transition-all"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </button>
        </div>
      </GlassCard>
    </motion.div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function SegmentsPage() {
  const [segments, setSegments]         = useState<Segment[]>([]);
  const [loading, setLoading]           = useState(true);
  const [showBuilder, setShowBuilder]   = useState(false);
  const [editTarget, setEditTarget]     = useState<Segment | null>(null);
  const [viewTarget, setViewTarget]     = useState<Segment | null>(null);
  const [seeding, setSeeding]           = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const { segments: segs } = await api.list();
      setSegments(segs);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleDelete = async (id: string) => {
    await api.delete(id);
    setSegments((prev) => prev.filter((s) => s.id !== id));
  };

  const handleSaved = (seg: Segment) => {
    setSegments((prev) => {
      const idx = prev.findIndex((s) => s.id === seg.id);
      if (idx >= 0) { const next = [...prev]; next[idx] = seg; return next; }
      return [seg, ...prev];
    });
    setShowBuilder(false);
    setEditTarget(null);
  };

  const handleSeedDefaults = async () => {
    setSeeding(true);
    try {
      await api.seedDefaults();
      await load();
    } finally {
      setSeeding(false);
    }
  };

  // KPI derived data
  const totalCustomers = segments.reduce((a, s) => a + s.customer_count, 0);
  const highClv  = segments.find((s) => s.name.toLowerCase().includes("high clv") || s.name.toLowerCase().includes("vip"));
  const atRisk   = segments.find((s) => s.name.toLowerCase().includes("risk") || s.name.toLowerCase().includes("churn"));
  const inactive = segments.find((s) => s.name.toLowerCase().includes("inact"));

  return (
    <div className="min-h-screen bg-[#050810] p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-white font-['Space_Grotesk']">Customer Segments</h1>
          <p className="text-sm text-gray-400 mt-1 font-['Geist']">
            Dynamic audience groups for targeting and automation rules.
          </p>
        </div>
        <div className="flex items-center gap-3">
          {segments.length === 0 && !loading && (
            <button
              onClick={handleSeedDefaults}
              disabled={seeding}
              className="flex items-center gap-2 px-4 py-2 rounded-xl text-sm text-[#A855F7] border border-[#A855F7]/30 hover:bg-[#A855F7]/10 transition-all font-['Geist'] disabled:opacity-50"
            >
              {seeding ? <Loader2 className="w-4 h-4 animate-spin" /> : <RefreshCcw className="w-4 h-4" />}
              Seed Defaults
            </button>
          )}
          <button
            onClick={() => { setEditTarget(null); setShowBuilder(true); }}
            className="flex items-center gap-2 px-4 py-2 rounded-xl text-sm text-[#00E5FF] border border-[#00E5FF]/30 hover:bg-[#00E5FF]/10 transition-all font-['Geist']"
          >
            <Plus className="w-4 h-4" />
            New Segment
          </button>
        </div>
      </div>

      {/* KPI strip */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4">
        {[
          {
            label: "Total Segments", value: segments.length, icon: Users2,
            color: "text-[#00E5FF]", glow: "shadow-[#00E5FF]/20",
          },
          {
            label: "Segmented Customers", value: totalCustomers.toLocaleString(), icon: TrendingUp,
            color: "text-[#00FF88]", glow: "shadow-[#00FF88]/20",
          },
          {
            label: "High Value", value: (highClv?.customer_count ?? 0).toLocaleString(), icon: Star,
            color: "text-[#FFD700]", glow: "shadow-[#FFD700]/20",
          },
          {
            label: "At Risk", value: (atRisk?.customer_count ?? 0).toLocaleString(), icon: AlertTriangle,
            color: "text-red-400", glow: "shadow-red-400/20",
          },
        ].map((kpi) => (
          <GlassCard key={kpi.label} className={`p-4 shadow-lg ${kpi.glow}`}>
            <div className="flex items-center gap-2 mb-2">
              <kpi.icon className={`w-4 h-4 ${kpi.color}`} />
              <span className="text-xs text-gray-400 font-['Geist']">{kpi.label}</span>
            </div>
            <p className={`text-2xl font-bold font-['Space_Grotesk'] ${kpi.color}`}>{kpi.value}</p>
          </GlassCard>
        ))}
      </div>

      {/* Segment list */}
      {loading ? (
        <div className="flex items-center justify-center h-48">
          <Loader2 className="w-8 h-8 text-[#00E5FF] animate-spin" />
        </div>
      ) : segments.length === 0 ? (
        <GlassCard className="p-12 text-center">
          <Users2 className="w-12 h-12 text-gray-600 mx-auto mb-4" />
          <p className="text-gray-400 font-['Geist'] mb-2">No segments yet.</p>
          <p className="text-sm text-gray-500 font-['Geist'] mb-6">
            Create a segment to group customers by CLV, engagement, churn risk, or activity.
          </p>
          <button
            onClick={handleSeedDefaults}
            disabled={seeding}
            className="inline-flex items-center gap-2 px-5 py-2 rounded-xl text-sm text-[#A855F7] border border-[#A855F7]/30 hover:bg-[#A855F7]/10 transition-all font-['Geist']"
          >
            {seeding ? <Loader2 className="w-4 h-4 animate-spin" /> : <RefreshCcw className="w-4 h-4" />}
            Create 4 Default Segments
          </button>
        </GlassCard>
      ) : (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {segments.map((seg) => (
            <SegmentCard
              key={seg.id}
              segment={seg}
              onEdit={() => { setEditTarget(seg); setShowBuilder(true); }}
              onDelete={() => handleDelete(seg.id)}
              onViewMembers={() => setViewTarget(seg)}
            />
          ))}
        </div>
      )}

      {/* Builder modal */}
      <AnimatePresence>
        {showBuilder && (
          <SegmentBuilderModal
            initial={editTarget}
            onClose={() => { setShowBuilder(false); setEditTarget(null); }}
            onSaved={handleSaved}
          />
        )}
      </AnimatePresence>

      {/* Members drawer */}
      <AnimatePresence>
        {viewTarget && (
          <MembersDrawer segment={viewTarget} onClose={() => setViewTarget(null)} />
        )}
      </AnimatePresence>
    </div>
  );
}
