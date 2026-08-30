# Vendor Notification: Tier 0 and the Recovery Ladder

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make an unanswered vendor leg fail loudly instead of silently, and put the vendor's queue in front of a human who can answer it.

**Architecture:** A sweep — not a consumer, because a leg nobody answered is defined by an event that never arrived, and only a timer can notice that. The decision is a pure function of two timestamps so the policy is testable without a clock. Tier 0 of the transport is the merchant-portal console: a poll and an audible alarm that will not stop until someone acknowledges.

**Tech Stack:** Rust, SQLx, Tokio timers; Next.js 14 App Router, TypeScript, TailwindCSS.

---

## Scope

This is subsystem 2, following `2026-08-30-vendor-leg-acceptance.md` (subsystem 1, shipped). It builds the two pieces that are **fully unblocked** and that together turn a queue into a notification loop.

| In | Why now |
|---|---|
| Recovery ladder for unanswered legs | Backend only, no external dependency. It is what makes acceptance safe to gate on later. |
| Merchant-portal vendor console (Tier 0) | The API shipped with nothing rendering it. This is the tier that works in a kitchen. |

**Deferred, with reasons — not oversights:**

- **FCM push (Tier 1).** `FcmClient` exists in `driver-ops` but omnideliv has none, and the VPS `.env` still lacks `FCM_PROJECT_ID` / `FCM_SERVICE_ACCOUNT_JSON`. Building the client before the credentials exist produces a code path nobody can run.
- **WhatsApp / SMS (Tiers 2–3) and `vendor.contact_phone`.** The column's only consumers are those tiers. Adding it now is a column nothing reads. Worse, "verified" would be a lie while identity's dev OTP bypass accepts `123456` for any number — the verification has to mean something before a channel depends on it.
- **Gating anything on acceptance.** Nothing today refuses to dispatch a courier because a store has not accepted. Turning that on is a separate decision with its own blast radius, and it needs this ladder to exist first.

---

## The one design constraint that shapes this plan

**The ladder must never auto-reject a leg.**

Subsystem 1 added a guard in the collection consumer: a leg that is not awaiting collection is not credited. That guard is correct and it is what stops a store being paid for an order it refused.

It also means auto-rejecting an unanswered leg would **stiff a store that cooked the food and simply never tapped Accept** — the courier collects, the guard sees `Rejected`, and the vendor is not paid. A tablet on bad Wi-Fi at a lunch rush is exactly when this happens, and it is the worst possible time to silently not pay someone.

So the terminal rung is a human, not a state change. An unanswered leg stays `pending` — which still blocks the order from advancing, because `blocks_collection()` includes `Pending` — and ops is told. A person decides whether that kitchen is cooking or closed.

---

## File Structure

| File | Responsibility |
|---|---|
| `services/omnideliv/src/application/services/leg_recovery.rs` | Create: the decision function and the sweep |
| `services/omnideliv/src/application/services/mod.rs` | Modify: export it |
| `services/omnideliv/src/domain/repositories/mod.rs` | Modify: `find_awaiting_acceptance` on `VendorLegRepository` |
| `services/omnideliv/src/infrastructure/db/leg_repo.rs` | Modify: implement it |
| `services/omnideliv/src/bootstrap.rs` | Modify: spawn the sweep timer |
| `apps/merchant-portal/src/lib/api/vendor-orders.ts` | Create: typed client for the queue and the four actions |
| `apps/merchant-portal/src/app/(dashboard)/storefront/orders/page.tsx` | Create: the console screen |
| `apps/merchant-portal/src/components/storefront/order-queue.tsx` | Create: the queue list and its actions |
| `apps/merchant-portal/src/components/storefront/new-order-alarm.tsx` | Create: the audible alarm and its acknowledgement |

The alarm is its own component because it owns a side effect — audio — with a lifecycle that has nothing to do with rendering a list, and mixing the two is how the sound ends up firing on every re-render.

---

## Task 1: The decision function

