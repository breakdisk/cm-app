# Couriers Page Payload Skew — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Admin Portal → OmniDeliv Couriers render correctly against both the deployed `field-ops` (which sends no compliance fields) and the post-#138 one (which does), then deploy `field-ops` so the real data arrives.

**Architecture:** Move the three display decisions — compliance pill, dispatchable cell, KPI counts — out of the page component and into a pure, tested module `src/lib/couriers/compliance-view.ts`, mirroring the existing `src/lib/compliance/labels.ts` convention. An absent field becomes its own state, distinct from a `null` one. Because `strictNullChecks` is off in this app, a jest suite is the only real gate, so the plan also stands up admin-portal's first working jest config.

**Tech Stack:** Next.js 14 (App Router), TypeScript, jest 29 via `next/jest` (SWC transform, no new packages), Tailwind, Docker/Dokploy on the VPS.

**Spec:** [`docs/superpowers/specs/2026-08-25-couriers-page-payload-skew-design.md`](../specs/2026-08-25-couriers-page-payload-skew-design.md)

---

## File Structure

| File | Responsibility |
|---|---|
| `apps/admin-portal/jest.config.js` | **Create.** `next/jest` wrapper; `node` env; maps `@/*`. First working jest config in this app. |
| `apps/admin-portal/src/lib/couriers/fixtures.ts` | **Create.** The three payload shapes, including the 13-key object captured live on 2026-08-25. Shared by tests. |
| `apps/admin-portal/src/lib/couriers/compliance-view.ts` | **Create.** Pure decisions: `complianceView`, `dispatchView`, `courierCounts`. No React, no fetch. |
| `apps/admin-portal/src/lib/couriers/compliance-view.test.ts` | **Create.** The gate. |
| `apps/admin-portal/src/lib/api/couriers.ts` | **Modify.** Three fields become optional; doc comments say why. |
| `apps/admin-portal/src/app/(dashboard)/couriers/page.tsx` | **Modify.** `CompliancePill`, `DispatchCell`, `counts` delegate to the module; tiles render `—` when a count is `null`. |

---

## Task 1: Stand up a working jest config

`apps/admin-portal` has `jest` in devDependencies, no config, and zero test
files. `npm test -- --passWithNoTests` has always exited 0 without running
anything. Prove the harness runs before trusting anything it says.

**Files:**
- Create: `apps/admin-portal/jest.config.js`
- Create (temporary): `apps/admin-portal/src/lib/couriers/harness.test.ts`

- [ ] **Step 1: Write a test that must fail**

Create `apps/admin-portal/src/lib/couriers/harness.test.ts`:

```ts
describe("jest harness", () => {
  it("runs TypeScript and can fail", () => {
    const n: number = 1;
    expect(n).toBe(2);
  });
});
```

- [ ] **Step 2: Run it and confirm the harness is dead**

```bash
cd apps/admin-portal && npm test -- --ci
```

Expected before the config exists: jest either reports `No tests found` /
`0 total` and exits 0, or fails to parse the TypeScript. Either way it does
**not** report `1 failed`. That is the dead harness.

- [ ] **Step 3: Create the config**

Create `apps/admin-portal/jest.config.js`:

```js
/**
 * The admin portal's first working jest config.
 *
 * `jest` has been in devDependencies and `npm test -- --passWithNoTests` has
 * been in `CI — Frontend` since this app was created, with no config and no
 * test files behind either. The job exited 0 every run without executing
 * anything.
 *
 * `next/jest` is what makes it work: it wires Next's own SWC transform, so
 * TypeScript and the `@/*` alias are handled without adding babel, ts-jest or
 * any new package.
 *
 * `testEnvironment: "node"` on purpose. The suite covers pure functions with no
 * DOM, and jsdom is a separate dependency this app does not have.
 */
const nextJest = require("next/jest");

const createJestConfig = nextJest({ dir: "./" });

