import type { DriverDocument } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";

interface Props {
  items:      DriverDocument[];
  selectedId: string | null;
  onSelect:   (profileId: string) => void;
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60_000);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function initials(id: string): string {
  return id.slice(0, 2).toUpperCase();
}

export function ReviewQueue({ items, selectedId, onSelect }: Props) {
  return (
    <div className="w-72 flex-shrink-0 rounded-xl border border-glass-border bg-glass-100 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-4 py-2.5 border-b border-glass-border flex items-center justify-between">
        <span className="text-xs font-bold uppercase tracking-widest text-white/60">
          Review Queue
        </span>
        <span className="text-xs font-mono bg-cyan-surface/20 border border-cyan-glow/25 text-cyan-neon rounded-full px-2 py-0.5">
          {items.length} pending
        </span>
      </div>

      {/* Items */}
      <div className="overflow-y-auto flex-1">
        {items.length === 0 && (
          <div className="flex items-center justify-center h-24 text-xs text-white/25">
            No documents pending review
          </div>
        )}
        {items.map((doc) => (
          <button
            key={doc.id}
            onClick={() => onSelect(doc.compliance_profile_id)}
            className={cn(
              "w-full text-left px-4 py-3 border-b border-glass-border flex gap-3 items-start hover:bg-glass-200 transition-colors",
              selectedId === doc.compliance_profile_id &&
                "bg-cyan-surface/20 border-l-2 border-l-cyan-neon",
            )}
          >
            {/* Avatar */}
            <div className="w-8 h-8 rounded-full bg-cyan-surface/20 border border-cyan-glow/25 flex items-center justify-center text-xs font-bold text-cyan-neon flex-shrink-0">
              {initials(doc.compliance_profile_id)}
            </div>

            {/* Info */}
            <div className="flex-1 min-w-0">
              <div className="text-sm font-semibold text-white/85 truncate">
                Profile {doc.compliance_profile_id.slice(0, 8)}
              </div>
              <div className="text-xs font-mono text-white/35 mt-0.5">
                {doc.document_type_id.slice(0, 8)}
              </div>
              <span className="inline-block mt-1 text-2xs font-semibold px-1.5 py-0.5 rounded bg-cyan-surface/20 border border-cyan-glow/25 text-cyan-neon">
                {doc.status === "submitted" ? "New submission" : "Renewal"}
              </span>
            </div>

            {/* Time */}
            <span className="text-2xs font-mono text-white/20 flex-shrink-0">
              {timeAgo(doc.submitted_at)}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