**Files:**
- Create: `services/omnideliv/src/application/services/leg_recovery.rs`
- Test: same file, `#[cfg(test)]`

- [ ] **Step 1: Write the failing test**

Create `services/omnideliv/src/application/services/leg_recovery.rs` with only the test module and the enum stub, then let it fail:

```rust
//! What to do about a leg no store ever answered.

use chrono::{DateTime, Duration, Utc};

/// Below this a store simply may not have looked yet. A kitchen at a lunch
/// rush does not check a screen the second it chimes.
const GRACE_MINUTES: i64 = 2;
/// Past this, re-alerting has stopped being useful and a person is needed.
const ESCALATE_MINUTES: i64 = 8;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum LegRecovery {
    /// Answered, or not waiting on an answer.
    None,
    /// Still fresh — leave it alone.
    Wait,
    /// Old enough that the first alert plausibly missed. Send it again.
    Realert,
    /// Out of time. Tell a human. Deliberately NOT a state change: see the
    /// module docs on why auto-rejecting would stiff a store that cooked.
    Escalate,
}

pub fn decide(
    answered: bool,
    created_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> LegRecovery {
    if answered {
        return LegRecovery::None;
    }
    let age = now - created_at;
    if age < Duration::minutes(GRACE_MINUTES) {
        LegRecovery::Wait
    } else if age < Duration::minutes(ESCALATE_MINUTES) {
        LegRecovery::Realert
    } else {
        LegRecovery::Escalate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(mins: i64) -> (DateTime<Utc>, DateTime<Utc>) {
        let created = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        (created, created + Duration::minutes(mins))
    }

    #[test]
    fn an_answered_leg_needs_nothing_however_old_it_is() {
        let (c, n) = at(600);
        assert_eq!(decide(true, c, n), LegRecovery::None);
    }

    #[test]
    fn a_fresh_leg_is_left_alone() {
        let (c, n) = at(1);
        assert_eq!(decide(false, c, n), LegRecovery::Wait);
    }

    #[test]
    fn the_boundaries_are_exact() {
        // Written as exact boundaries because an off-by-one here is a store
        // alerted twice in a minute, or never alerted at all.
        let (c, n) = at(GRACE_MINUTES);
        assert_eq!(decide(false, c, n), LegRecovery::Realert, "grace is exclusive");

        let (c, n) = at(GRACE_MINUTES - 1);
        assert_eq!(decide(false, c, n), LegRecovery::Wait);

        let (c, n) = at(ESCALATE_MINUTES);
        assert_eq!(decide(false, c, n), LegRecovery::Escalate, "escalate is exclusive");

        let (c, n) = at(ESCALATE_MINUTES - 1);
        assert_eq!(decide(false, c, n), LegRecovery::Realert);
    }

    #[test]
    fn a_clock_that_went_backwards_does_not_escalate() {
        // NTP correction, or a row written by a host running fast. A negative
        // age must read as fresh, not as ancient.
        let created = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let now = created - Duration::minutes(30);
        assert_eq!(decide(false, created, now), LegRecovery::Wait);
    }
}
```

- [ ] **Step 2: Export the module**

In `services/omnideliv/src/application/services/mod.rs`, add alongside the existing declarations:

```rust
pub mod leg_recovery;
```

- [ ] **Step 3: Run the tests**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv leg_recovery
```

Expected: `test result: ok. 4 passed`. If `a_clock_that_went_backwards_does_not_escalate` fails, the comparison is treating a negative `Duration` as large — fix `decide`, not the test.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/src/application/services/
git commit -m "feat(omnideliv): decide what an unanswered vendor leg needs"
```

---

## Task 2: The sweep query

**Files:**
- Modify: `services/omnideliv/src/domain/repositories/mod.rs`
- Modify: `services/omnideliv/src/infrastructure/db/leg_repo.rs`

- [ ] **Step 1: Add the row type and the method to the trait**

In `services/omnideliv/src/domain/repositories/mod.rs`, next to `VendorLegRow`:

```rust
/// A leg the sweep is considering. Carries what an alert needs and nothing
/// else — the sweep runs across every tenant and must not become proportional
/// to basket size.
#[derive(Debug, Clone)]
pub struct AwaitingLeg {
    pub leg_id:               Uuid,
    pub order_id:             Uuid,
    pub tenant_id:            Uuid,
    pub vendor_id:            Uuid,
    pub goods_subtotal_cents: i64,
    pub created_at:           chrono::DateTime<chrono::Utc>,
}
```

Add to `VendorLegRepository`:

```rust
    /// Legs still waiting for their store to answer, oldest first.
    ///
    /// Deliberately across all tenants: an unanswered order is an operator
    /// concern, not a customer request, and scoping it per tenant would mean
    /// the sweep only runs for tenants someone remembered to enumerate. This
    /// is the same reasoning as `OrderRepository::find_awaiting_courier`.
    async fn find_awaiting_acceptance(&self) -> anyhow::Result<Vec<AwaitingLeg>>;
```

- [ ] **Step 2: Implement it**

In `services/omnideliv/src/infrastructure/db/leg_repo.rs`, add to the impl block. Import `AwaitingLeg` alongside the others.

```rust
    async fn find_awaiting_acceptance(&self) -> anyhow::Result<Vec<AwaitingLeg>> {
        // Bounded like find_awaiting_courier: a sweep that returns everything
        // turns one bad hour into an unbounded query. Oldest first, so the
        // legs nearest escalation are handled even when the cap bites.
        let rows = sqlx::query(
            r#"
            SELECT id, order_id, tenant_id, vendor_id, goods_subtotal_cents, created_at
              FROM omnideliv.order_vendor_legs
             WHERE status = 'pending'
             ORDER BY created_at ASC
             LIMIT 500
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| AwaitingLeg {
                leg_id:               r.get("id"),
                order_id:             r.get("order_id"),
                tenant_id:            r.get("tenant_id"),
                vendor_id:            r.get("vendor_id"),
                goods_subtotal_cents: r.get("goods_subtotal_cents"),
                created_at:           r.get("created_at"),
            })
            .collect())
    }
```

- [ ] **Step 3: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/src/domain/repositories/mod.rs services/omnideliv/src/infrastructure/db/leg_repo.rs
git commit -m "feat(omnideliv): find vendor legs still awaiting an answer"
```

---

## Task 3: The sweep

**Files:**
- Modify: `services/omnideliv/src/application/services/leg_recovery.rs`

- [ ] **Step 1: Add the service below the decision function**

```rust
use std::sync::Arc;

use crate::domain::entities::telemetry::event_type;
use crate::domain::entities::TelemetryEvent;
use crate::domain::repositories::{TelemetryRepository, VendorLegRepository};
use crate::infrastructure::messaging::{LegRef, VendorLegEvents};

/// The periodic sweep over legs nobody answered.
///
/// Separate from the consumer for the same reason `RecoveryService` is: a leg
/// nobody answered is defined by an event that never arrived, and nothing
/// event-driven can notice an absence. Only a timer can.
pub struct LegRecoveryService {
    legs:      Arc<dyn VendorLegRepository>,
    events:    Arc<dyn VendorLegEvents>,
    telemetry: Arc<dyn TelemetryRepository>,
}

impl LegRecoveryService {
    pub fn new(
        legs: Arc<dyn VendorLegRepository>,
        events: Arc<dyn VendorLegEvents>,
        telemetry: Arc<dyn TelemetryRepository>,
    ) -> Self {
        Self { legs, events, telemetry }
    }