module.exports = createJestConfig({
  testEnvironment: "node",
  moduleNameMapper: { "^@/(.*)$": "<rootDir>/src/$1" },
  testMatch: ["<rootDir>/src/**/*.test.ts", "<rootDir>/src/**/*.test.tsx"],
});
```

- [ ] **Step 4: Run again and confirm the harness is alive**

```bash
cd apps/admin-portal && npm test -- --ci
```

Expected: `Tests: 1 failed, 1 total`. A failing test that actually fails is the
proof. If it still says `0 total`, the config is not being picked up — stop and
fix that before continuing.

- [ ] **Step 5: Flip the assertion and confirm it passes**

Edit the same file so it reads `expect(n).toBe(1);`. Run again.
Expected: `Tests: 1 passed, 1 total`.

- [ ] **Step 6: Delete the harness probe and commit the config**

```bash
cd apps/admin-portal && rm src/lib/couriers/harness.test.ts
cd ../.. && git add apps/admin-portal/jest.config.js
git commit -m "test(admin-portal): a jest config, so npm test stops being a green no-op"
```

---

## Task 2: Capture the payloads as fixtures

**Files:**
- Create: `apps/admin-portal/src/lib/couriers/fixtures.ts`

- [ ] **Step 1: Write the fixtures**

Create `apps/admin-portal/src/lib/couriers/fixtures.ts`:

```ts
/**
 * Payload shapes this screen has to survive.
 *
 * `LEGACY_COURIER_*` is not invented. It is the exact object
 * `GET /v1/field-ops/admin/couriers` returned from the production API on
 * 2026-08-25, when the running field-ops image predated the compliance gate:
 * thirteen keys, and `block_reason`, `compliance_status` and
 * `compliance_assignable` absent rather than null. Keep it byte-faithful — its
 * value is that nobody wrote it.
 */
import type { AdminCourier } from "@/lib/api/couriers";

/** Live, 2026-08-25. Off duty, and the server said nothing about why. */
export const LEGACY_OFF_DUTY = {
  id:           "10ae4c3d-3dd3-4480-95d3-051ba3b20d36",
  user_id:      "10ae4c3d-3dd3-4480-95d3-051ba3b20d36",
  first_name:   "Courier",
  last_name:    "",
  phone:        "63581208617",
  status:       "offline",
  is_active:    true,
  vehicle_type: null,
  zone:         null,
  last_lat:     24.5000267,
  last_lng:     54.372825,
  last_seen_at: "2026-08-18T11:10:58.339168Z",
  dispatchable: false,
} as AdminCourier;

/** Live, 2026-08-25. On duty, recent fix at the time it was captured. */
export const LEGACY_AVAILABLE = {
  id:           "761d071d-81e8-414b-88b8-c2c02caad198",
  user_id:      "761d071d-81e8-414b-88b8-c2c02caad198",
  first_name:   "Courier",
  last_name:    "",
  phone:        "971581206817",
  status:       "available",
  is_active:    true,
  vehicle_type: null,
  zone:         null,
  last_lat:     24.5018547,
  last_lng:     54.3737363,
  last_seen_at: "2026-08-23T09:10:09.602427Z",
  dispatchable: true,
} as AdminCourier;

/** Legacy shape, suspended by ops. */
export const LEGACY_SUSPENDED = {
  ...LEGACY_AVAILABLE,
  id:           "8a1f0000-0000-0000-0000-00000000000a",
  user_id:      "8a1f0000-0000-0000-0000-00000000000a",
  is_active:    false,
  dispatchable: false,
} as AdminCourier;

/** Post-#138: compliance has never seen this courier. */
export const CURRENT_NOT_ONBOARDED: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "b0000000-0000-0000-0000-00000000000b",
  user_id:               "b0000000-0000-0000-0000-00000000000b",
  block_reason:          null,
  compliance_status:     null,
  compliance_assignable: true,
};

/** Post-#138: compliant, working. */
export const CURRENT_COMPLIANT: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "c0000000-0000-0000-0000-00000000000c",
  user_id:               "c0000000-0000-0000-0000-00000000000c",
  block_reason:          null,
  compliance_status:     "compliant",
  compliance_assignable: true,
};

/**
 * Post-#138, observe-only: compliance has refused them and they are still
 * being offered work because `ENFORCE_COMPLIANCE` is false. `dispatchable` and
 * `compliance_assignable` disagree on purpose.
 */
