"use client";
import { useEffect, useState } from "react";
import {
  fetchProfile,
  approveDocument,
  rejectDocument,
  getAdminDocumentUrl,
  suspendProfile,
  reinstateProfile,
} from "@/lib/api/compliance";
import type { DriverDocument, ComplianceProfile } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";
import { Check, X, ExternalLink, ShieldX, ShieldCheck } from "lucide-react";

interface Props {
  profileId:   string;
  onApprove:   (docId: string) => void;
  onReject:    (docId: string, reason: string) => void;
  onSuspend:   (profileId: string) => void;
  onReinstate: (profileId: string) => void;
}

const STATUS_BADGE: Record<string, string> = {
  compliant:          "bg-green-surface/20 border-green-glow/20 text-green-signal",
  under_review:       "bg-amber-surface/20 border-amber-glow/25 text-amber-signal",
  pending_submission: "bg-glass-100 border-glass-border text-white/40",
  expiring_soon:      "bg-amber-surface/20 border-amber-glow/25 text-amber-signal",
  expired:            "bg-red-surface/20 border-red-glow/25 text-red-signal",
  suspended:          "bg-red-surface/20 border-red-glow/25 text-red-signal",
};

export function DocumentDetailPanel({
  profileId,
  onApprove,
  onReject,
  onSuspend,
  onReinstate,
}: Props) {
  const [detail,        setDetail]        = useState<{ profile: ComplianceProfile; documents: DriverDocument[]; audit_log: unknown[] } | null>(null);
  const [rejectDocId,   setRejectDocId]   = useState<string | null>(null);
  const [rejectReason,  setRejectReason]  = useState("");
  const [suspendOpen,   setSuspendOpen]   = useState(false);
  const [suspendReason, setSuspendReason] = useState("");

  useEffect(() => {
    setDetail(null);
    setSuspendOpen(false);
    setSuspendReason("");
    fetchProfile(profileId).then(setDetail);
  }, [profileId]);

  if (!detail) {
    return (
      <div className="flex-1 flex items-center justify-center text-white/25 text-sm">
        Loading…
      </div>
    );
  }

  const { profile, documents } = detail;

  // Sort: pending/under_review first
  const sorted = [...documents].sort((a, b) => {
    const rank = (s: string) =>
      s === "submitted" || s === "under_review" ? 0 : 1;
    return rank(a.status) - rank(b.status);
  });

  async function handleApprove(docId: string) {
    try {
      await approveDocument(docId);
      onApprove(docId);
      fetchProfile(profileId).then(setDetail);
    } catch (err) {
      console.error("Failed to approve document:", err);
    }
  }

  async function handleReject(docId: string) {
    if (!rejectReason.trim()) return;
    try {
      await rejectDocument(docId, rejectReason);
      onReject(docId, rejectReason);
      setRejectDocId(null);
      setRejectReason("");
      fetchProfile(profileId).then(setDetail);
    } catch (err) {
      console.error("Failed to reject document:", err);
    }
  }

  async function handleSuspend() {
    try {
      await suspendProfile(profile.id, suspendReason.trim() || undefined);
      setSuspendOpen(false);
      setSuspendReason("");
      const updated = await fetchProfile(profileId);
      setDetail(updated);
      onSuspend(profile.id);
    } catch (err) {
      console.error("Failed to suspend profile:", err);
    }
  }

  async function handleReinstate() {
    try {
      await reinstateProfile(profile.id);
      const updated = await fetchProfile(profileId);
      setDetail(updated);
      onReinstate(profile.id);
    } catch (err) {
      console.error("Failed to reinstate profile:", err);
    }
  }

  const badgeClass =
    STATUS_BADGE[profile.overall_status] ?? STATUS_BADGE.pending_submission;

  const isSuspended = profile.overall_status === "suspended";

  return (
    <div className="flex-1 rounded-xl border border-cyan-glow/20 bg-cyan-surface/5 flex flex-col overflow-hidden">
      {/* Profile header */}
      <div className="px-4 py-3 border-b border-glass-border">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-cyan-surface/20 border-2 border-cyan-glow/30 flex items-center justify-center text-sm font-bold text-cyan-neon flex-shrink-0">
            {String(profile.entity_id ?? "").slice(0, 2).toUpperCase()}
          </div>
          <div>
            <div className="text-sm font-semibold text-white truncate max-w-[200px]">
              {profile.entity_id}
            </div>
            <div className="text-xs font-mono text-white/35">{profile.jurisdiction}</div>
          </div>
          <span
            className={cn(
              "ml-auto text-xs px-3 py-1 rounded-full border font-mono font-semibold",
              badgeClass,
            )}
          >
            {String(profile.overall_status ?? "").replace(/_/g, " ")}
          </span>
          {/* Cross-portal — drivers are managed (commission, zone, SLA) in partner-portal.
              Plain <a> so the /partner basePath persists after the jump. */}
          {profile.entity_type === "driver" && profile.entity_id && (
            <a
              href={`/partner/drivers?focus=${encodeURIComponent(String(profile.entity_id))}`}
              title="Open driver in Partner Portal"
              className="ml-2 inline-flex items-center gap-1 rounded-md border border-glass-border bg-glass-100 px-2 py-1 text-2xs font-mono text-white/50 hover:border-purple-plasma/40 hover:text-purple-plasma transition-colors"
            >
              Partner ↗
            </a>
          )}
          {/* Suspend / Reinstate */}
          {isSuspended ? (
            <button
              onClick={handleReinstate}
              className="ml-1 px-2 py-1 rounded-lg text-2xs border border-green-glow/30 bg-green-surface/20 text-green-signal flex items-center gap-1 hover:bg-green-surface/30 transition-colors"
            >
              <ShieldCheck className="h-3 w-3" /> Reinstate
            </button>
          ) : (
            <button
              onClick={() => setSuspendOpen((o) => !o)}
              className="ml-1 px-2 py-1 rounded-lg text-2xs border border-red-glow/30 bg-red-surface/20 text-red-signal flex items-center gap-1 hover:bg-red-surface/30 transition-colors"
            >
              <ShieldX className="h-3 w-3" /> Suspend
            </button>
          )}
        </div>

        {/* Inline suspend-reason form */}
        {suspendOpen && !isSuspended && (
          <div className="mt-2 flex gap-2">
            <input
              value={suspendReason}
              onChange={(e) => setSuspendReason(e.target.value)}
              placeholder="Suspension reason (optional)…"
              className="flex-1 bg-red-surface/30 border border-red-glow/25 rounded-lg px-3 py-1.5 text-xs font-mono text-white/60 placeholder:text-white/25 outline-none"
            />
            <button
              onClick={handleSuspend}
              className="px-3 py-1.5 rounded-lg text-xs font-bold bg-red-surface/20 border border-red-glow/30 text-red-signal transition-colors"
            >
              Confirm
            </button>
          </div>
        )}
      </div>

      {/* Document list */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-3">
        {sorted.map((doc) => {
          const isPending =
            doc.status === "submitted" || doc.status === "under_review";

          return (
            <div
              key={doc.id}
              className={cn(
                "rounded-xl p-3.5 border",
                isPending
                  ? "border-amber-glow/30 bg-amber-surface/10"
                  : "border-green-glow/20 bg-green-surface/5",
              )}
            >
              {/* Doc header row */}
              <div className="flex items-start gap-3">
                <div className="w-16 h-12 rounded-lg bg-glass-100 border border-glass-border flex items-center justify-center text-xl flex-shrink-0">
                  🪪
                </div>
                <div className="flex-1">
                  <div className="text-xs font-bold uppercase tracking-wider text-white/75">
                    {doc.document_type_id.slice(0, 12)}
                  </div>
                  <div className="text-xs font-mono text-white/40 mt-1">
                    {doc.document_number}
                  </div>
                  {doc.expiry_date && (
                    <div
                      className={cn(
                        "text-xs mt-1",
                        isPending ? "text-amber-signal/80" : "text-green-signal/70",
                      )}
                    >
                      Exp: {doc.expiry_date}
                    </div>
                  )}
                </div>
                {doc.file_url && (
                  <button
                    onClick={async () => {
                      try {
                        const url = await getAdminDocumentUrl(doc.id);
                        window.open(url, "_blank", "noreferrer");
                      } catch (err) {
                        console.error("Failed to get document URL:", err);
                      }
                    }}
                    className="px-2.5 py-1.5 rounded-lg text-xs bg-glass-100 border border-glass-border text-white/50 flex items-center gap-1 hover:text-white/80 transition-colors"
                  >
                    <ExternalLink className="h-3 w-3" /> View
                  </button>
                )}
              </div>

              {/* Approve / Reject actions */}
              {isPending && (
                <div className="mt-3">
                  <div className="flex gap-2">
                    <button
                      onClick={() => handleApprove(doc.id)}
                      className="flex-1 py-1.5 rounded-lg text-xs font-bold bg-green-surface/20 border border-green-glow/35 text-green-signal flex items-center justify-center gap-1 hover:bg-green-surface/30 transition-colors"
                    >
                      <Check className="h-3 w-3" /> Approve
                    </button>
                    <button
                      onClick={() =>
                        setRejectDocId(rejectDocId === doc.id ? null : doc.id)
                      }
                      className="flex-1 py-1.5 rounded-lg text-xs font-bold bg-red-surface/20 border border-red-glow/30 text-red-signal flex items-center justify-center gap-1 hover:bg-red-surface/30 transition-colors"
                    >
                      <X className="h-3 w-3" /> Reject
                    </button>
                  </div>

                  {rejectDocId === doc.id && (
                    <div className="mt-2 flex gap-2">
                      <input
                        value={rejectReason}
                        onChange={(e) => setRejectReason(e.target.value)}
                        placeholder="Rejection reason (required)…"
                        className="flex-1 bg-red-surface/30 border border-red-glow/25 rounded-lg px-3 py-1.5 text-xs font-mono text-white/60 placeholder:text-white/25 outline-none"
                      />
                      <button
                        onClick={() => handleReject(doc.id)}
                        disabled={!rejectReason.trim()}
                        className="px-3 py-1.5 rounded-lg text-xs font-bold bg-red-surface/20 border border-red-glow/30 text-red-signal disabled:opacity-40 transition-colors"
                      >
                        Submit
                      </button>
                    </div>
                  )}
                </div>
              )}

              {/* Approved confirmation */}
              {!isPending && doc.reviewed_at && (
                <div className="mt-2 inline-flex items-center gap-1.5 text-xs text-green-signal/70 bg-green-surface/10 border border-green-glow/20 rounded-full px-2.5 py-0.5">
                  <Check className="h-3 w-3" /> Approved ·{" "}
                  {new Date(doc.reviewed_at).toLocaleDateString()}
                </div>
              )}

              {/* Rejection reason */}
              {doc.status === "rejected" && doc.rejection_reason && (
                <div className="mt-2 text-xs font-mono text-red-signal/70 bg-red-surface/10 border border-red-glow/20 rounded-lg px-2.5 py-1.5">
                  Rejected: {doc.rejection_reason}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
