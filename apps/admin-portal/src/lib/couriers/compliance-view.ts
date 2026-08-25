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