export const CURRENT_OBSERVE_ONLY: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "d0000000-0000-0000-0000-00000000000d",
  user_id:               "d0000000-0000-0000-0000-00000000000d",
  dispatchable:          true,
  block_reason:          null,
  compliance_status:     "rejected",
  compliance_assignable: false,
};

/** Post-#138 with enforcement on: the server itself names compliance. */
export const CURRENT_ENFORCED_BLOCK: AdminCourier = {
  ...LEGACY_AVAILABLE,
  id:                    "e0000000-0000-0000-0000-00000000000e",
  user_id:               "e0000000-0000-0000-0000-00000000000e",
  dispatchable:          false,
  block_reason:          "compliance",
  compliance_status:     "expired",
  compliance_assignable: false,
};

/** Fixed clock for the GPS-age branch: 2026-08-23T09:15:00Z. */
export const NOW_MS = Date.parse("2026-08-23T09:15:00.000Z");
```

- [ ] **Step 2: Commit**

```bash
git add apps/admin-portal/src/lib/couriers/fixtures.ts
git commit -m "test(admin-portal): the courier payloads, including the live legacy one"
```

---

## Task 3: `complianceView` — an absent field is its own state

**Files:**
- Create: `apps/admin-portal/src/lib/couriers/compliance-view.ts`
- Create: `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`
- Modify: `apps/admin-portal/src/lib/api/couriers.ts`

- [ ] **Step 1: Make the three fields optional**

In `apps/admin-portal/src/lib/api/couriers.ts`, change the three declarations
inside `interface AdminCourier`. Keep every existing doc comment; append the
note below to each.

`block_reason` becomes:

```ts
  block_reason?: "suspended" | "off_duty" | "compliance" | null;
```

`compliance_status` becomes:

```ts
  compliance_status?: string | null;
```

`compliance_assignable` becomes:

```ts
  compliance_assignable?: boolean;
```

Then add this paragraph to the doc comment of each of the three, adjusting the
field name:

```
   * **Optional on the wire.** A field-ops built before the compliance gate does
   * not send this key at all, and portal and service are separate deploy units
   * — the skew is structural, not a one-off. `undefined` means "this build has
   * no opinion" and is not the same claim as `null`. Note that the compiler
   * does not enforce the `?`: this app sets `"strict": false`, so
   * `strictNullChecks` is off. `compliance-view.ts` and its tests are the gate.
```

- [ ] **Step 2: Write the failing tests**

Create `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`:

```ts
import {
  complianceView,
} from "./compliance-view";
import {
  LEGACY_OFF_DUTY,
  CURRENT_NOT_ONBOARDED,
  CURRENT_COMPLIANT,
  CURRENT_OBSERVE_ONLY,
} from "./fixtures";

