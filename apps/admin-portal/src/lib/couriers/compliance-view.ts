/**
 * What the couriers screen says about a courier, as pure functions.
 *
 * Split out of the page for the reason `lib/compliance/labels.ts` was: these are
 * decisions with real consequences — "why is this person not getting jobs?" —
 * and a decision embedded in JSX cannot be tested against a payload.
 *
 * The module exists because of one production failure. The deployed field-ops
 * predated the compliance gate and sent none of the three compliance fields, so
 * `compliance_status` was `undefined`; `undefined === null` is false, the
 * not-onboarded branch did not catch it, and the next line called `.replace()`
 * on it. Every row of the only management surface couriers have threw.
 *
 * So: an absent field is its own state everywhere in here, and it never
 * collapses into `null`.
 */
import type { AdminCourier } from "@/lib/api/couriers";

export type ComplianceKind = "unsupported" | "not-onboarded" | "known";

export interface ComplianceView {
  kind:   ComplianceKind;
  label:  string;
  tone:   string;
  title?: string;
}

/**
 * Has this payload got a compliance opinion at all?
 *
 * Both fields are checked rather than one: a build that sends either sends both,
 * but keying the whole module off a single field would make it turn on whichever
 * one happened to be picked, and the two carry different meanings when null.
 */
export function hasComplianceFields(c: AdminCourier): boolean {
  return c.compliance_status !== undefined || c.compliance_assignable !== undefined;
}

/**
 * The Compliance column.
 *
 * Three kinds, and conflating any two of them makes the column lie:
 *
 * - `unsupported` — this field-ops build has no compliance concept. Nothing is
 *   knowable, so the cell says nothing and explains why on hover. Rendering
 *   "not onboarded" here would blame the courier for a stale deploy.
 * - `not-onboarded` — compliance has genuinely never seen them. Not a
 *   clearance; these are exactly the people who still need onboarding.
 * - `known` — compliance has spoken. Show what it said, verbatim, because
 *   `is_assignable` is stored from the event and never re-derived from the
 *   status string: `expired` is deliberately still assignable during its grace
 *   period, and a screen that guessed otherwise would report couriers as
 *   stopped who are not.
 */
export function complianceView(c: AdminCourier): ComplianceView {
  if (!hasComplianceFields(c)) {
    return {
      kind:  "unsupported",
      label: "—",
      tone:  "bg-white/5 text-white/30",
      title: "This deployment's field-ops predates the compliance gate and reports nothing about compliance. Not a statement about this courier.",
    };
  }

  if (c.compliance_status === null || c.compliance_status === undefined) {
    return {
      kind:  "not-onboarded",
      label: "not onboarded",
      tone:  "bg-white/5 text-white/40",
      title: "No compliance profile has been opened for this courier yet. They are not blocked — unknown couriers are still offered work.",
    };
  }

  const tone = c.compliance_assignable
    ? (c.compliance_status === "compliant"
        ? "bg-emerald-400/10 text-emerald-300"
        : "bg-amber-400/10 text-amber-300")
    : "bg-rose-400/10 text-rose-300";

  return { kind: "known", label: c.compliance_status.replace(/_/g, " "), tone };
}

export interface DispatchView {
  label:  string;
  tone:   string;
  title?: string;
}

/** Minutes since the last GPS fix, or null when there has never been one. */
export function fixAgeMinutes(iso: string | null | undefined, nowMs: number): number | null {
  if (!iso) return null;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return null;
  return Math.floor((nowMs - t) / 60_000);
}

/**
 * The proximity search only considers a position from the last ten minutes, so a
 * courier who is active and on duty is still invisible to dispatch if their
 * phone stopped reporting. That window lives in the query rather than on the
 * row, which is why this one reason is always derived here.
 */
const STALE_FIX_MINUTES = 10;