    /// One pass. Returns how many legs were escalated, so the caller logs a
    /// number that means something rather than "sweep ran".
    pub async fn sweep(&self) -> anyhow::Result<usize> {
        let now = chrono::Utc::now();
        let waiting = self.legs.find_awaiting_acceptance().await?;
        let mut escalated = 0;

        for leg in waiting {
            // Everything this returns is `pending` by construction, so the leg
            // has not answered. `decide` still takes the flag rather than
            // assuming it, because the query is not the only future caller.
            match decide(false, leg.created_at, now) {
                LegRecovery::None | LegRecovery::Wait => {}

                LegRecovery::Realert => {
                    // Republishing the same event the checkout published. A
                    // transport that missed the first one gets another chance;
                    // one that delivered it will deliver a duplicate, which for
                    // a store is a second chime about an order it has not
                    // answered — the correct behaviour, not a defect.
                    let r = LegRef {
                        tenant_id:            leg.tenant_id,
                        vendor_id:            leg.vendor_id,
                        order_id:             leg.order_id,
                        leg_id:               leg.leg_id,
                        goods_subtotal_cents: leg.goods_subtotal_cents,
                        status:               crate::domain::entities::LegStatus::Pending,
                    };
                    if let Err(e) = self.events.leg_received(&r).await {
                        tracing::warn!(err = %e, leg_id = %leg.leg_id,
                            "re-alert publish failed");
                    }
                }

                LegRecovery::Escalate => {
                    escalated += 1;
                    // Loud, and with the vendor named: the ops question is
                    // always "is that kitchen open", and an alert that does not
                    // say which kitchen cannot be acted on.
                    tracing::error!(
                        leg_id = %leg.leg_id, order_id = %leg.order_id,
                        vendor_id = %leg.vendor_id, tenant_id = %leg.tenant_id,
                        age_minutes = (now - leg.created_at).num_minutes(),
                        "vendor has not answered this order — needs a human",
                    );

                    // The leg is deliberately NOT rejected here. The collection
                    // consumer refuses to credit a leg that is not awaiting
                    // collection, so auto-rejecting would stop a store being
                    // paid for food it actually cooked and simply forgot to
                    // accept on the tablet. Leaving it `pending` also keeps the
                    // order from advancing, because `blocks_collection`
                    // includes `Pending`.
                    let e = TelemetryEvent::new(
                        leg.tenant_id, leg.order_id, event_type::VENDOR_UNANSWERED,
                        None, None,
                        serde_json::json!({
                            "leg_id":       leg.leg_id,
                            "vendor_id":    leg.vendor_id,
                            "age_minutes":  (now - leg.created_at).num_minutes(),
                        }),
                    );
                    if let Err(err) = self.telemetry.append(&e).await {
                        tracing::error!(err = %err, leg_id = %leg.leg_id,
                            "unanswered-leg telemetry failed");
                    }
                }
            }
        }

        Ok(escalated)
    }
}
```

- [ ] **Step 2: Add the telemetry event type**

Find the `event_type` constants in `services/omnideliv/src/domain/entities/telemetry.rs` and add one alongside the existing ones, matching their naming:

```rust
    pub const VENDOR_UNANSWERED: &str = "vendor_unanswered";
```

- [ ] **Step 3: Verify**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv
```

Expected: the whole suite passes.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/src/
git commit -m "feat(omnideliv): sweep for vendor legs nobody answered"
```

---

## Task 4: Wire the timer

**Files:**
- Modify: `services/omnideliv/src/bootstrap.rs`

- [ ] **Step 1: Spawn it next to the existing recovery sweep**

Find the block that spawns `RecoveryService::sweep` (search for `recovery sweep escalated`). Add an equivalent below it, reusing the `legs`, `vendor_events` and `telemetry` Arcs — clone them before they are moved into `AppState`:

```rust
    // Legs nobody answered. A separate sweep from the stuck-order one above
    // because it asks a different question on a different clock: that one is
    // "did a courier take this", this one is "did the store even look".
    let leg_recovery = Arc::new(
        crate::application::services::leg_recovery::LegRecoveryService::new(
            legs_for_recovery, vendor_events_for_recovery, telemetry.clone(),
        ),
    );
    tokio::spawn(async move {
        // Same 60s cadence and the same skip-the-immediate-first-tick shape as
        // the stuck-order sweep. A sweep that fires at boot would alert on
        // every open leg every time a pod restarts.
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        tick.tick().await;
        loop {
            tick.tick().await;
            match leg_recovery.sweep().await {
                Ok(0) => {}
                Ok(n) => tracing::warn!(escalated = n, "vendor legs still unanswered"),
                Err(e) => tracing::error!(err = %e, "vendor leg sweep failed"),
            }
        }
    });