describe("complianceView", () => {
  it("does not throw on a payload with no compliance fields", () => {
    expect(() => complianceView(LEGACY_OFF_DUTY)).not.toThrow();
  });

  it("reports an absent field as unsupported, not as onboarded or blocked", () => {
    const v = complianceView(LEGACY_OFF_DUTY);
    expect(v.kind).toBe("unsupported");
    expect(v.label).toBe("—");
  });

  it("distinguishes an absent field from an explicit null", () => {
    expect(complianceView(LEGACY_OFF_DUTY).kind).toBe("unsupported");
    expect(complianceView(CURRENT_NOT_ONBOARDED).kind).toBe("not-onboarded");
  });

  it("labels a null status as not onboarded", () => {
    expect(complianceView(CURRENT_NOT_ONBOARDED).label).toBe("not onboarded");
  });

  it("renders a known status with underscores turned into spaces", () => {
    expect(complianceView(CURRENT_OBSERVE_ONLY).label).toBe("rejected");
    expect(complianceView({ ...CURRENT_COMPLIANT, compliance_status: "pending_submission" }).label)
      .toBe("pending submission");
  });

  it("tones a refused courier differently from a compliant one", () => {
    expect(complianceView(CURRENT_COMPLIANT).tone).not.toBe(
      complianceView(CURRENT_OBSERVE_ONLY).tone,
    );
  });
});
```

- [ ] **Step 3: Run and verify it fails**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: FAIL — `Cannot find module './compliance-view'`.

- [ ] **Step 4: Write the implementation**

Create `apps/admin-portal/src/lib/couriers/compliance-view.ts`:

```ts
/**
 * What the couriers screen says about a courier, as pure functions.
 *
 * Split out of the page for the reason `lib/compliance/labels.ts` was: these
 * are decisions with real consequences — "why is this person not getting jobs?"
 * — and a decision embedded in JSX cannot be tested against a payload.
 *
 * The whole module exists because of one production failure. The deployed
 * field-ops predated the compliance gate and sent none of the three compliance
 * fields, so `compliance_status` was `undefined`; `undefined === null` is false,
 * the not-onboarded branch did not catch it, and the next line called
 * `.replace()` on it. Every row threw.
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

/** Has this payload got a compliance opinion at all? */
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
 * - `known` — compliance has spoken; show what it said, verbatim.
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
```

- [ ] **Step 5: Run and verify it passes**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: `Tests: 6 passed, 6 total`.

- [ ] **Step 6: Mutation-check the guard**

Temporarily change `hasComplianceFields` to `return true;`. Re-run.
Expected: at least 2 tests fail (`reports an absent field as unsupported…`,
`distinguishes an absent field from an explicit null`). Revert the mutation and
confirm green again. If nothing failed, the tests are not covering the guard.

- [ ] **Step 7: Commit**

```bash
git add apps/admin-portal/src/lib/couriers/compliance-view.ts \
        apps/admin-portal/src/lib/couriers/compliance-view.test.ts \
        apps/admin-portal/src/lib/api/couriers.ts
git commit -m "fix(admin-portal): an absent compliance field is not a null one"
```

---

## Task 4: `dispatchView` — never guess GPS when the reason is knowable

**Files:**
- Modify: `apps/admin-portal/src/lib/couriers/compliance-view.ts`
- Modify: `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`

- [ ] **Step 1: Write the failing tests**

Append to `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`:

```ts
import { dispatchView } from "./compliance-view";
import {
  LEGACY_AVAILABLE,
  LEGACY_SUSPENDED,
  CURRENT_ENFORCED_BLOCK,
  NOW_MS,
} from "./fixtures";

describe("dispatchView", () => {
  it("does not throw on a payload with no block_reason", () => {
    expect(() => dispatchView(LEGACY_OFF_DUTY, NOW_MS)).not.toThrow();
  });

  it("says an offline courier is off duty rather than guessing at their GPS", () => {
    const v = dispatchView(LEGACY_OFF_DUTY, NOW_MS);
    expect(v.label).toBe("not on duty");
    expect(v.label).not.toMatch(/on duty/i === null ? /x/ : /last fix|never sent/);
  });

  it("derives suspension from is_active when the server did not say", () => {
    expect(dispatchView(LEGACY_SUSPENDED, NOW_MS).label).toBe("suspended by ops");
  });

  it("ranks suspension above duty, as the server does", () => {
    const both = { ...LEGACY_SUSPENDED, status: "offline" } as AdminCourier;
    expect(dispatchView(both, NOW_MS).label).toBe("suspended by ops");
  });

  it("prefers the server's block_reason over anything it could derive", () => {
    expect(dispatchView(CURRENT_ENFORCED_BLOCK, NOW_MS).label)
      .toBe("blocked · expired");
  });

  it("never claims compliance blocks on a payload that cannot know", () => {
    for (const c of [LEGACY_OFF_DUTY, LEGACY_AVAILABLE, LEGACY_SUSPENDED]) {
      expect(dispatchView(c, NOW_MS).label).not.toMatch(/compliance/i);
    }
  });

  it("still flags a stale fix for an on-duty courier", () => {
    const stale = { ...LEGACY_AVAILABLE, last_seen_at: "2026-08-23T08:00:00.000Z" } as AdminCourier;
    expect(dispatchView(stale, NOW_MS).label).toMatch(/stale/);
  });

  it("flags a courier who has never sent a position", () => {
    const never = { ...LEGACY_AVAILABLE, last_seen_at: null } as AdminCourier;
    expect(dispatchView(never, NOW_MS).label).toMatch(/never sent a position/);
  });

  it("says a healthy legacy courier is receiving offers", () => {
    expect(dispatchView(LEGACY_AVAILABLE, NOW_MS).label).toBe("receiving offers");
  });

  it("shows the observe-only disagreement when compliance is known", () => {
    expect(dispatchView(CURRENT_OBSERVE_ONLY, NOW_MS).label)
      .toBe("receiving offers · compliance would block");
  });
});
```

Also add `AdminCourier` and the extra fixture names to the existing imports at
the top of the test file so the whole file has one import block each:

```ts
import type { AdminCourier } from "@/lib/api/couriers";
```

- [ ] **Step 2: Simplify the one awkward assertion**

The second test above contains a contorted expression. Replace that test body
with the plain version:

```ts
  it("says an offline courier is off duty rather than guessing at their GPS", () => {
    const v = dispatchView(LEGACY_OFF_DUTY, NOW_MS);
    expect(v.label).toBe("not on duty");
    expect(v.label).not.toMatch(/last fix|never sent/);
  });
