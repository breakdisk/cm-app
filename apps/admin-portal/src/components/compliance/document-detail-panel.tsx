"use client";
import { useEffect, useState } from "react";
import { fetchProfile, fetchDocumentUrl, isStoredObject } from "@/lib/api/compliance";
import type { DriverDocument } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";
import { entityLabel, initialsFor, typeLabel } from "@/lib/compliance/labels";
import { Check, X, ExternalLink, Eye, Loader2, ShieldOff, ShieldCheck } from "lucide-react";

interface Props {
  profileId:   string;
  /** Increment to force a re-fetch (after approve / reject / suspend / reinstate). */
  refreshKey:  number;
  onApprove:   (docId: string) => void;
  onReject:    (docId: string, reason: string) => void;
  /** Only present for users with compliance:admin permission. */
  onSuspend?:   (profileId: string, reason?: string) => void;
  onReinstate?: (profileId: string) => void;
  /** `document_type_id → name`. Empty while the catalogue is loading. */
  typeNames:   Map<string, string>;
  /** `entity_id → person`. Empty when the roster could not be loaded. */
  entityNames: Map<string, string>;
}

/**
 * What we know about one document's presigned link.
 *
 * `url` arrives on demand — the link is fetched when the reviewer asks to see
 * the document, not when the panel renders, because each fetch writes an audit
 * row and a per-render fetch would turn that log into noise.
 *
 * `unrenderable` means the browser refused to draw it as an image. The stored
 * type may be a PDF — the documents table has no `content_type` column, so
 * there is nothing to check up front — and the honest response is to offer the
 * link rather than to leave a broken image where a licence should be.
 */
