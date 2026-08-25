# Admin Portal → OmniDeliv Couriers: surviving payload skew

**Date:** 2026-08-25
**Status:** approved, implementing
**Surface:** `apps/admin-portal` → `/admin/couriers`, plus a `field-ops` deploy

## The gap

The couriers page renders a Compliance column and two KPI tiles from three
fields — `compliance_status`, `compliance_assignable`, `block_reason` — that the
**deployed** `field-ops` does not send. The page is broken in production now.

Found by running it against the live deployment, not by reading it:

```
GET /v1/field-ops/admin/couriers   → 13 keys, none of those three
docker images | grep field-ops     → latest built 2026-08-23 12:52
                                      (PR #138 merged 2026-08-24)
docker exec logisticos-field-ops \
  grep -a -c compliance_assignable /app/field_ops   → 0
```

The container was recreated 2026-08-24 12:06 — on the old image. **A restart is
not a pull.** Meanwhile the admin-portal image (built 2026-08-25 10:05) *does*
carry the column: `grep -rl "compliance would block" /app/.next` hits
`(dashboard)/couriers/page.js`.

### Three consequences, all live

1. **The table throws.** `compliance_status` is `undefined`, so
   `c.compliance_status === null` is `false`, and the next line calls
   `.replace()` on it. TypeError on every row of the only management surface
   couriers have.
2. **The Dispatchable column states something false.** `block_reason` is absent,
   so all three of its checks fall through to the GPS-staleness branch. Courier
   `10ae4c3d`, whose `status` is `"offline"`, renders as *"on duty · last fix
   Nm ago (stale)"*. That column exists precisely to stop wrong answers to "why
   isn't this person getting jobs?"
3. **Both tiles invert.** `complianceBlocked` counts `!undefined` → 3 of 3;
   `notOnboarded` counts `=== null` → 0. The truth is the exact opposite: all
   three couriers are unknown to compliance and none is blocked.

## Why this is a code defect and not only a missed deploy

`field-ops` and `admin-portal` are separate deploy units — one a GHCR image in a
Dokploy compose app, the other a Dokploy application built from source. Neither
waits for the other, and zero-downtime rolling deploys are a stated
non-negotiable. **A portal that hard-crashes because a backend field has not
arrived yet is broken by construction**, independent of this particular missed
`docker compose pull`.

## The distinction the code is missing

| wire | means | current behaviour |
|---|---|---|
| `compliance_status: null` | compliance has never spoken about this courier | "not onboarded" — correct, keep |
| key **absent** | this `field-ops` build has no compliance concept | falls past the `null` branch, then throws |

They are not the same claim and must not render the same. In the second case
neither "not onboarded" nor "blocked" is true: nothing is knowable.

## Design

### A pure module for the decisions

New `apps/admin-portal/src/lib/couriers/compliance-view.ts`, mirroring the
convention already set by `src/lib/compliance/labels.ts`: pure functions over
data the caller fetched, tested independently, page stays presentational.

```ts
type ComplianceView =
  | { kind: "unsupported";   label: string; tone: string; title: string }
  | { kind: "not-onboarded"; label: string; tone: string; title: string }
  | { kind: "known";         label: string; tone: string; title?: string };

complianceView(c: AdminCourier): ComplianceView
dispatchView(c: AdminCourier, nowMs: number): { label: string; tone: string; title?: string }
courierCounts(list: AdminCourier[]): {
  total: number; dispatchable: number; suspended: number;
  complianceBlocked: number | null;   // null = not knowable from this payload
  notOnboarded:      number | null;
}
```

### The `block_reason` fallback, and why re-deriving is correct here

`AdminCourier.block_reason`'s doc comment forbids re-deriving the rule
client-side. That rule holds **while the server answers**. When the field is
absent the server is not answering, and today's fall-through states something
flatly false.

`Courier::dispatch_block` (`services/field-ops/src/domain/entities/courier.rs`)
orders its reasons:

1. `!is_active` → `suspended`
2. `status != Available` → `off_duty`
3. `enforce_compliance && !compliance_assignable` → `compliance`

The first two derive **exactly** from `is_active` and `status`, both of which the
legacy payload already sends. So the fallback reproduces the server's own answer
for the two knowable reasons, in the server's own order, and stays silent about
compliance — which is right, because a build without the compliance term has no
verdict to report. The one reason a client can never derive is the one it never
guesses.

Precedence within `dispatchView`, whichever path supplied the reason:
suspended → off duty → compliance → stale/absent GPS fix → receiving offers.
The GPS branch stays last and stays client-derived; it is the one reason that
legitimately lives in the query rather than on the row.

### Types