```

- [ ] **Step 3: Run and verify it fails**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: FAIL — `dispatchView is not a function` / not exported.

- [ ] **Step 4: Write the implementation**

Append to `apps/admin-portal/src/lib/couriers/compliance-view.ts`:

```ts
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
 * The proximity search only considers a position from the last ten minutes, so
 * a courier who is active and on duty is still invisible to dispatch if their
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
 * answering.** The old code fell through to the GPS branch and reported an
 * `offline` courier as "on duty · last fix Nm ago (stale)" — a wrong answer,
 * stated confidently, in the column that exists to prevent exactly that.
 *
 * So absence falls back to deriving the two reasons that are visible in the
 * legacy payload, in `Courier::dispatch_block`'s own order — suspended, then
 * off duty. Compliance is deliberately not derived and not guessed: it is the
 * one reason a client can never see, and a build that omits the field has no
 * compliance term to report anyway.
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
      label: `blocked · ${(c.compliance_status ?? "unknown").replace(/_/g, " ")}`,
      tone:  "text-rose-300",
    };
  }

  const age = fixAgeMinutes(c.last_seen_at, nowMs);
  if (age === null) {
    return { label: "on duty · never sent a position", tone: "text-amber-300" };
  }
  if (age > STALE_FIX_MINUTES) {
    return { label: `on duty · last fix ${age}m ago (stale)`, tone: "text-amber-300" };
  }

  // Observe-only: compliance has refused them and enforcement is off, so they
  // are still getting work. Only sayable when the server told us — a payload
  // without the field has no verdict to disagree with.
  if (hasComplianceFields(c) && c.compliance_assignable === false) {
    return {
      label: "receiving offers · compliance would block",
      tone:  "text-amber-300",
      title: "Compliance has refused this courier, but enforcement is off in this deployment so they are still being offered work. Turning enforcement on will stop them.",
    };
  }

  return { label: "receiving offers", tone: "text-emerald-300" };
}

/**
 * The two reasons a legacy payload still makes visible, in the server's order.
 *
 * `Courier::dispatch_block` checks `!is_active` before
 * `status != Available` before compliance. Reproducing the first two exactly is
 * what makes this fallback safe: it is not a second opinion, it is the same
 * rule over the same two fields.
 */
function derivedBlockReason(c: AdminCourier): "suspended" | "off_duty" | null {
  if (!c.is_active) return "suspended";
  if (c.status !== "available") return "off_duty";
  return null;
}
```

- [ ] **Step 5: Run and verify it passes**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: `Tests: 16 passed, 16 total`.

- [ ] **Step 6: Mutation-check the fallback**

Temporarily change `derivedBlockReason` to `return null;`. Re-run.
Expected: at least 3 failures, including *"says an offline courier is off duty
rather than guessing at their GPS"* — that is the production bug reappearing.
Revert and confirm green.

- [ ] **Step 7: Commit**

```bash
git add apps/admin-portal/src/lib/couriers/compliance-view.ts \
        apps/admin-portal/src/lib/couriers/compliance-view.test.ts