/**
 * Why this courier is or is not being offered work.
 *
 * `block_reason` is the authority whenever the server sends it — it weighs
 * compliance, and whether compliance *blocks* depends on a deployment flag this
 * client is never told, so a client-side copy of that rule would confidently
 * disagree with the dispatcher.
 *
 * **When the key is absent the server is not the authority, because it is not
 * answering.** The old code fell straight through to the GPS branch and reported
 * an `offline` courier as "on duty · last fix Nm ago (stale)" — a wrong answer,
 * stated confidently, in the column that exists to prevent exactly that.
 *
 * So absence falls back to deriving the two reasons visible in the legacy
 * payload, in `Courier::dispatch_block`'s own order. Compliance is deliberately
 * not derived and not guessed: it is the one reason a client can never see, and
 * a build that omits the field has no compliance term to report anyway.
 */
export function dispatchView(c: AdminCourier, nowMs: number): DispatchView {
  const reason = c.block_reason ?? derivedBlockReason(c);

  if (reason === "suspended") {
    return { label: "suspended by ops", tone: "text-rose-300" };
  }
  if (reason === "off_duty") {
    return { label: "not on duty", tone: "text-white/40" };
  }
  if (reason === "compliance") {
    return {
      label: `blocked \u00b7 ${(c.compliance_status ?? "unknown").replace(/_/g, " ")}`,
      tone:  "text-rose-300",
    };
  }

  const age = fixAgeMinutes(c.last_seen_at, nowMs);
  if (age === null) {
    return { label: "on duty \u00b7 never sent a position", tone: "text-amber-300" };
  }
  if (age > STALE_FIX_MINUTES) {
    return { label: `on duty \u00b7 last fix ${age}m ago (stale)`, tone: "text-amber-300" };
  }

  // Observe-only: compliance has refused them and enforcement is off, so they
  // are still getting work. Only sayable when the server told us — a payload
  // without the field has no verdict to disagree with.
  if (hasComplianceFields(c) && c.compliance_assignable === false) {
    return {
      label: "receiving offers \u00b7 compliance would block",
      tone:  "text-amber-300",
      title: "Compliance has refused this courier, but enforcement is off in this deployment so they are still being offered work. Turning enforcement on will stop them.",
    };
  }

  return { label: "receiving offers", tone: "text-emerald-300" };
}

/**
 * The two reasons a legacy payload still makes visible, in the server's order.
 *
 * `Courier::dispatch_block` checks `!is_active` before `status != Available`
 * before compliance. Reproducing the first two exactly is what makes this
 * fallback safe: it is not a second opinion, it is the same rule over the same
 * two fields, and it stops where the payload stops knowing.
 */
function derivedBlockReason(c: AdminCourier): "suspended" | "off_duty" | null {
  if (!c.is_active) return "suspended";
  if (c.status !== "available") return "off_duty";
  return null;
}

export interface CourierCounts {
  total:        number;
  dispatchable: number;
  suspended:    number;
  /** `null` when no courier in the list carries a compliance verdict. */
  complianceBlocked: number | null;
  /** `null` when no courier in the list carries a compliance verdict. */
  notOnboarded:      number | null;
}

/**
 * The KPI tiles.
 *
 * `complianceBlocked` and `notOnboarded` are `null` — not `0` — when nothing in
 * the list carries a compliance verdict. `0` is a claim, and the claim it makes
 * ("nobody is blocked") is one this payload cannot support. The old code counted
 * `!c.compliance_assignable`, which on an absent field is `!undefined`, so it
 * reported every courier as blocked while `=== null` reported none as
 * un-onboarded: both tiles read exactly backwards, and the "Compliance blocked"
 * one is the number that is supposed to say what enforcing the flag would cost.
 *
 * Couriers the server said nothing about are excluded from the compliance counts
 * rather than assumed either way, so a roster served by a mix of old and new
 * instances mid-deploy reports only what is actually known.
 */
export function courierCounts(couriers: AdminCourier[]): CourierCounts {
  const known = couriers.filter(hasComplianceFields);

  return {
    total:        couriers.length,
    dispatchable: couriers.filter((c) => c.dispatchable).length,
    suspended:    couriers.filter((c) => !c.is_active).length,
    complianceBlocked: known.length === 0
      ? null
      : known.filter((c) => c.compliance_assignable === false).length,
    notOnboarded: known.length === 0
      ? null
      : known.filter((c) => c.compliance_status === null).length,
  };
}
