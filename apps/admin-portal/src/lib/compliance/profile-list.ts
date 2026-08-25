/**
 * Ordering and labelling for the console's roster of compliance profiles.
 *
 * The review queue answers "what is waiting for me to click Approve". It lists
 * *documents* in `submitted` / `under_review`, so a profile with no documents at
 * all is not in it — and `pending_submission` is where every profile starts and
 * where every profile that nobody has acted on stays. Those people were
 * unreachable from the console entirely: they had a profile, a status and a
 * KPI-strip tile counting them, and no row anywhere to open.
 *
 * This module is what the second tab sorts by. Pure functions over data the
 * page fetched, like `labels.ts` beside it.
 */
import type { ComplianceProfile } from "@/lib/api/compliance";

/**
 * How far up the list a status belongs. Lower sorts first.
 *
 * Ordered by what a reviewer can *do* about it, not by severity:
 *
 * - `rejected` and `suspended` first — a person has been stopped and is likely
 *   waiting on a human to explain or undo it.
 * - `expired` next: assignable during its grace period, so it is a deadline
 *   rather than an outage, but the deadline is running.
 * - `pending_submission` — the people this tab exists for. Nothing is waiting in
 *   the queue for them precisely because they have submitted nothing.
 * - `under_review` below that: already visible in the queue, so listing it high
 *   here would just duplicate the tab next door.
 * - `expiring_soon`, then `compliant` — nothing to do.
 *
 * An unknown status sorts just above `compliant`: a status a newer migration
 * adds is more likely to need attention than not, but guessing it is urgent
 * would push real work down the page.
 */
const RANK: Record<string, number> = {
  rejected:           0,
  suspended:          1,
  expired:            2,
  pending_submission: 3,
  under_review:       4,
  expiring_soon:      5,
  compliant:          7,
};

const UNKNOWN_RANK = 6;

export function profileRank(status: string): number {
  return RANK[status] ?? UNKNOWN_RANK;
}

/** Is there anything for a human to do about this profile? */
export function needsAttention(status: string): boolean {
  return profileRank(status) < RANK.expiring_soon;
}

/**
 * The roster, most actionable first, ties broken by name so the order is stable
 * between refreshes.
 *
 * Stability matters more than it sounds: this list refreshes on a 30-second
 * timer, and rows that reshuffle under the cursor get mis-clicked. Sorting by
 * the resolved name rather than the id means two profiles that rank equally
 * keep their positions even as statuses change around them.
 *
 * Does not mutate its input — the page holds the same array for its KPI strip.
 */
export function outstandingFirst(
  profiles:    ComplianceProfile[],
  entityNames: Map<string, string>,
): ComplianceProfile[] {
  return [...profiles].sort((a, b) => {
    const byRank = profileRank(a.overall_status) - profileRank(b.overall_status);
    if (byRank !== 0) return byRank;
    const an = entityNames.get(a.entity_id) ?? a.entity_id;
    const bn = entityNames.get(b.entity_id) ?? b.entity_id;
    return an.localeCompare(bn);
  });
}

/**
 * How many profiles nobody has dealt with.
 *
 * Deliberately *not* the queue's length. The queue counts documents awaiting a
 * decision; this counts people awaiting anything at all, and the gap between
 * the two numbers is the thing that was invisible — a fleet that has submitted
 * nothing shows a queue of zero and reads as "all clear".
 */
export function attentionCount(profiles: ComplianceProfile[]): number {
  return profiles.filter((p) => needsAttention(p.overall_status)).length;
}