The three fields become optional on `AdminCourier`:

```ts
block_reason?:          "suspended" | "off_duty" | "compliance" | null;
compliance_status?:     string | null;
compliance_assignable?: boolean;
```

That is what the wire actually is during skew.

⚠ **This is documentation, not a gate, and the difference matters.** The
original design said `tsc --noEmit` would then force every consumer to handle
absence. It will not: `apps/admin-portal/tsconfig.json` sets `"strict": false`,
so `strictNullChecks` is off and `undefined` is assignable to every type.
Verified rather than assumed — a probe file declaring `s?: string | null` and
calling `p.s.replace(...)` type-checks clean.

That is also *why this bug reached production*: the field was already typed
`string | null` and the compiler said nothing about `.replace()` on it. Turning
`strictNullChecks` on for this app is out of scope — it would surface a mountain
of pre-existing errors and hold CI red for code this change never touches, the
same reasoning `ci-frontend.yml` already applies to lint warnings.

**The jest suite below is the only real gate.** The optional markers earn their
place by telling the next reader what the wire actually carries, not by
enforcing it.

`buildEntityNames` in `labels.ts` reads only `id`, `user_id`, `first_name`,
`last_name` and is unaffected.

### Tiles

When the payload carries no compliance fields, "Compliance blocked" and "Not
onboarded" are not computable. They render `—`, never `0`: `0` is a claim, and
the claim it makes ("nobody is blocked") is one this data cannot support. The
dash also makes a lagging backend visible on the screen instead of silent.

`Couriers`, `Receiving offers` and `Suspended` remain computable from the legacy
payload and keep rendering numbers.

### Tests

`apps/admin-portal` has `jest` in devDependencies, **no jest config, and zero
test files**. `npm test -- --coverage --ci --passWithNoTests` in the
`Unit Tests (admin-portal)` job of `CI — Frontend` has therefore always been a
green no-op — the same dead-harness shape recorded elsewhere in this repo.

Add `jest.config.js` built on `next/jest` (SWC transform, already a Next 14
dependency — no new packages) with the default `node` environment, since the
module under test is pure TypeScript with no DOM.

Suite `src/lib/couriers/compliance-view.test.ts` driven by three payload shapes:

- **legacy** — the exact 13-key object captured from the live API on 2026-08-25
- **current** — post-#138, all three fields present, including a courier with
  `compliance_assignable: false` while `dispatchable: true` (observe-only)
- **null-status** — a post-#138 courier compliance has never seen

Assertions that matter:

- legacy payload never throws, and every state is reachable without one
- legacy off-duty courier reads "not on duty", **not** the GPS branch
- legacy `complianceBlocked` / `notOnboarded` are `null`, not `0`
- `null` status renders "not onboarded"; absent renders the unsupported state;
  the two are distinguishable
- observe-only disagreement (`dispatchable && !compliance_assignable`) still
  renders "receiving offers · compliance would block"
- server `block_reason` outranks the client fallback when both could speak

Each guard is mutation-checked: reverting it must fail a named test.

## Deploy

The code fix does not remove the need for the deploy — it makes the page honest
while the deploy is outstanding. Both happen.

1. `docker compose pull field-ops && docker compose up -d field-ops` in
   `/etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/`
2. `docker exec logisticos-field-ops grep -a -c compliance_assignable /app/field_ops`
   → 0 becomes ≥ 1. An HTTP probe cannot distinguish a missing route from a
   present one; auth runs before routing and both answer 401.
3. field-ops migrations 8 → 9, 0 failed. A migration that cannot apply silently
   pins the service to its last-good image.
4. Re-run the live probe: the three keys present, and `block_reason` is
   `"off_duty"` for courier `10ae4c3d`.
5. `bash scripts/create-kafka-topics.sh` — the `driver` topic is still missing,
   and a consumer that subscribed before the first publish never recovers on its
   own. Restart `logisticos-compliance` afterwards.
6. Rebuild and redeploy `oscargomarketnet-admin-mr96mp` to ship this fix. Verify
   by grepping `/app/.next` for a string unique to this change, not by the image
   tag.

## Explicitly out of scope

- `ENFORCE_COMPLIANCE` stays `false`. Nothing here changes when it flips.
- **No drill-down** from a courier row to their compliance documents. It is a
  real gap — the Compliance column is a dead end, and a courier at
  `pending_submission` appears in the compliance console nowhere at all, because
  that console renders only the pending-review queue. Separate piece of work.
- **No admin upload-on-behalf endpoint.** Still absent; still the blocker on a
  field worker without the app leaving `pending_submission`.
- No backfill of compliance profiles for the three existing couriers.