git commit -m "fix(admin-portal): an offline courier is off duty, not a stale GPS fix"
```

---

## Task 5: `courierCounts` — a dash is not a zero

**Files:**
- Modify: `apps/admin-portal/src/lib/couriers/compliance-view.ts`
- Modify: `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`

- [ ] **Step 1: Write the failing tests**

Append to `apps/admin-portal/src/lib/couriers/compliance-view.test.ts`
(add `courierCounts` to the import from `./compliance-view`):

```ts
describe("courierCounts", () => {
  const legacy = [LEGACY_OFF_DUTY, LEGACY_AVAILABLE, LEGACY_SUSPENDED];

  it("counts what a legacy payload does support", () => {
    const n = courierCounts(legacy);
    expect(n.total).toBe(3);
    expect(n.dispatchable).toBe(1);
    expect(n.suspended).toBe(1);
  });

  it("returns null, not zero, for counts a legacy payload cannot support", () => {
    const n = courierCounts(legacy);
    expect(n.complianceBlocked).toBeNull();
    expect(n.notOnboarded).toBeNull();
  });

  it("counts compliance once the server reports it", () => {
    const n = courierCounts([CURRENT_COMPLIANT, CURRENT_NOT_ONBOARDED, CURRENT_OBSERVE_ONLY]);
    expect(n.complianceBlocked).toBe(1);
    expect(n.notOnboarded).toBe(1);
  });

  it("ignores couriers the server said nothing about when mixing payloads", () => {
    const n = courierCounts([LEGACY_AVAILABLE, CURRENT_OBSERVE_ONLY]);
    expect(n.complianceBlocked).toBe(1);
    expect(n.notOnboarded).toBe(0);
  });

  it("is empty-safe", () => {
    const n = courierCounts([]);
    expect(n.total).toBe(0);
    expect(n.complianceBlocked).toBeNull();
  });
});
```

- [ ] **Step 2: Run and verify it fails**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: FAIL — `courierCounts is not a function`.

- [ ] **Step 3: Write the implementation**

Append to `apps/admin-portal/src/lib/couriers/compliance-view.ts`:

```ts
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
 * ("nobody is blocked") is one this payload cannot support. The old code
 * counted `!undefined` and reported every courier as blocked while reporting
 * none as un-onboarded: both tiles exactly inverted.
 *
 * Couriers the server said nothing about are excluded from the compliance
 * counts rather than assumed either way, so a mixed roster mid-deploy reports
 * only what is actually known.
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
```

- [ ] **Step 4: Run and verify it passes**

```bash
cd apps/admin-portal && npm test -- --ci compliance-view
```

Expected: `Tests: 21 passed, 21 total`.

- [ ] **Step 5: Mutation-check**

Temporarily change `complianceBlocked` to
`couriers.filter((c) => !c.compliance_assignable).length` — the original buggy
expression. Re-run. Expected: *"returns null, not zero…"* fails. Revert and
confirm green.

- [ ] **Step 6: Commit**

```bash
git add apps/admin-portal/src/lib/couriers/compliance-view.ts \
        apps/admin-portal/src/lib/couriers/compliance-view.test.ts
git commit -m "fix(admin-portal): both compliance tiles read exactly backwards"
```

---

## Task 6: Wire the page to the module

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/couriers/page.tsx`

- [ ] **Step 1: Replace the local helpers**

Delete the local `fixAgeMinutes`, `CompliancePill` and `DispatchCell`
definitions, and the `counts` `useMemo` body. Replace the import block and those
definitions so the file reads:

```tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { motion } from "framer-motion";
import { Bike, RefreshCw, ShieldOff, ShieldCheck, AlertTriangle } from "lucide-react";
import { toast } from "sonner";

import { variants } from "@/lib/design-system/tokens";
import { GlassCard } from "@/components/ui/glass-card";
import { fetchCouriers, setCourierActive, type AdminCourier } from "@/lib/api/couriers";
import { complianceView, dispatchView, courierCounts } from "@/lib/couriers/compliance-view";

function CompliancePill({ c }: { c: AdminCourier }) {
  const v = complianceView(c);
  return (
    <span className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${v.tone}`} title={v.title}>
      {v.label}
    </span>
  );
}