```

Add the two clones where `legs` and `vendor_events` are constructed:

```rust
    let legs_for_recovery = legs.clone();
    let vendor_events_for_recovery = vendor_events.clone();
```

- [ ] **Step 2: Verify it compiles**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: `Finished`. A "use of moved value" points at a clone taken after the move into `AppState` — take it before.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/src/bootstrap.rs
git commit -m "feat(omnideliv): run the unanswered-leg sweep on a timer"
```

---

## Task 5: The portal API client

**Files:**
- Create: `apps/merchant-portal/src/lib/api/vendor-orders.ts`

- [ ] **Step 1: Write it**

Follow the shape of `apps/merchant-portal/src/lib/api/storefront.ts` — `authFetch`, `API_BASE`, exported types, a flat exported object of calls.

```ts
/**
 * OmniDeliv vendor order queue — merchant-portal client.
 *
 * The queue endpoint is the record. Every alert this console raises is a hint
 * that something is on it; a missed alert costs a poll interval and never an
 * order. That is why this polls unconditionally rather than only refreshing
 * when told to.
 */
import { authFetch } from "@/lib/auth/auth-fetch";
import { API_BASE } from "@/lib/api/endpoints";

/** Mirrors `LegStatus` in services/omnideliv. Only the live ones reach here. */
export type LegStatus = "pending" | "accepted" | "preparing" | "ready";

export interface VendorLegRow {
  leg_id: string;
  order_id: string;
  status: LegStatus;
  goods_subtotal_cents: number;
  ready_in_minutes: number | null;
  accepted_at: string | null;
  created_at: string;
}

export interface TransitionResponse {
  leg_id: string;
  status: string;
  /** False when the leg was already in that state — a retry, or a colleague. */
  changed: boolean;
}

async function post(path: string, body?: unknown): Promise<TransitionResponse> {
  const res = await authFetch(`${API_BASE}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      // One key per attempt, so a retry of THIS submission replays rather than
      // acting twice. A new tap is a new action and gets a new key.
      "X-Idempotency-Key": crypto.randomUUID(),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) {
    // 409 is the one a human needs explained: somebody else moved this leg.
    if (res.status === 409) {
      throw new Error("This order was already updated somewhere else. Refreshing.");
    }
    throw new Error(await res.text().catch(() => "Request failed"));
  }
  return res.json();
}

export const vendorOrdersApi = {
  async queue(): Promise<VendorLegRow[]> {
    const res = await authFetch(`${API_BASE}/v1/omnideliv/vendors/me/orders`);
    if (res.status === 404) return []; // Signed in, but runs no store.
    if (!res.ok) throw new Error("Could not load the order queue");
    return res.json();
  },

  accept: (legId: string, readyInMinutes: number) =>
    post(`/v1/omnideliv/vendors/me/legs/${legId}/accept`, {
      ready_in_minutes: readyInMinutes,
    }),

  reject: (legId: string, reason: string) =>
    post(`/v1/omnideliv/vendors/me/legs/${legId}/reject`, { reason }),

  ready: (legId: string) => post(`/v1/omnideliv/vendors/me/legs/${legId}/ready`),

  served: (legId: string) => post(`/v1/omnideliv/vendors/me/legs/${legId}/served`),
};
```

- [ ] **Step 2: Typecheck**

```bash
cd apps/merchant-portal && npx tsc --noEmit
```

Expected: no errors. Pre-existing errors elsewhere in the app are not yours — confirm any error names a file you touched before acting on it.

- [ ] **Step 3: Commit**

```bash
git add apps/merchant-portal/src/lib/api/vendor-orders.ts
git commit -m "feat(merchant-portal): client for the vendor order queue"
```

---

## Task 6: The alarm

**Files:**
- Create: `apps/merchant-portal/src/components/storefront/new-order-alarm.tsx`

A kitchen does not watch a screen. The alarm repeats until a person acknowledges it, and it is its own component because it owns an effect whose lifecycle has nothing to do with rendering a list.

- [ ] **Step 1: Write it**

```tsx
"use client";
/**
 * Repeating audible alert for unanswered orders.
 *
 * Synthesised with WebAudio rather than shipped as an asset: no file to 404,
 * no bundle weight, and it cannot be silenced by a blocked CDN.
 *
 * It repeats until acknowledged on purpose. A single chime is missed over an
 * extractor fan, and a store that misses the chime is a customer waiting on
 * food nobody started.
 */
