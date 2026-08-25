import type { ComplianceProfile } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";
import { entityLabel, initialsFor } from "@/lib/compliance/labels";
import { outstandingFirst, needsAttention } from "@/lib/compliance/profile-list";

interface Props {
  profiles:    ComplianceProfile[];
  selectedId:  string | null;
  onSelect:    (profileId: string) => void;
  /** `entity_id → person`. Empty when the roster could not be loaded. */
  entityNames: Map<string, string>;
}

const STATUS_TONE: Record<string, string> = {
  compliant:          "bg-green-surface/20 border-green-glow/20 text-green-signal",
  under_review:       "bg-amber-surface/20 border-amber-glow/25 text-amber-signal",
  expiring_soon:      "bg-amber-surface/20 border-amber-glow/25 text-amber-signal",
  pending_submission: "bg-glass-100 border-glass-border text-white/45",
  expired:            "bg-red-surface/20 border-red-glow/25 text-red-signal",
  suspended:          "bg-red-surface/20 border-red-glow/25 text-red-signal",
  rejected:           "bg-red-surface/20 border-red-glow/25 text-red-signal",
};

/**
 * Everyone compliance holds a profile for — not only the ones with a document
 * waiting to be judged.
 *
 * The review queue beside this lists *documents* in `submitted` /
 * `under_review`. Someone who has submitted nothing has no document, so they
 * had no row in the console at all: a profile, a status, a tile in the KPI
 * strip counting them, and nowhere to click. `pending_submission` is where
 * every profile starts, so for a fleet that has not begun onboarding the
 * console showed an empty queue and read as "all clear".
 *
 * That is the list this renders, most actionable first.
 */
export function ProfileRoster({ profiles, selectedId, onSelect, entityNames }: Props) {
  const ordered = outstandingFirst(profiles, entityNames);

  return (
    <div className="overflow-y-auto flex-1">
      {ordered.length === 0 && (
        <div className="flex items-center justify-center h-24 px-4 text-center text-xs text-white/25">
          No compliance profiles yet. One opens the first time a courier is
          announced or opens their documents screen.
        </div>
      )}
      {ordered.map((p) => {
        const who = entityLabel(entityNames, p.entity_id);
        return (
          <button
            key={p.id}
            onClick={() => onSelect(p.id)}
            className={cn(
              "w-full text-left px-4 py-3 border-b border-glass-border flex gap-3 items-center hover:bg-glass-200 transition-colors",
              selectedId === p.id && "bg-cyan-surface/20 border-l-2 border-l-cyan-neon",
            )}
          >
            <div
              className={cn(
                "w-8 h-8 rounded-full border flex items-center justify-center text-xs font-bold flex-shrink-0",
                needsAttention(p.overall_status)
                  ? "bg-amber-surface/20 border-amber-glow/25 text-amber-signal"
                  : "bg-glass-100 border-glass-border text-white/40",
              )}
            >
              {initialsFor(who)}
            </div>

            <div className="flex-1 min-w-0">
              <div className="text-sm font-semibold text-white/85 truncate" title={p.entity_id}>
                {who}
              </div>
              <span
                className={cn(
                  "inline-block mt-1 text-2xs font-semibold px-1.5 py-0.5 rounded border",
                  STATUS_TONE[p.overall_status] ?? "bg-glass-100 border-glass-border text-white/45",
                )}
              >
                {p.overall_status.replace(/_/g, " ")}
              </span>
            </div>

            <span className="text-2xs font-mono text-white/20 flex-shrink-0">{p.jurisdiction}</span>
          </button>
        );
      })}
    </div>
  );
}