interface DocView {
  url?:          string;
  loading?:      boolean;
  error?:        string;
  unrenderable?: boolean;
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
  refreshKey,
  onApprove,
  onReject,
  onSuspend,
  onReinstate,
  typeNames,
  entityNames,
}: Props) {
  const [detail,          setDetail]          = useState<{ profile: any; documents: DriverDocument[] } | null>(null);
  const [rejectDocId,     setRejectDocId]     = useState<string | null>(null);
  const [rejectReason,    setRejectReason]    = useState("");
  const [suspendReason,   setSuspendReason]   = useState("");
  const [suspendOpen,     setSuspendOpen]     = useState(false);
  const [views,           setViews]           = useState<Record<string, DocView>>({});

  useEffect(() => {
    setDetail(null);
    // Links are per-document and expire in fifteen minutes. Dropping them when
    // the panel switches profiles keeps one courier's licence from lingering in
    // memory behind another courier's row.
    setViews({});
    fetchProfile(profileId)
      .then(setDetail)
      .catch(() => setDetail({ profile: null, documents: [] }));
  }, [profileId, refreshKey]);

  /**
   * Fetch the link for one document and show it inline.
   *
   * Inline rather than a new tab: the reviewer needs the licence and the
   * Approve / Reject buttons on screen at the same moment, and a decision taken
   * in another tab is a decision taken from memory.
   */
  async function showDocument(docId: string) {
    setViews((v) => ({ ...v, [docId]: { loading: true } }));
    try {
      const url = await fetchDocumentUrl(docId);
      setViews((v) => ({ ...v, [docId]: { url } }));
    } catch (e) {
      setViews((v) => ({
        ...v,
        [docId]: { error: e instanceof Error ? e.message : "Could not load this document." },
      }));
    }
  }

  if (!detail) {
    return (
      <div className="flex-1 flex items-center justify-center text-white/25 text-sm">
        Loading…
      </div>
    );
  }

  if (!detail.profile) {
    return (
      <div className="flex-1 flex items-center justify-center text-white/25 text-sm">
        Failed to load profile — check backend connectivity.
      </div>
    );
  }

  const { profile, documents } = detail;
  const isSuspended = profile.overall_status === "suspended";

  // Sort: pending/under_review first
  const sorted = [...documents].sort((a, b) => {
    const rank = (s: string) => (s === "submitted" || s === "under_review" ? 0 : 1);
    return rank(a.status) - rank(b.status);
  });

  const badgeClass = STATUS_BADGE[profile.overall_status] ?? STATUS_BADGE.pending_submission;
  // The person, resolved against the roster this portal already holds.
  // compliance itself only knows the uuid.
  const who = entityLabel(entityNames, String(profile.entity_id ?? ""));

  return (
    <div className="flex-1 rounded-xl border border-cyan-glow/20 bg-cyan-surface/5 flex flex-col overflow-hidden">
      {/* Profile header */}
      <div className="px-4 py-3 border-b border-glass-border flex items-center gap-3">
        <div className="w-10 h-10 rounded-full bg-cyan-surface/20 border-2 border-cyan-glow/30 flex items-center justify-center text-sm font-bold text-cyan-neon flex-shrink-0">
          {initialsFor(who)}
        </div>
        <div className="min-w-0">
          <div
            className="text-sm font-semibold text-white truncate max-w-[200px]"
            title={String(profile.entity_id ?? "")}
          >
            {who}
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

        {/* Cross-portal link */}
        {profile.entity_type === "driver" && profile.entity_id && (
          <a
            href={`/partner/drivers?focus=${encodeURIComponent(String(profile.entity_id))}`}
            title="Open driver in Partner Portal"
            className="ml-2 inline-flex items-center gap-1 rounded-md border border-glass-border bg-glass-100 px-2 py-1 text-2xs font-mono text-white/50 hover:border-purple-plasma/40 hover:text-purple-plasma transition-colors"
          >
            Partner ↗
          </a>
        )}
      </div>

      {/* Suspend / Reinstate admin actions (only shown when admin callbacks provided) */}
      {(onSuspend || onReinstate) && (
      <div className="px-4 py-2.5 border-b border-glass-border flex items-center gap-2 flex-wrap">
        {isSuspended ? (
          <button
            onClick={() => onReinstate?.(profileId)}
            className="flex items-center gap-1.5 rounded-lg border border-green-glow/35 bg-green-surface/10 px-3 py-1.5 text-xs font-bold text-green-signal hover:bg-green-surface/20 transition-colors"
          >
            <ShieldCheck className="h-3 w-3" /> Reinstate Driver
          </button>
        ) : (
          <>
            <button
              onClick={() => setSuspendOpen((o) => !o)}
              className="flex items-center gap-1.5 rounded-lg border border-red-glow/30 bg-red-surface/10 px-3 py-1.5 text-xs font-bold text-red-signal hover:bg-red-surface/20 transition-colors"
            >
              <ShieldOff className="h-3 w-3" /> Suspend Driver
            </button>
            {suspendOpen && (
              <div className="flex flex-1 gap-2 min-w-0">
                <input
                  value={suspendReason}
                  onChange={(e) => setSuspendReason(e.target.value)}
                  placeholder="Suspension reason (optional)…"
                  className="flex-1 bg-red-surface/20 border border-red-glow/20 rounded-lg px-3 py-1.5 text-xs font-mono text-white/60 placeholder:text-white/25 outline-none"
                />
                <button
                  onClick={() => {
                    onSuspend?.(profileId, suspendReason || undefined);
                    setSuspendOpen(false);
                    setSuspendReason("");
                  }}
                  className="px-3 py-1.5 rounded-lg text-xs font-bold bg-red-surface/20 border border-red-glow/30 text-red-signal hover:bg-red-surface/30 transition-colors"
                >
                  Confirm
                </button>
                <button
                  onClick={() => { setSuspendOpen(false); setSuspendReason(""); }}
                  className="px-3 py-1.5 rounded-lg text-xs border border-glass-border text-white/40 hover:text-white/60 transition-colors"
                >
                  Cancel
                </button>
              </div>
            )}
          </>
        )}
      </div>
      )}

      {/* Document list */}
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-3">
        {sorted.length === 0 && (
          <div className="flex items-center justify-center h-24 text-xs text-white/25">
            No documents on file
          </div>
        )}
        {sorted.map((doc) => {
          const isPending = doc.status === "submitted" || doc.status === "under_review";
          const view      = views[doc.id] ?? {};
          // Only objects this service stored can be presigned. A caller-hosted
          // `http(s)://` URL is already a working link, and the seeded mocks use
          // `#`, which is neither.
          const stored    = isStoredObject(doc.file_url);
          const plainLink = !stored && doc.file_url && doc.file_url !== "#";

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
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-bold uppercase tracking-wider text-white/75 truncate">
                    {typeLabel(typeNames, doc.document_type_id)}
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

                {/* Seeing the document.
                    `file_url` is an `s3://` URI, which a browser cannot open —
                    this used to be an anchor pointing straight at it, so every
                    decision on this panel was taken without sight of the
                    document. The link is minted on demand and audited. */}
                {stored && !view.url && (
                  <button
                    onClick={() => showDocument(doc.id)}
                    disabled={view.loading}
                    className="px-2.5 py-1.5 rounded-lg text-xs bg-glass-100 border border-glass-border text-white/50 flex items-center gap-1 hover:text-white/80 disabled:opacity-40 transition-colors flex-shrink-0"
                  >
                    {view.loading
                      ? <Loader2 className="h-3 w-3 animate-spin" />
                      : <Eye className="h-3 w-3" />}
                    View
                  </button>
                )}
                {plainLink && (
                  <a
                    href={doc.file_url}
                    target="_blank"
                    rel="noreferrer"
                    className="px-2.5 py-1.5 rounded-lg text-xs bg-glass-100 border border-glass-border text-white/50 flex items-center gap-1 hover:text-white/80 transition-colors flex-shrink-0"
                  >
                    <ExternalLink className="h-3 w-3" /> View
                  </a>
                )}
              </div>

              {/* The document itself, next to the buttons that decide on it. */}
              {view.url && !view.unrenderable && (
                /* eslint-disable-next-line @next/next/no-img-element */
                <img
                  src={view.url}
                  alt="Submitted document"
                  onError={() =>
                    setViews((v) => ({ ...v, [doc.id]: { ...v[doc.id], unrenderable: true } }))
                  }
                  className="mt-3 w-full max-h-80 object-contain rounded-lg border border-glass-border bg-black/30"
                />
              )}

              {/* Not an image the browser will draw — a PDF, most likely. There
                  is no content_type column to check beforehand, so this is
                  found by trying. */}
              {view.url && view.unrenderable && (
                <a
                  href={view.url}
                  target="_blank"
                  rel="noreferrer"
                  className="mt-3 inline-flex items-center gap-1.5 rounded-lg border border-glass-border bg-glass-100 px-3 py-1.5 text-xs text-white/60 hover:text-white transition-colors"
                >
                  <ExternalLink className="h-3 w-3" /> Open document
                </a>
              )}

              {view.error && (
                <div className="mt-3 flex items-center gap-2 text-xs font-mono text-red-signal/80 bg-red-surface/10 border border-red-glow/20 rounded-lg px-2.5 py-1.5">
                  <span className="flex-1">{view.error}</span>
                  <button
                    onClick={() => showDocument(doc.id)}
                    className="underline hover:text-white transition-colors"
                  >
                    Retry
                  </button>
                </div>
              )}

              {/* Approve / Reject actions — panel calls parent callbacks; parent owns API call */}
              {isPending && !isSuspended && (
                <div className="mt-3">
                  <div className="flex gap-2">
                    <button
                      onClick={() => onApprove(doc.id)}
                      className="flex-1 py-1.5 rounded-lg text-xs font-bold bg-green-surface/20 border border-green-glow/35 text-green-signal flex items-center justify-center gap-1 hover:bg-green-surface/30 transition-colors"
                    >
                      <Check className="h-3 w-3" /> Approve
                    </button>
                    <button
                      onClick={() => setRejectDocId(rejectDocId === doc.id ? null : doc.id)}
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
                        onClick={() => {
                          if (!rejectReason.trim()) return;
                          onReject(doc.id, rejectReason);
                          setRejectDocId(null);
                          setRejectReason("");
                        }}
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