import { useEffect, useRef } from "react";

/** How often the alert repeats while at least one order is unanswered. */
const REPEAT_MS = 15_000;

export function NewOrderAlarm({ active }: { active: boolean }) {
  const ctxRef = useRef<AudioContext | null>(null);

  useEffect(() => {
    if (!active) return;

    const beep = () => {
      try {
        // Created lazily and reused: browsers block an AudioContext built
        // before a user gesture, and constructing one per beep leaks them.
        const Ctor =
          window.AudioContext ??
          (window as unknown as { webkitAudioContext?: typeof AudioContext })
            .webkitAudioContext;
        if (!Ctor) return;
        const ctx = (ctxRef.current ??= new Ctor());
        if (ctx.state === "suspended") void ctx.resume();

        const osc = ctx.createOscillator();
        const gain = ctx.createGain();
        osc.type = "sine";
        osc.frequency.value = 880;
        // Ramp rather than a hard stop: an abrupt gate clicks audibly.
        gain.gain.setValueAtTime(0.0001, ctx.currentTime);
        gain.gain.exponentialRampToValueAtTime(0.25, ctx.currentTime + 0.02);
        gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + 0.45);
        osc.connect(gain).connect(ctx.destination);
        osc.start();
        osc.stop(ctx.currentTime + 0.5);
      } catch {
        // Audio is an enhancement. A browser that refuses it must not take the
        // queue down with it — the list on screen is still the record.
      }
    };

    beep();
    const id = setInterval(beep, REPEAT_MS);
    return () => clearInterval(id);
  }, [active]);

  useEffect(
    () => () => {
      void ctxRef.current?.close();
    },
    [],
  );

  return null;
}
```

- [ ] **Step 2: Typecheck and commit**

```bash
cd apps/merchant-portal && npx tsc --noEmit
```

```bash
git add apps/merchant-portal/src/components/storefront/new-order-alarm.tsx
git commit -m "feat(merchant-portal): repeating alert for unanswered orders"
```

---

## Task 7: The console screen

**Files:**
- Create: `apps/merchant-portal/src/components/storefront/order-queue.tsx`
- Create: `apps/merchant-portal/src/app/(dashboard)/storefront/orders/page.tsx`

- [ ] **Step 1: Write the queue component**

Match the design language of `apps/merchant-portal/src/app/(dashboard)/storefront/page.tsx` — `GlassCard`, `variants` from `@/lib/design-system/tokens`, lucide icons, Framer Motion. Read that file first and follow what it does rather than inventing a second style.

Requirements the component must meet:

- Renders each leg as a card: order reference (short form of `order_id`), subtotal in currency, age in minutes, and its status.
- **Pending legs sort first and are visually loudest.** They are the only ones costing a customer time.
- Actions by status: `pending` → Accept (with a ready-in-minutes choice) and Reject (with a reason); `accepted` / `preparing` → Ready; `ready` → Served.
- Accept offers preset minute values (10, 15, 20, 30, 45) rather than a free-text field. A kitchen taps; it does not type.
- Reject requires a reason, chosen from presets (`Out of stock`, `Closing`, `Too busy`, `Cannot fulfil`) — the substitution path reads this, and a free-text box at a lunch rush yields "asdf".
- Every action disables its own button while in flight, and re-fetches the queue afterwards.
- A `409` refetches and tells the user someone else already moved it, rather than showing a raw error.

- [ ] **Step 2: Write the page**

```tsx
"use client";
/**
 * OmniDeliv vendor order console.
 *
 * Tier 0 of the notification design in ADR-0017, and the tier that actually
 * works in a kitchen: the screen is already open on the counter, so a poll and
 * a sound beat a push notification nobody has enabled.
 *
 * Polls unconditionally. The queue endpoint is the record and every other
 * channel is a hint, so this must not depend on having received one.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { OrderQueue } from "@/components/storefront/order-queue";
import { NewOrderAlarm } from "@/components/storefront/new-order-alarm";
import { vendorOrdersApi, type VendorLegRow } from "@/lib/api/vendor-orders";

/** Fast enough that a customer is not waiting on a screen refresh. */
const POLL_MS = 10_000;

