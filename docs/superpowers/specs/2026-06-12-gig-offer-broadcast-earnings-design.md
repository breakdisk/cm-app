# Gig Task Offer Broadcast ("Grab") + Driver Earnings in Profile

**Date:** 2026-06-12
**Status:** Approved (design reviewed; review feedback resolved inline — see "Review resolutions")
**Scope:** Dispatch broadcast-offer machinery with atomic claim, driver-ops earnings API,
payments driver-scoped ledger read, Android Home grab card + Profile earnings UI.

## Problem

Commit `512c0a0` shipped rich home-screen task cards, but the underlying model is still
1:1: `quick_dispatch` picks **one** driver and the card is a viewer for that pre-made
decision. There is no contention — a hundred gig drivers never see the same task, so a
"grab" is impossible. Decline requires manual ops re-dispatch. Separately, gig drivers
have no view of what they have earned or what COD cash they owe the hub; the Profile tab
has no financial section despite `DriverLedger` (payments) and `per_delivery_rate_cents`
(driver-ops) already existing.

## Decisions (with user)

1. **Contention model: broadcast waves.** Offer fans out to the top-N nearest eligible
   gig drivers simultaneously; first atomic claim wins; unclaimed offers escalate through
   widening waves. (Rejected: open task board — sniping/thundering herd; sequential
   cascade — too slow at scale.)
2. **Profile financial scope: earnings + COD ledger.** Per-task payout history with
   daily/weekly totals, plus cash-position view from the existing payments `DriverLedger`.
   No wallet/payout rails (deferred — separate project requiring disbursement + KYC).

## Part 1 — Offer broadcast & atomic claim

### Entities (dispatch schema, new migration)

```sql
CREATE TABLE dispatch.task_offers (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id     UUID NOT NULL,
    shipment_id   UUID NOT NULL,
    queue_id      UUID NOT NULL,
    status        TEXT NOT NULL DEFAULT 'open',  -- open|claimed|expired|cancelled
    wave          INT  NOT NULL DEFAULT 1,        -- 1..3
    payout_cents  BIGINT,                         -- snapshot at offer creation; contractual
    expires_at    TIMESTAMPTZ NOT NULL,
    claimed_by    UUID,
    claimed_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE dispatch.task_offer_candidates (
    offer_id    UUID NOT NULL REFERENCES dispatch.task_offers(id),
    driver_id   UUID NOT NULL,
    wave        INT  NOT NULL,
    notified_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    seen_at     TIMESTAMPTZ,            -- client-verified impression (see metrics)
    response    TEXT NOT NULL DEFAULT 'none',  -- none|passed
    PRIMARY KEY (offer_id, driver_id)
);

-- Closes the concurrent double-grab race AND a latent check-then-insert race in
-- create_route_plan / assign_driver / quick_dispatch (all read find_active_by_driver
-- under READ COMMITTED before inserting):
CREATE UNIQUE INDEX idx_one_active_assignment_per_driver
ON dispatch.driver_assignments (driver_id)
WHERE status IN ('pending', 'accepted');
```

### Flow

1. **Broadcast.** New `broadcast_dispatch` path, invoked from the ops dispatch console
   as a "Broadcast to gig drivers" action beside the existing quick-dispatch button
   (automatic broadcast-on-create per tenant config is a follow-up flag, not in scope).
   It reuses the
   `quick_dispatch` candidate pipeline — `find_available_near` → `vehicle_can_carry` →
   compliance gate — but instead of `min_by(score)` takes the **top 10**, creates one
   `task_offers` row (TTL **30 s**, `payout_cents` snapshotted from
   `per_delivery_rate_cents`), records candidates, and fans out the existing rich FCM
   payload as `type=task_offer`. `quick_dispatch` is untouched (dispatcher-targeted +
   full-time assignment).

2. **Atomic claim.** `POST /v1/offers/:id/claim` (driver JWT):

   ```sql
   UPDATE dispatch.task_offers
   SET status='claimed', claimed_by=$driver, claimed_at=now()
   WHERE id=$1 AND status='open' AND expires_at > now()
   ```

   - 0 rows → `409 OFFER_TAKEN` (or expired).
   - 1 row → same transaction: insert `DriverAssignment` (born `accepted`), insert
     route, flip queue item to `dispatched`. The partial unique index is the
     one-active-assignment guard; a `23505 unique_violation` (driver grabbed another
     offer concurrently) rolls back the claim → `409 DRIVER_BUSY`.
   - **No network I/O inside the transaction.** `TaskAssigned` (winner) and FCM
     `type=offer_closed` to the losing candidates publish after commit.
   - `SET LOCAL lock_timeout = '250ms'` on the claim path: blocked losers shed load
     fast instead of stacking on the pool if the winner's commit ever stalls.

3. **Wave escalation.** Sweeper (tokio interval, same pattern as dispatch orphan
   cleanup): expired wave → wave+1 with widened radius (3 → 6 → 10 km), excluding
   candidates with `response='passed'`. After wave 3 → `status='expired'`, surfaces in
   the ops dispatch console (manual path remains the non-AI fallback).

4. **Decline semantics (gig).** "Pass" records `response='passed'` (never re-offered)
   but does **not** increment `decline_count`; ignoring an offer carries no penalty.
   The 20-decline ban applies only to targeted 1:1 assignments. Gig performance strip
   shows **acceptance rate** = claims ÷ impression-verified offers seen.