function DispatchCell({ c, nowMs }: { c: AdminCourier; nowMs: number }) {
  const v = dispatchView(c, nowMs);
  return <span className={`text-[12px] ${v.tone}`} title={v.title}>{v.label}</span>;
}
```

- [ ] **Step 2: Give the page a clock it controls**

`dispatchView` takes the current time rather than reading it, so it can be
tested. Inside `CouriersPage`, add state stamped whenever the roster loads, and
use it in the table:

```tsx
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
```

In `load()`, immediately after `setCouriers(await fetchCouriers());`, add:

```tsx
      setNowMs(Date.now());
```

Replace the counts memo with:

```tsx
  const counts = useMemo(() => courierCounts(couriers), [couriers]);
```

And the Dispatchable cell in the table body with:

```tsx
                  <td className="px-4 py-3"><DispatchCell c={c} nowMs={nowMs} /></td>
```

- [ ] **Step 3: Render a dash for an unknowable count**

Replace the tile array so `null` renders as `—`:

```tsx
        {([
          ["Couriers", counts.total, "text-white", undefined],
          ["Receiving offers", counts.dispatchable, "text-emerald-300", undefined],
          ["Suspended", counts.suspended, "text-rose-300", undefined],
          ["Compliance blocked", counts.complianceBlocked, "text-amber-300",
            "This deployment's field-ops reports nothing about compliance, so this cannot be counted."],
          ["Not onboarded", counts.notOnboarded, "text-white/60",
            "This deployment's field-ops reports nothing about compliance, so this cannot be counted."],
        ] as [string, number | null, string, string | undefined][]).map(([label, value, tone, title]) => (
          <GlassCard key={label} className="p-4">
            <div className="text-[11px] uppercase tracking-wide text-white/40">{label}</div>
            <div
              className={`mt-1 text-2xl font-semibold ${value === null ? "text-white/25" : tone}`}
              title={value === null ? title : undefined}
            >
              {value === null ? "—" : value}
            </div>
          </GlassCard>
        ))}
```

- [ ] **Step 4: Update the file's header comment**

The header says the Dispatchable column has "four independent answers". Add a
fifth paragraph recording what actually happened:

```
 * A sixth answer turned out to be "the server did not say". The deployed
 * field-ops predated the compliance gate and sent none of the three compliance
 * fields, so this page threw on every row and, before it threw, reported an
 * offline courier as an on-duty one with a stale GPS fix. The decisions now
 * live in `lib/couriers/compliance-view.ts` where they are tested against that
 * exact payload — portal and service are separate deploy units and the skew is
 * structural.
```

- [ ] **Step 5: Type-check**

```bash
cd apps/admin-portal && rm -f tsconfig.tsbuildinfo && node node_modules/typescript/bin/tsc --noEmit
```

Expected: no errors. Clearing `tsconfig.tsbuildinfo` first is required — a stale
incremental cache makes a type check report clean without looking at the file.

- [ ] **Step 6: Confirm the type check actually covers this file**

Inject a deliberate error — change `courierCounts(couriers)` to
`courierCounts(couriers, 1)` — and re-run the command from Step 5. Expected:
an arity error naming `page.tsx`. Revert it and re-run to confirm clean.

- [ ] **Step 7: Lint and full test run**

```bash
cd apps/admin-portal && npm run lint && npm test -- --ci
```

Expected: lint clean; `Tests: 21 passed, 21 total`.

- [ ] **Step 8: Commit**

```bash
git add apps/admin-portal/src/app/\(dashboard\)/couriers/page.tsx
git commit -m "refactor(admin-portal): the couriers page delegates its decisions to a tested module"
```

---

## Task 7: Deploy field-ops and verify

Not optional, and not a substitute for Tasks 1–6: the code fix makes the page
honest *while* the deploy is outstanding. Run on the VPS `75.119.138.135`, in
`/etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/`.

- [ ] **Step 1: Record the before state**

```bash
docker exec logisticos-field-ops grep -a -c compliance_assignable /app/field_ops
```

Expected: `0`. This is the evidence the deploy is behind.

- [ ] **Step 2: Pull and restart**

```bash
docker compose pull field-ops && docker compose up -d field-ops
```

- [ ] **Step 3: Verify the binary, not the tag**

```bash
docker exec logisticos-field-ops grep -a -c compliance_assignable /app/field_ops
```

Expected: `1` or more. An HTTP probe cannot confirm this — auth runs before
routing, so a present and an absent route both answer 401.

- [ ] **Step 4: Verify migration 0009 applied**

```bash
docker exec logisticos-postgres psql -U logisticos -d svc_field_ops \
  -c "SELECT version, success FROM field_ops._sqlx_migrations ORDER BY version;"
