"use client";
/**
 * Admin Portal — Compliance Console
 * KPI strip + review queue + document detail panel.
 */
import { useState, useEffect, useCallback } from "react";
import { motion } from "framer-motion";
import { ShieldCheck, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { ComplianceKpiStrip } from "@/components/compliance/kpi-strip";
import { ReviewQueue } from "@/components/compliance/review-queue";
import { DocumentDetailPanel } from "@/components/compliance/document-detail-panel";
import {
  fetchReviewQueue,
  fetchProfiles,
  fetchDocumentTypes,
  approveDocument,
  rejectDocument,
  suspendProfile,
  reinstateProfile,
  type PendingReviewItem,
  type ComplianceProfile,
} from "@/lib/api/compliance";
import { fetchCouriers } from "@/lib/api/couriers";
import { buildEntityNames, buildTypeNames } from "@/lib/compliance/labels";
import { usePermissions } from "@/hooks/usePermissions";

// -- Mock data (Storybook / local dev without backend) -------------------------

const MOCK_PROFILES: ComplianceProfile[] = [
  { id: "p1", entity_type: "driver", entity_id: "drv-001", overall_status: "compliant",          jurisdiction: "PH-NCR", last_reviewed_at: "2026-03-20T08:00:00Z", suspended_at: null },
  { id: "p2", entity_type: "driver", entity_id: "drv-002", overall_status: "under_review",       jurisdiction: "PH-NCR", last_reviewed_at: null,                    suspended_at: null },
  { id: "p3", entity_type: "driver", entity_id: "drv-003", overall_status: "expiring_soon",      jurisdiction: "PH-CV",  last_reviewed_at: "2026-02-15T10:30:00Z", suspended_at: null },
  { id: "p4", entity_type: "driver", entity_id: "drv-004", overall_status: "suspended",          jurisdiction: "PH-NCR", last_reviewed_at: "2026-01-10T14:00:00Z", suspended_at: "2026-03-01T00:00:00Z" },
  { id: "p5", entity_type: "driver", entity_id: "drv-005", overall_status: "pending_submission", jurisdiction: "PH-RM",  last_reviewed_at: null,                    suspended_at: null },
  { id: "p6", entity_type: "driver", entity_id: "drv-006", overall_status: "compliant",          jurisdiction: "PH-NCR", last_reviewed_at: "2026-03-22T09:15:00Z", suspended_at: null },
  { id: "p7", entity_type: "driver", entity_id: "drv-007", overall_status: "under_review",       jurisdiction: "PH-NCR", last_reviewed_at: null,                    suspended_at: null },
];

const MOCK_QUEUE: PendingReviewItem[] = [
  { id: "doc-1", compliance_profile_id: "p2", entity_id: "drv-002", entity_type: "driver", jurisdiction: "PH-NCR", overall_status: "under_review", document_type_id: "dt-license",    document_number: "LTO-2024-789012", expiry_date: "2027-06-30", file_url: "#", status: "submitted",    rejection_reason: null, reviewed_by: null, reviewed_at: null, submitted_at: "2026-03-25T10:00:00Z" },
  { id: "doc-2", compliance_profile_id: "p7", entity_id: "drv-007", entity_type: "driver", jurisdiction: "PH-NCR", overall_status: "under_review", document_type_id: "dt-insurance",  document_number: "INS-2026-003",    expiry_date: "2027-03-31", file_url: "#", status: "under_review", rejection_reason: null, reviewed_by: null, reviewed_at: null, submitted_at: "2026-03-24T15:30:00Z" },
  { id: "doc-3", compliance_profile_id: "p2", entity_id: "drv-002", entity_type: "driver", jurisdiction: "PH-NCR", overall_status: "under_review", document_type_id: "dt-vehicle-reg", document_number: "LTO-REG-456",     expiry_date: "2026-12-31", file_url: "#", status: "submitted",    rejection_reason: null, reviewed_by: null, reviewed_at: null, submitted_at: "2026-03-25T11:20:00Z" },
];

export default function CompliancePage() {
  const [queue,           setQueue]           = useState<PendingReviewItem[]>([]);
  const [profiles,        setProfiles]        = useState<ComplianceProfile[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);
  const [loading,         setLoading]         = useState(true);
  const [error,           setError]           = useState<string | null>(null);
  const [panelRefreshKey, setPanelRefreshKey] = useState(0);
  const [typeNames,       setTypeNames]       = useState<Map<string, string>>(new Map());
  const [entityNames,     setEntityNames]     = useState<Map<string, string>>(new Map());

  const { hasPermission } = usePermissions();
  const canAdmin = hasPermission("compliance:admin");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [q, p] = await Promise.all([fetchReviewQueue(), fetchProfiles()]);
      setQueue(q);
      setProfiles(p);
    } catch (e) {
      const msg = e instanceof Error ? e.message : "Failed to load compliance data";
      setError(msg);
      setQueue((prev) => (prev.length === 0 ? MOCK_QUEUE : prev));
      setProfiles((prev) => (prev.length === 0 ? MOCK_PROFILES : prev));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { refresh(); }, [refresh]);

  /**
   * The two lookups that turn uuids into words, fetched once rather than on the
   * 30-second refresh: the document-type catalogue is seeded by migration and
   * does not move, and a courier's name changes far more slowly than their
   * document status.
   *
   * Both failures are swallowed on purpose. Neither is the reviewer's job — a
   * roster this console cannot reach costs them names, not the queue, and
   * `entityLabel` and `typeLabel` fall back to short ids. Surfacing it in the
   * page's error banner would report the compliance service as broken when it
   * is answering perfectly.
   */
  useEffect(() => {
    fetchDocumentTypes()
      .then((t) => setTypeNames(buildTypeNames(t)))
      .catch(() => {});
    fetchCouriers()
      .then((c) => setEntityNames(buildEntityNames(c)))
      .catch(() => {});
  }, []);

  useEffect(() => {
    const id = setInterval(refresh, 30_000);
    return () => clearInterval(id);
  }, [refresh]);

  async function handleApprove(docId: string) {
    const tid = toast.loading("Approving document...");
    setQueue((prev) => prev.filter((d) => d.id !== docId));
    try {
      await approveDocument(docId);
      toast.success("Document approved", { id: tid });
      setPanelRefreshKey((k) => k + 1);
      refresh();
    } catch (err) {
      toast.error((err as Error).message ?? "Approval failed", { id: tid });
      refresh();
    }
  }

  async function handleReject(docId: string, reason: string) {
    const tid = toast.loading("Rejecting document...");
    setQueue((prev) => prev.filter((d) => d.id !== docId));
    try {
      await rejectDocument(docId, reason);
      toast.success("Document rejected", { id: tid });
      setPanelRefreshKey((k) => k + 1);
      refresh();
    } catch (err) {
      toast.error((err as Error).message ?? "Rejection failed", { id: tid });
      refresh();
    }
  }

  async function handleSuspend(profileId: string, reason?: string) {
    const tid = toast.loading("Suspending driver profile...");
    try {
      await suspendProfile(profileId, reason);
      toast.success("Driver suspended", { id: tid });
      setPanelRefreshKey((k) => k + 1);
      refresh();
    } catch (err) {
      toast.error((err as Error).message ?? "Suspend failed", { id: tid });
    }
  }

  async function handleReinstate(profileId: string) {
    const tid = toast.loading("Reinstating driver profile...");
    try {
      await reinstateProfile(profileId);
      toast.success("Driver reinstated", { id: tid });
      setPanelRefreshKey((k) => k + 1);
      refresh();
    } catch (err) {
      toast.error((err as Error).message ?? "Reinstate failed", { id: tid });
    }
  }

  return (
    <motion.div
      variants={variants.staggerContainer}
      initial="hidden"
      animate="visible"
      className="flex flex-col gap-5 p-6"
    >
      <motion.div variants={variants.fadeInUp} className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="w-9 h-9 rounded-xl bg-cyan-surface/20 border border-cyan-glow/25 flex items-center justify-center">
            <ShieldCheck className="h-4 w-4 text-cyan-neon" />
          </div>
          <div>
            <h1 className="font-heading text-2xl font-bold text-white">Compliance</h1>
            <p className="text-sm text-white/40 font-mono mt-0.5">
              {queue.length} pending review {"·"} {profiles.length} total drivers
            </p>
          </div>
        </div>
        <button
          onClick={refresh}
          disabled={loading}
          className="flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-2 text-xs text-white/60 hover:text-white transition-colors disabled:opacity-50"
        >
          <RefreshCw size={12} className={loading ? "animate-spin" : ""} /> Refresh
        </button>
      </motion.div>

      {error && (
        <motion.div variants={variants.fadeInUp}>
          <div className="rounded-lg border border-red-signal/30 bg-red-signal/5 px-4 py-3 text-xs text-red-signal font-mono">
            {error} {"—"}{" "}
            <button onClick={refresh} className="underline hover:text-white transition-colors">
              Retry
            </button>
          </div>
        </motion.div>
      )}

      <motion.div variants={variants.fadeInUp}>
        <ComplianceKpiStrip profiles={profiles} />
      </motion.div>

      <motion.div variants={variants.fadeInUp} className="flex gap-4 h-[600px]">
        <ReviewQueue
          items={queue}
          selectedId={selectedProfile}
          onSelect={setSelectedProfile}
          typeNames={typeNames}
          entityNames={entityNames}
        />

        {selectedProfile ? (
          <DocumentDetailPanel
            profileId={selectedProfile}
            refreshKey={panelRefreshKey}
            onApprove={handleApprove}
            onReject={handleReject}
            onSuspend={canAdmin ? handleSuspend : undefined}
            onReinstate={canAdmin ? handleReinstate : undefined}
            typeNames={typeNames}
            entityNames={entityNames}
          />
        ) : (
          <GlassCard className="flex-1 flex items-center justify-center">
            <div className="text-center text-white/25">
              <ShieldCheck className="h-10 w-10 mx-auto mb-3 opacity-20" />
              <p className="text-sm">Select a driver from the queue to review their documents</p>
            </div>
          </GlassCard>
        )}
      </motion.div>
    </motion.div>
  );
}