5. **Impression-verified metrics.** `task_offer_candidates.seen_at` is set by a
   fire-and-forget `POST /v1/offers/:id/seen` fired from `TaskCards.kt` at first card
   composition. FCM delivery (candidate row) is NOT an impression — counting it would
   penalize drivers in dead zones / background mode. Self-correcting: an offline driver
   can neither render nor claim a 30 s offer, so absent impressions fairly drop them
   from both numerator and denominator. No offline queue for this endpoint.

6. **Payout integrity.** The price on the grab card is contractual: `payout_cents` is
   snapshotted on the offer and copied to the task on claim. Rate changes never alter
   accepted work. This snapshot is the earnings record Part 2 reads.

### Android Home UI

- Offer card → **grab card**: countdown ring (TTL), payout chip prominent, full-width
  **GRAB** (green/glow) + quiet "Pass" text button. Claim fires on tap (optimistic UI);
  409 flips the card to a brief "Taken" state (<300 ms feedback) then dismisses.
- Accepted task cards keep current behavior: tap → Route tab.
- App restart / dismissed card: `GET /v1/offers/open` (driver-scoped) repopulates live
  offers — FCM is not a single point of failure.

## Part 2 — Earnings & financial history in Profile

Two money views, two systems of record — not merged:

1. **Earnings — driver-ops owns it.** With `payout_cents` snapshotted per task,
   earnings history is a query, not a ledger:
   `GET /v1/drivers/me/earnings?from&to` → daily/weekly totals + per-task entries
   (date, AWB, merchant, category, payout). Supporting index
   `(driver_id, status, completed_at)`; detail list strictly paginated.
   **No rollup table** (reviewed and rejected — see resolutions). Future bonus /
   adjustment line items: additive `earnings_adjustments` table, same API shape.
2. **Cash position — payments owns it.** New driver-scoped read endpoint
   `GET /v1/drivers/me/ledger`: current open `DriverLedger` (balance + entries) +
   recent reconciled ledgers. Driver JWT identity, same auth pattern as driver-ops.

### Profile tab UI

New **Earnings** section between profile header and compliance section:

- **Summary card** — Today / This Week totals + 7-day sparkline.
- **Cash to Remit card** (only when open ledger balance > 0) — amber-glow alert,
  balance + "from N COD deliveries" (money owed to the hub).
- Tap → full **Earnings screen** (profile feature module): tabs *Earnings* / *Cash*;
  lists grouped by day; row = AWB · merchant · amount; cash tab shows ledger entries
  (debit red, remittance green).
- Full-time drivers: Cash tab only (payout invisibility rule preserved). Gig: both.

## Review resolutions

External design review raised four runtime concerns; resolution after codebase
verification:

| # | Concern | Resolution |
|---|---------|-----------|
| 1 | Concurrent double-grab (one driver, two offers, two fingers) passes the read-guard in both transactions | **Accepted.** Partial unique index above (statuses corrected to `pending/accepted`; failure mode is `23505 unique_violation` → 409, not a serialization error). Also fixes the same latent race in three existing call sites. |
| 2 | Candidate-row "offers seen" denominator punishes offline/background phones | **Accepted.** Client impression endpoint + `seen_at`; denominator = verified impressions only. |
| 3 | Claim-lock waiters could saturate the pool at larger wave sizes | **Accepted in substance.** Real lever is winner tx duration (already no network I/O inside tx); added `lock_timeout='250ms'` as load-shedding. Statement reordering doesn't help — losers block on the row lock until winner commits regardless. |
| 4 | Earnings-by-query needs a daily rollup cache table | **Rejected (YAGNI).** ~11k rows/driver/year; indexed range SUM is single-digit ms, summary card scans ~7 days. Async rollup adds consistency lag (delivery done, money not visible — trust damage) and a worker/trigger against codebase conventions. Escape hatch documented: add rollup only if p99 > 200 ms budget is breached with data. |

## Build order

```
Phase 1: snapshot payout_cents on offer/task (+ migration, contract fields)
Phase 2: earnings API (driver-ops) + ledger read (payments) + Profile UI
Phase 3: grab machinery — task_offers, claim CAS, waves, sweeper, Home grab card,
         offer_closed fan-in, acceptance-rate strip
```

Phase 2 depends only on Phase 1; Phase 3 is the largest and lands last.

## Error handling

- Claim races: deterministic DB outcomes (CAS 0-rows / 23505) mapped to typed 409s;
  app renders "Taken" state.
- FCM enrichment and `offer_closed` fan-out are fire-and-forget (existing pattern);
  `GET /v1/offers/open` is the recovery path.
- Sweeper idempotent: wave escalation keyed on `(offer_id, wave)`; expired offers are
  terminal.
- Earnings endpoints read-only; no money movement anywhere in this scope.

## Testing

- Rust: concurrency test for the claim CAS (two simultaneous claims → exactly one
  winner); unique-index violation mapping; wave-escalation exclusion of passed
  candidates; earnings aggregation correctness; ledger endpoint tenant/driver scoping.
- Android: validated via GitHub Actions CI per project convention (no local Gradle).