export default function VendorOrdersPage() {
  const [legs, setLegs] = useState<VendorLegRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [muted, setMuted] = useState(false);
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    // A slow response must not stack requests behind it; the next tick is
    // never far away.
    if (inFlight.current) return;
    inFlight.current = true;
    try {
      setLegs(await vendorOrdersApi.queue());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load the order queue");
    } finally {
      inFlight.current = false;
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const unanswered = legs.filter((l) => l.status === "pending").length;

  // Re-arms itself: muting silences the orders on screen now, and a NEW
  // unanswered order starts the alarm again. A permanent mute is how a store
  // stops hearing about orders entirely.
  const mutedCount = useRef(0);
  useEffect(() => {
    if (muted) mutedCount.current = unanswered;
    else mutedCount.current = 0;
  }, [muted, unanswered]);
  const alarmActive = unanswered > 0 && (!muted || unanswered > mutedCount.current);

  return (
    <div className="space-y-6">
      <NewOrderAlarm active={alarmActive} />
      <OrderQueue
        legs={legs}
        loaded={loaded}
        error={error}
        unanswered={unanswered}
        muted={muted}
        onToggleMute={() => setMuted((m) => !m)}
        onChanged={refresh}
      />
    </div>
  );
}
```

- [ ] **Step 3: Typecheck**

```bash
cd apps/merchant-portal && npx tsc --noEmit
```

- [ ] **Step 4: Add the nav entry**

The storefront nav is gated on `useHasStorefront`. Find where the existing Storefront item is registered and add an Orders entry beside it under the same gate — a parcel merchant must not see it.

- [ ] **Step 5: Commit**

```bash
git add apps/merchant-portal/src
git commit -m "feat(merchant-portal): vendor order console with a repeating alert"
```

---

## Definition of done

- [ ] A leg unanswered for 2 minutes is re-alerted; one unanswered for 8 minutes escalates with the vendor named
- [ ] The ladder never changes a leg's status — no auto-reject, verified by reading the sweep
- [ ] The sweep is bounded and runs across all tenants
- [ ] A vendor with a live order sees it on the console within one poll interval
- [ ] The alarm repeats until acknowledged, and re-arms for a genuinely new order
- [ ] Accept, reject, ready and served all work from the console, and a 409 is explained rather than dumped
- [ ] `cargo test -p logisticos-omnideliv` and `cargo clippy --all-targets` clean
- [ ] `npx tsc --noEmit` clean in `apps/merchant-portal`

## What this plan deliberately does not do

- **No push, WhatsApp or SMS.** Tiers 1–3 need FCM credentials that are not on the VPS and a verified `contact_phone` that cannot mean anything while the dev OTP bypass is open.
- **No gating on acceptance.** A courier is still offered regardless of whether a store answered. This ladder is the prerequisite for changing that, not the change.
- **No auto-cancel or refund.** The terminal rung is a human. Automating it needs the partial-capture work that is still blocked in `services/payments`.