```

Expected: versions through 9, `success` true for all. A migration that cannot
apply silently pins the service to its last-good image.

- [ ] **Step 5: Re-probe the live API**

Run the probe from the local machine:

```bash
python scripts/probe-couriers.py
```

Create that script first if it does not exist — it logs in as `admin@demo.com`
against `https://os-api.cargomarket.net` with a `curl/8.4.0` User-Agent
(Cloudflare answers urllib's default agent with a 1010 ban that reads like an
auth failure) and prints the raw keys of each courier object.

Expected: `block_reason`, `compliance_status` and `compliance_assignable` are
present on every courier, and `block_reason` is `"off_duty"` for courier
`10ae4c3d`.

- [ ] **Step 6: Create the `driver` Kafka topic and restart compliance**

```bash
bash scripts/create-kafka-topics.sh
docker restart logisticos-compliance
```

The `driver` topic has never existed and a consumer that subscribed before the
first publish never recovers on its own.

- [ ] **Step 7: Redeploy the admin portal**

```bash
cd /etc/dokploy/applications/oscargomarketnet-admin-mr96mp/code
git fetch origin && git reset --hard origin/master
docker build -f apps/admin-portal/Dockerfile -t oscargomarketnet-admin-mr96mp:latest .
docker service update --force --image oscargomarketnet-admin-mr96mp:latest oscargomarketnet-admin-mr96mp
```

- [ ] **Step 8: Verify the portal build carries this change**

```bash
C=$(docker ps --format '{{.Names}}' | grep admin-mr96mp)
docker exec "$C" sh -lc 'grep -rl "predates the compliance gate" /app/.next | head'
```

Expected: at least one hit under `/app/.next`. Check the build output for a
string unique to this change, never the image tag — the checkout advancing is
not a deploy and a container restart is not a rebuild.

---

## Self-Review

**Spec coverage:**

| Spec section | Task |
|---|---|
| Pure module `compliance-view.ts` | 3, 4, 5 |
| `complianceView` / absent ≠ null | 3 |
| `dispatchView` + `block_reason` fallback in server order | 4 |
| `courierCounts` returning `null` | 5 |
| Optional types + note that tsc does not enforce them | 3 Step 1 |
| Tiles render `—` not `0` | 5, 6 |
| jest via `next/jest`, no new packages | 1 |
| Three payload shapes incl. captured legacy | 2 |
| Every guard mutation-checked | 3 S6, 4 S6, 5 S5 |
| Deploy: pull, binary grep, migrations, probe, Kafka topic, portal rebuild | 7 |

No spec requirement is unassigned. Out-of-scope items (drill-down,
upload-on-behalf, `ENFORCE_COMPLIANCE`, profile backfill) have no tasks, as
intended.

**Placeholder scan:** none — every code step carries complete code, every
command carries its expected output.

**Type consistency:** `complianceView` / `dispatchView` / `courierCounts` /
`hasComplianceFields` / `fixAgeMinutes` / `derivedBlockReason` are named
identically in every task. `ComplianceView`, `DispatchView` and `CourierCounts`
are each defined once, in the task that first uses them. `dispatchView` takes
`(c, nowMs)` in its definition (Task 4) and at both call sites (Tasks 4, 6).
