# Mobile Design Part 2 → Backend Wiring Plan (promotions, penalty policy, whole-home moving)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Link the part-2 delta handoff at
`D:\LogisticOS\Uber Freight mobile app design part2\design_handoff_promotions_penalty_home`
to the backend. It covers whole-home moving (A), prompt intent routing (B), the
penalty policy (C) and marketing & promotions (D). Phase 0 first removes the
obstructions that would make the rest unsafe to build.

**Architecture:** Phase 0 changes existing money paths. It amends open PR #161 so
accessorials carry `billed` and `paid`, and closes a live cross-tenant cancel that
auto-refunds. Cancellation becomes one path that always runs a server-side policy
and hands payments a retention and a remainder. Parts C2, B, A and D then land as
their own plans, in that order. Promotions gets a new service behind its own
gateway prefix. Customer credit is a non-cash entitlement in that service, not a
wallet.

**Tech Stack:** Rust/Axum/SQLx (order-intake, payments, dispatch, driver-ops,
engagement, ai-layer, new `promotions`), `libs/types` (`ChargeType`), `libs/auth`
(RBAC and the `loyalty_program` feature key), Kafka (`libs/events` plus the topic
scripts), React Native (customer-app), Kotlin/Compose (driver-app-android).

**Builds on:** `docs/superpowers/plans/2026-09-12-mobile-design-to-backend-wiring.md`
and PR [breakdisk/cm-app#161](https://github.com/breakdisk/cm-app/pull/161), which
is open, CI green, and not merged.

---

## Decisions taken before this plan was written (2026-09-14)

| Decision | Choice | Consequence |
|---|---|---|
| Accessorial `billed`/`paid` split | **Amend #161 before merge** | No migration or compat shim. The single-column shape never ships. |
| Cancellation shape | **One cancel path, always priced** | `can_cancel()` widens. Every cancel runs the policy resolver (0% when early). Tenant/owner scoping and the permission fix land in the same change. No route can cancel without pricing. |
| Promotions home | **New `promotions` service, `/v1/promotions/*`** | Avoids the `/v1/offers` collision with dispatch, as the gateway's own comment asks. The redemption ledger does not live in a messaging service. |
| Customer credit | **Non-cash entitlement in promotions** | The 2026-08-11 no-customer-wallet decision stands. Payments' merchant wallet is untouched. Credit is never withdrawable, never topped up, and applies only through the quote's discount stack. |

---

## What the verification found

The handoff is careful, and most of its "what exists today" claims hold. These are
the ones that don't, or that matter more than the handoff says. Every claim
below was checked against the code on 2026-09-14.

### Finding 1: cancel is cross-tenant and auto-refunds (P0, security, live)

`POST /v1/shipments/:id/cancel` → `cancel_shipment`
([order-intake/src/api/http/mod.rs:164](../../../services/order-intake/src/api/http/mod.rs)):

1. **The customer cannot cancel at all.** The handler calls
   `require_permission(SHIPMENT_UPDATE)`. The `customer` role holds
   `SHIPMENT_CREATE`, `SHIPMENT_READ` and `SHIPMENT_CANCEL`, not `SHIPMENT_UPDATE`
   ([libs/auth/src/rbac.rs](../../../libs/auth/src/rbac.rs)). The customer app's
   `cancelShipment` (`apps/customer-app/src/services/api/shipments.ts:106`)
   therefore 403s today.
2. **The lookup has no tenant or owner filter.** `ShipmentService::cancel` calls
   `repo.find_by_id(&id)`, which runs
   `SELECT … FROM order_intake.shipments WHERE id = $1`
   ([infrastructure/db/mod.rs:341](../../../services/order-intake/src/infrastructure/db/mod.rs)).
   Migration 0001 enables and forces RLS on `tenant_id = current_setting('app.tenant_id')`,
   but **nothing in order-intake or any shared lib ever sets `app.tenant_id`**.
   A forced policy with no setting would error on every query, and the service
   works, so the database role must be bypassing RLS. In effect RLS is not
   enforcing anything here. Any caller holding `SHIPMENT_UPDATE` in tenant A can
   cancel tenant B's shipment by UUID.
3. **The event carries a nil tenant.** `Event::new(..., uuid::Uuid::nil(), ...)`.
4. **Payments then refunds it, in full, automatically.**
   `shipment_cancelled_consumer` ([payments/src/infrastructure/messaging/shipment_cancelled_consumer.rs](../../../services/payments/src/infrastructure/messaging/shipment_cancelled_consumer.rs))
   looks up `find_captured_by_reference("shipping_fee", "shipment", shipment_id)`
   and calls `intent_service.refund(intent.id)`. That is a **full** refund.

So a cross-tenant cancel reverses another tenant's customer payment. The fix
lands in Phase 0 Task 2. It does not depend on anything else in this plan and
**can ship on its own ahead of the rest**.

### Finding 2: the refund consumer would erase every retention fee (P0, money)

The same consumer refunds 100% of the captured intent on every `shipment.cancelled`.
The penalty policy's 15% late fee and 100% move-day retention would be refunded
anyway, unless the consumer learns to refund only the remainder.
`PaymentIntentService::refund(intent_id)` takes no amount. The gateway adapter does:
`network_international.rs:259` has
`async fn refund(&self, gateway_payment_ref: &str, amount_cents: i64)`, so partial
refunds need no new provider work.

**Deploy-order trap:** the payments change must be live **before** order-intake
emits a non-zero retention. An old consumer that receives a retention event
refunds everything.

### Finding 3: no shipment has a scheduled time (blocks C1)

`Shipment` ([domain/entities/shipment.rs](../../../services/order-intake/src/domain/entities/shipment.rs))
has `created_at` and `updated_at` and nothing else time-shaped. The cancel tier
is "hours to pickup", and there is no pickup time to measure against.
`RescheduleShipmentCommand` carries `preferred_date` and `preferred_time_slot`,
but order-intake's `reschedule` does not store them (it resets status to
`Confirmed`). Only delivery-experience's repository keeps a `preferred_date`.
Part A's move slot is the natural source, and Phase 0 Task 4 adds the column.

### Finding 4: `rating_avg` is never written (blocks C2's rating penalty)

`drivers.rating_avg` / `rating_count` were added in driver-ops migration 0011
with the comment "populated by the phase-2 feedback ingestion pipeline". A search
of every service and migration finds **no writer**. The column is always NULL.
The drop sheet's "rating −0.4" has no number to adjust, so C2 has to build the
ingestion pipeline or state the rating as unrated.

### Finding 5: `/v1/offers` is already taken (Part D as specified would 404)

The gateway routes `path.starts_with("/v1/offers")` → **dispatch**
(`api-gateway/src/proxy/mod.rs:83`), which serves `/offers/open`,
`/offers/:id/claim|pass|seen` for the driver load board. Part D's
`GET /v1/offers` and `POST /v1/offers/{code}/validate` would reach dispatch and
404. The handoff repeats part 1's error of attributing `/v1/offers` to
driver-ops. Resolved by decision: promotions lives at `/v1/promotions/*`.

### Finding 6: the campaign inbox is half-built, but not customer-reachable

- `engagement.campaign_sends` already has per-customer rows with `read_at`.
- `GET /v1/customers/:customer_id/sends` exists, but requires `ENGAGEMENT_READ`
  (tenant-wide) and trusts the path id. Customers hold only `ENGAGEMENT_READ_OWN`,
  so it 403s for them, and granting the wide permission would expose every
  customer's sends.
- `read_at` is written only by the provider receipt webhook. There is no
  customer mark-read route, so "Mark all read" has nowhere to go.
- `GET /v1/notifications` is already self-scoped from the token and used by the
  customer app (`notifications.ts:65`). That is the pattern to copy.

So the inbox needs a self-pinned `GET /v1/customers/me/sends` plus a mark-read
route, not a new notifications service. The gateway's existing
`/v1/customers/…/sends` → engagement rule already matches `/v1/customers/me/sends`.

### Finding 7: PR #161 ships the shape part 2 forbids

The part-2 consumer design's `ACCESSORIALS` object is now
`helper {billed:14, paid:11, unit:'flight'}`, `assembly {26,20}`,
`haulaway {39,26}`, `carbon {4,0}`. The Wiring Map says to split before any
promotion ships, because "a tier discount against a single-column rate card
silently docks the driver". #161's `AccessorialRate { amount_cents }` is that
single column, and `Accessorial Config.dc.html` was removed from this handoff.
#161's billed amounts still match the design (14 USD × 58 = 812 PHP = `81200`
cents). Only `paid` is new.

### Confirmed accurate

| Handoff claim | Verified |
|---|---|
| Cancel is a status flip: `{reason}` → 204, publishes `shipment.cancelled`, no money | Yes (plus Findings 1–2) |
| `can_cancel()` allows only `Pending`/`Confirmed` | Yes, `shipment.rs:102` |
| `DECLINE_BAN_THRESHOLD = 20` → `is_active=false`, offline | Yes, `driver-ops/.../assignment_rejected_consumer.rs:23` |
| `GET /v1/drivers/me` returns `offers_seen`, `offers_claimed`, `decline_count`, `rating_avg`, `rating_count` | Yes, `driver-ops/src/api/http/drivers.rs:218-226` (but see Finding 4) |
| No promotions, loyalty, referral, corporate, crew, survey, grace, waiting-fee, no-show or policy-version code exists | Yes. The only "promo" hits are a merchant-wallet doc comment and tenant "promote draft→active". |
| `/v1/agents/chat` returns no intent | Yes: `{session_id, reply, escalated, status}` (`ai-layer/src/api/http/mod.rs:198`) |

### Reuse, don't rebuild

- **Partial refund** is already supported by the NI adapter (`refund(ref, amount_cents)`).
  The refund-obligation durability (`mark_refund_requested` plus
  `sweep_pending_refunds`) already exists. Extend it; don't write a second refund path.
- **Invoice lines** already have `discount: Option<Money>` per line and
  `ManualAdjustment` with required reason
  ([payments/src/domain/entities/invoice.rs:89](../../../services/payments/src/domain/entities/invoice.rs)).
  `ChargeType` lives in `libs/types/src/invoice.rs:284` and already has
  `RescheduleFee`, `FailedDeliveryFee`, `StorageFee`. New variants go there, and
  every `match` over it across services must be updated in the same change.
- **Tenant gating** for loyalty already exists as a pricing-matrix key.
  `Claims::has_feature("loyalty_program")` falls back to business/enterprise tiers
  (`libs/auth/src/claims.rs:210`). It has no consumers, the same state
  `enterprise_mcp` was in before the remote MCP server.
- **Crew firm** is a carrier: `drivers.carrier_id: Option<Uuid>` exists
  (`driver-ops/src/domain/entities/driver.rs:26`). **Crew size and multiple drivers
  per job do not**: a task has one `driver_id`.
- **Dispatch assignments** have `assigned_at`, `accepted_at`, `rejected_at`
  (`dispatch/src/domain/entities/driver_assignment.rs:15-17`). Driver-ops tasks
  have `started_at`. There is no `arrived_at` and no `grace_expires_at`.
- **A new service** registers in `Cargo.toml` (workspace), `docker-compose.yml`,
  `.github/workflows/build-images.yml` (path filter + env), `ci-rust.yml`, and
  `api-gateway/src/config.rs` as an `Option<String>` URL, exactly like
  `field_ops_url`. The prefix is resolved **before** the flat `/v1` chain.
- **New Kafka topics** go in `libs/events/src/topics.rs` **and**
  `scripts/create-kafka-topics.sh` / `scripts/kafka/create-topics.sh`, checked by
  `scripts/check-kafka-topics.sh`. Per memory, the guard has been wrong three times.
  Pre-create on the live broker, and verify with `--members --verbose`, not `--describe`.

### Inconsistencies inside the handoff

| Where | Conflict | Treat as |
|---|---|---|
| Promo window | README and design code say **11th–24th** (`WINDOW = {from: 11, to: 24}`). The Wiring Map says **11th–19th**. | 11–24 (two sources to one), confirm |
| Penalty numbers | `Penalty Policy.dc.html` leaves six decisions **open** (late fee 10–20%, grace 30–60 min, dispatch penalty form, decline threshold, survey treatment, no-show reinstatement). The README picks 15%, 45 min, 20%, 85%. | Placeholders. The README's own "Rates to confirm" says none came from a rate card. |
| 24–48 hour cancel band | "Free until 48 hours before" and "inside 24 hours 15%" leave 24–48h **unpriced**. The README calls it "deliberately unstated". | Must be decided. A resolver cannot leave a hole. |
| Screenshot 16 | Indexed as "Wallet". It is the **Payments** screen with a credit-balance panel, no withdraw and no top-up. | Payments + credit panel |
| Brand | The penalty sheet header says "Haulline AI". Part D says "Haulline's margin". | Confirm whether Haulline is a tenant/white-label name |
| `/v1/offers` service | Attributed to driver-ops again | It is dispatch (Finding 5) |

---

## Scope

Five plans, in order:

1. **This document, Phase 0.** Written out step by step below. It changes existing
   money paths and must land first.
2. **Part C2: driver penalties.** Own plan.
3. **Part B: intent routing.** Own plan. Small, and it unblocks A's entry point.
4. **Part A: whole-home moving.** Own plan. The largest backend lift in order-intake.
5. **Part D: promotions service.** Own plan. A new service.

App UI for each part is a separate plan per app. Build no screen against an
endpoint that has not deployed and been grep-verified in the running binary.

---

## File structure — Phase 0

| File | Responsibility |
|---|---|
| Modify `services/order-intake/src/config.rs` | `AccessorialRate` gains `billed_cents` and `paid_cents`, plus a `validate()` that rejects paid > billed |
| Modify `services/order-intake/src/domain/value_objects/accessorials.rs` | `PricedAccessorial` carries both. `paid_cents` never serialises to a consumer. |
| Modify `services/order-intake/src/api/http/accessorials.rs` | `GET /v1/accessorials` shows billed only |
| Modify `services/order-intake/src/api/http/quote.rs` | Breakdown uses billed. The token signs `accessorial_paid_cents`. |
| Modify `services/order-intake/src/bootstrap.rs` | Validate the card at startup, and fail boot on a paying-more-than-billing card |
| Modify `services/order-intake/src/api/http/mod.rs:164` | Cancel accepts `SHIPMENT_CANCEL`, passes tenant and actor |
| Modify `services/order-intake/src/application/commands/mod.rs:174` | `CancelShipmentCommand` carries tenant and actor |
| Create `services/order-intake/src/domain/value_objects/cancel_authority.rs` | Pure tenant/owner check, shared by every by-id handler |
| Modify `services/order-intake/src/application/services/shipment_service.rs:724` | Scoped lookup, real tenant on the event |
| Create `services/payments/src/domain/value_objects/refund_decision.rs` | Pure: captured amount + event → full / partial / none |
| Modify `services/payments/src/application/services/payment_intent_service.rs:347` | `refund_amount(intent_id, amount_cents)` beside `refund` |
| Modify `services/payments/src/infrastructure/messaging/shipment_cancelled_consumer.rs` | Refund only the remainder |
| Create `services/order-intake/migrations/00NN_shipment_schedule_and_policy.sql` | `scheduled_pickup_at`, `cancellation_policy_version` |
| Create `services/order-intake/src/domain/value_objects/cancellation_policy.rs` | Pure tier + fee resolver |
| Modify `libs/types/src/invoice.rs:284` | `ChargeType::CancellationRetention` |

---

## Phase 0: remove the obstructions

### Task 1: Split accessorials into billed and paid, on PR #161's branch

Branch `claude/uber-freight-design-backend-900a06`. Auto-fix is on for #161, so a
CI failure after this push will wake the session.

**Files:** `services/order-intake/src/config.rs`,
`services/order-intake/src/domain/value_objects/accessorials.rs`,
`services/order-intake/src/api/http/accessorials.rs`,
`services/order-intake/src/api/http/quote.rs`,
`services/order-intake/src/domain/value_objects/quote_token.rs`,
`services/order-intake/src/bootstrap.rs`

- [ ] **Step 1: Write the failing tests**

Replace the `php_card()` fixture in `accessorials.rs` tests with the design's
current values (USD minor units) and add three tests:

```rust
fn usd_card() -> AccessorialsConfig {
    AccessorialsConfig {
        helper: Some(AccessorialRate {
            billed_cents: 1_400, paid_cents: 1_100, currency: "USD".into(),
            basis: AccessorialBasis::StairFlight, max_units: Some(8),
        }),
        assembly: Some(AccessorialRate {
            billed_cents: 2_600, paid_cents: 2_000, currency: "USD".into(),
            basis: AccessorialBasis::Booking, max_units: None,
        }),
        haulaway: Some(AccessorialRate {
            billed_cents: 3_900, paid_cents: 2_600, currency: "USD".into(),
            basis: AccessorialBasis::Booking, max_units: None,
        }),
        carbon_offset: Some(AccessorialRate {
            billed_cents: 400, paid_cents: 0, currency: "USD".into(),
            basis: AccessorialBasis::Booking, max_units: None,
        }),
    }
}

/// What the mover is paid is priced from its own column. A future discount
/// moves billed; nothing here may derive paid from billed.
#[test]
fn paid_is_priced_from_its_own_column() {
    let items = price_accessorials_itemised(
        &usd_card(), Currency::USD, &[req("helper", Some(2)), req("haulaway", None)],
    ).unwrap();
    assert_eq!(items.iter().map(|i| i.billed_cents).sum::<i64>(), 2_800 + 3_900);
    assert_eq!(items.iter().map(|i| i.paid_cents).sum::<i64>(),   2_200 + 2_600);
}

/// paid_cents is settlement data. It reveals the take rate, so it must never
/// reach the consumer app, in the quote breakdown or anywhere else.
#[test]
fn a_priced_accessorial_never_serialises_paid() {
    let items = price_accessorials_itemised(
        &usd_card(), Currency::USD, &[req("assembly", None)],
    ).unwrap();
    let json = serde_json::to_value(&items[0]).unwrap();
    assert!(json.get("paid_cents").is_none(), "settlement leaked: {json}");
    assert_eq!(json["amount_cents"], 2_600);
}

/// A card paying the mover more than the customer is billed loses money on
/// every job. That is a misconfiguration, and boot should refuse it.
#[test]
fn a_card_that_pays_more_than_it_bills_is_rejected() {
    let mut card = usd_card();
    card.assembly.as_mut().unwrap().paid_cents = 9_999;
    let err = card.validate().expect_err("paid > billed must not boot");
    assert!(err.contains("assembly"), "error must name the code: {err}");
}
```

Update the existing six tests' `amount` assertions to `billed_cents`.

- [ ] **Step 2: Run to verify they fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake accessorial 2>&1 | tail -15
```

Expected: FAIL, `no field billed_cents`.

- [ ] **Step 3: Implement the split in `config.rs`**

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct AccessorialRate {
    /// What the customer is billed, in minor units of the tenant's currency.
    /// Never converted. A promotion discounts this column and only this column.
    pub billed_cents: i64,
    /// What settlement pays the mover for performing it. Promotions never read
    /// or change this. A consumer discount comes out of platform margin, never
    /// out of the driver's fee.
    pub paid_cents: i64,
    pub currency: String,
    #[serde(default)] pub basis: AccessorialBasis,
    #[serde(default)] pub max_units: Option<u16>,
}

impl AccessorialsConfig {
    /// Refuse a card that pays more than it bills. Called at startup.
    pub fn validate(&self) -> Result<(), String> {
        let bad: Vec<&str> = self
            .offered()
            .into_iter()
            .filter(|(_, r)| r.paid_cents > r.billed_cents || r.paid_cents < 0 || r.billed_cents < 0)
            .map(|(code, _)| code)
            .collect();
        if bad.is_empty() {
            Ok(())
        } else {
            Err(format!("accessorial rate card pays more than it bills (or is negative) for: {}", bad.join(", ")))
        }
    }
}
```

The env vars become `ACCESSORIALS__HELPER__BILLED_CENTS` and
`ACCESSORIALS__HELPER__PAID_CENTS` (likewise for assembly, haulaway and carbon_offset).

- [ ] **Step 4: Carry both columns through pricing, keeping the wire name**

In `accessorials.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct PricedAccessorial {
    pub code: String,
    pub units: u16,
    /// Serialised as `amount_cents`, the name the design's response shape uses.
    #[serde(rename = "amount_cents")]
    pub billed_cents: i64,
    /// Settlement only. Never serialised.
    #[serde(skip_serializing)]
    pub paid_cents: i64,
}
```

In `price_accessorials_itemised`, set
`billed_cents: rate.billed_cents.saturating_mul(units as i64)` and
`paid_cents: rate.paid_cents.saturating_mul(units as i64)`. `price_accessorials`
sums `billed_cents`. In `api/http/accessorials.rs`, `amount_cents: rate.billed_cents`.

- [ ] **Step 5: Sign the paid total into the quote token**

`QuoteTokenPayload` gains
`#[serde(default)] pub accessorial_paid_cents: Option<i64>`, so settlement is fixed
at quote time and tamper-checked with the price. It stays **out** of `QuoteResponse`.
`quote.rs` computes `items.iter().map(|i| i.paid_cents).sum()`.

- [ ] **Step 6: Validate at boot**

In `bootstrap.rs`, before `ShipmentService::new`:

```rust
cfg.accessorials
    .validate()
    .map_err(|e| anyhow::anyhow!("{e} — refusing to start order-intake"))?;
```

- [ ] **Step 7: Run, clippy, commit, push**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake 2>&1 | grep -E '^test result|FAILED'
CARGO_INCREMENTAL=0 cargo clippy -p logisticos-order-intake --all-targets 2>&1 | grep -E '^(error|warning)(\[|:)' | grep -v redis
git add services/order-intake && git commit -F - <<'EOF'
feat(order-intake): split accessorial rates into billed and paid

Part 2 of the mobile design puts promotions on top of the accessorial card, and
a discount against a single amount column silently docks the driver. Each rate
now carries billed_cents (what the customer is charged, what promotions discount)
and paid_cents (what settlement pays, which promotions never read).

paid_cents never serialises to the consumer: it reveals the take rate. It is
signed into the quote token instead, so settlement is fixed at quote time.
A card that pays more than it bills is refused at boot.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
git push
```

Then update #161's description: the env-var names changed, so the VPS `.env`
needs `…__BILLED_CENTS` / `…__PAID_CENTS`, not `…__AMOUNT_CENTS`.

---

### Task 2: Scope cancel to tenant and owner, and let a customer reach it

Independent of Task 1. **Can ship alone as a security fix**, from its own branch
off `master`.

**Files:** `services/order-intake/src/domain/value_objects/cancel_authority.rs`
(create), `commands/mod.rs:174`, `api/http/mod.rs:164`, `shipment_service.rs:724`

- [ ] **Step 1: Verify who holds what, before encoding it**

```bash
awk '/"merchant" => vec!\[/,/\],/' libs/auth/src/rbac.rs
awk '/"dispatcher" => vec!\[/,/\],/' libs/auth/src/rbac.rs
grep -n 'merchant_id = claims.user_id' services/order-intake/src/api/http/mod.rs
```

Confirmed 2026-09-14: `create_shipment` sets `cmd.merchant_id = claims.user_id`
for every caller, so a customer-booked shipment's `merchant_id` is the customer's
user id. **If `merchant` holds `SHIPMENT_UPDATE`**, tenant-wide authority must
come from an explicit operator role check, not from `SHIPMENT_UPDATE`. Otherwise
one merchant can cancel another merchant's shipment in the same tenant. Encode
whichever the grep shows.

- [ ] **Step 2: Write the failing test**

Create `cancel_authority.rs`:

```rust
//! Who may act on a shipment by id. RLS does not do this: order-intake never
//! sets app.tenant_id, so the forced policy in migration 0001 is not what
//! scopes queries. This check is.

use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
pub struct Actor {
    pub tenant_id: Uuid,
    pub user_id:   Uuid,
    /// True for tenant operators, who may act on any shipment in their tenant.
    /// False for merchants and customers, who may act only on their own.
    pub tenant_wide: bool,
}

/// `false` must surface as 404, not 403, so a caller cannot probe which
/// shipment ids exist in other tenants.
pub fn may_act_on(actor: &Actor, shipment_tenant: Uuid, shipment_owner: Uuid) -> bool {
    if actor.tenant_id != shipment_tenant {
        return false;
    }
    actor.tenant_wide || actor.user_id == shipment_owner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> (Uuid, Uuid, Uuid) { (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()) }

    #[test]
    fn another_tenant_is_refused_even_for_an_operator() {
        let (t1, t2, u) = ids();
        let op = Actor { tenant_id: t1, user_id: u, tenant_wide: true };
        assert!(!may_act_on(&op, t2, u), "the live bug: cross-tenant cancel by uuid");
    }

    #[test]
    fn a_customer_may_cancel_only_their_own() {
        let (t, me, other) = ids();
        let customer = Actor { tenant_id: t, user_id: me, tenant_wide: false };
        assert!(may_act_on(&customer, t, me));
        assert!(!may_act_on(&customer, t, other));
    }

    #[test]
    fn an_operator_may_cancel_anything_in_their_tenant() {
        let (t, op, owner) = ids();
        let actor = Actor { tenant_id: t, user_id: op, tenant_wide: true };
        assert!(may_act_on(&actor, t, owner));
    }
}
```

Register it in `domain/value_objects/mod.rs`, run
`cargo test -p logisticos-order-intake cancel_authority`, and expect a FAIL to
compile until the module is declared, then PASS.

- [ ] **Step 3: Carry the actor on the command**

```rust
pub struct CancelShipmentCommand {
    #[serde(default)] pub shipment_id: uuid::Uuid,
    pub reason: String,
    /// Set by the handler from the token. Never from the body.
    #[serde(skip)] pub actor: Option<crate::domain::value_objects::cancel_authority::Actor>,
}
```

- [ ] **Step 4: Fix the handler**

```rust
async fn cancel_shipment(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(mut cmd): Json<CancelShipmentCommand>,
) -> impl IntoResponse {
    // Customers hold SHIPMENT_CANCEL, not SHIPMENT_UPDATE, so the old check made
    // the customer app's Cancel button a guaranteed 403.
    if !(claims.has_permission(permissions::SHIPMENT_CANCEL)
        || claims.has_permission(permissions::SHIPMENT_UPDATE))
    {
        return Err(AppError::Forbidden { resource: "shipment".into() });
    }
    cmd.shipment_id = id;
    cmd.actor = Some(Actor {
        tenant_id: claims.tenant_id,
        user_id: claims.user_id,
        tenant_wide: /* the rule Step 1's grep settled */ claims.has_permission(permissions::SHIPMENT_UPDATE),
    });
    s.svc.cancel(cmd).await?;
    Ok::<_, AppError>((StatusCode::NO_CONTENT, ()))
}
```

Match `AppError::Forbidden`'s real shape. It is `{ resource }` in ai-layer, so
confirm it in `libs/errors` before compiling.

- [ ] **Step 5: Scope the service and fix the event tenant**

In `ShipmentService::cancel`, directly after `find_by_id`:

```rust
let actor = cmd.actor.ok_or_else(|| AppError::Internal(anyhow::anyhow!(
    "cancel reached the service with no actor — the handler must set it",
)))?;
if !may_act_on(&actor, shipment.tenant_id.inner(), shipment.merchant_id.inner()) {
    // 404, not 403: do not confirm that the id exists in another tenant.
    return Err(AppError::NotFound { resource: "Shipment", id: cmd.shipment_id.to_string() });
}
```

Replace `uuid::Uuid::nil()` in `Event::new` with `shipment.tenant_id.inner()`.

- [ ] **Step 6: Apply the same check to every other by-id caller**

The defect is `find_by_id` without scope, not something specific to cancel.
Extend the fix to every place it appears:

```bash
grep -rn 'find_by_id(' services/order-intake/src/application services/order-intake/src/api | grep -v 'fn find_by_id'
```

Expect at least `get_shipment`, `reschedule`, `admin_override_status`, and the
billing reads. Each gets `may_act_on`. The mesh-internal `/v1/internal/*` callers
are exempt, since there is no user actor and Istio asserts the caller. Say so in a
comment at each one.

- [ ] **Step 7: Run the suite, commit**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake 2>&1 | grep -E '^test result|FAILED'
CARGO_INCREMENTAL=0 cargo clippy -p logisticos-order-intake --all-targets 2>&1 | grep -E '^(error|warning)(\[|:)' | grep -v redis
```

Commit message: `fix(order-intake): scope shipment-by-id actions to tenant and owner`.
State Findings 1.1–1.4 in the body, including that payments auto-refunded the
cross-tenant cancel.

---

### Task 3: Refund only the remainder

**Files:** `services/payments/src/domain/value_objects/refund_decision.rs` (create),
`payment_intent_service.rs:347`, `shipment_cancelled_consumer.rs`

**Deploy first.** This must be live before Task 4 lets order-intake emit a
retention.

- [ ] **Step 1: Write the failing test**

```rust
//! How much of a captured payment a cancellation returns.
//!
//! Backward compatible by construction: an event without `refunded_cents` came
//! from an order-intake predating the penalty policy, and those cancellations
//! were always full refunds.

#[derive(Debug, PartialEq, Eq)]
pub enum RefundDecision {
    Full,
    Partial(i64),
    /// Everything retained: record it, call no gateway.
    None,
}

pub fn refund_decision(captured_cents: i64, refunded_cents: Option<i64>) -> Result<RefundDecision, String> {
    match refunded_cents {
        None => Ok(RefundDecision::Full),
        Some(r) if r < 0 => Err(format!("negative refund {r}")),
        Some(r) if r > captured_cents => Err(format!(
            "refund {r} exceeds captured {captured_cents} — never refund more than was taken"
        )),
        Some(0) => Ok(RefundDecision::None),
        Some(r) if r == captured_cents => Ok(RefundDecision::Full),
        Some(r) => Ok(RefundDecision::Partial(r)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_from_before_the_policy_is_a_full_refund() {
        assert_eq!(refund_decision(21_400, None), Ok(RefundDecision::Full));
    }

    #[test]
    fn a_late_cancel_refunds_only_the_remainder() {
        // 15% of 21_400 retained = 3_210, refunded 18_190.
        assert_eq!(refund_decision(21_400, Some(18_190)), Ok(RefundDecision::Partial(18_190)));
    }

    #[test]
    fn a_move_day_cancel_calls_no_gateway() {
        assert_eq!(refund_decision(21_400, Some(0)), Ok(RefundDecision::None));
    }

    #[test]
    fn a_refund_larger_than_the_capture_is_refused() {
        assert!(refund_decision(21_400, Some(21_401)).is_err());
    }
}
```

- [ ] **Step 2: Verify the intent state machine before adding a state**

```bash
grep -rn -A12 'pub enum PaymentIntentStatus\|pub enum IntentStatus' services/payments/src/domain
```

A partial refund is not `Refunded`. If there is no `PartiallyRefunded`, add it to
the enum, the `claim_for_refund` transition and the sweep, or a retry after a
partial refund will try to refund again.

- [ ] **Step 3: Add `refund_amount` beside `refund`, with the same two guards**

`PaymentIntentService::refund_amount(intent_id, amount_cents)` mirrors `refund`:
domain status guard first, then `claim_for_refund`, then
`gateway.refund(ref, amount_cents)`. Do not duplicate the durability logic.
Factor `refund` as `refund_amount(id, intent.amount_cents)`.

- [ ] **Step 4: Use it in the consumer**

Parse `data.refunded_cents` as `Option<i64>` and call `refund_decision`.
`Full` → `refund`, `Partial(n)` → `refund_amount(n)`, `None` → log the retention
and return `Ok`. `mark_refund_requested` must record the **amount**, so
`sweep_pending_refunds` retries the partial, not a full refund. Check its schema,
and add a column if the obligation row has no amount.

- [ ] **Step 5: Test, commit**

`cargo test -p logisticos-payments`. Commit:
`fix(payments): refund only what a cancellation returns`.

---

### Task 4: One cancel path, always priced

Depends on Tasks 2 and 3. Rates are **placeholders** until the "Rates to confirm"
table is signed off, so every number here comes from config.

**Files:** `migrations/00NN_shipment_schedule_and_policy.sql` (create),
`domain/value_objects/cancellation_policy.rs` (create), `shipment.rs:102`,
`shipment_service.rs`, `api/http/mod.rs` (preview route), `libs/types/src/invoice.rs`

- [ ] **Step 1: Migration**

```sql
-- The cancellation tier is hours-to-pickup, and a shipment had no pickup time.
-- Set from the Part A move slot, and from reschedule, which today discards the
-- date it is sent.
ALTER TABLE order_intake.shipments
    ADD COLUMN IF NOT EXISTS scheduled_pickup_at         TIMESTAMPTZ,
    -- The policy the customer agreed to at booking. A cancellation is priced
    -- under this version, never the current one.
    ADD COLUMN IF NOT EXISTS cancellation_policy_version TEXT;
```

Per ADR-0012, confirm the per-schema `_sqlx_migrations` numbering before naming
the file. Per memory, a migration that cannot apply silently pins the service to
its last-good image.

- [ ] **Step 2: Write the failing resolver tests**

```rust
use chrono::{DateTime, Duration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelTier { Early, Late, SameDay }

/// Placeholder values. See "Rates to confirm".
#[derive(Debug, Clone)]
pub struct CancellationPolicy {
    pub version: String,
    pub free_until_hours: i64,   // 48
    pub late_from_hours: i64,    // 24
    /// What 24–48 h costs. The design leaves it unstated, and a resolver
    /// cannot. Must be set explicitly.
    pub between_bps: u32,
    pub late_bps: u32,           // 1500
    pub same_day_bps: u32,       // 10000
}

pub struct CancellationQuote {
    pub tier: CancelTier,
    pub fee_cents: i64,
    pub survey_applied_cents: i64,
    pub refund_cents: i64,
}

/// `booking_cents` excludes the survey fee. The survey is applied to the
/// retention first: survey_kept = min(fee, survey), and the rest is refunded.
pub fn quote_cancellation(
    p: &CancellationPolicy,
    scheduled_pickup_at: DateTime<Utc>,
    now: DateTime<Utc>,
    crew_dispatched: bool,
    booking_cents: i64,
    survey_cents: i64,
) -> CancellationQuote {
    // Minutes, not hours: num_hours() truncates, and 23h59m must not read as 23h.
    let minutes_out = (scheduled_pickup_at - now).num_minutes();
    let late_from = p.late_from_hours * 60;
    let free_until = p.free_until_hours * 60;

    let (tier, bps) = if crew_dispatched || minutes_out <= 0 {
        (CancelTier::SameDay, p.same_day_bps)
    } else if minutes_out < late_from {
        (CancelTier::Late, p.late_bps)
    } else if minutes_out < free_until {
        // The 24–48 h band. Priced explicitly from config because the design
        // leaves it unstated, and a hole here is an unpriced cancellation.
        (CancelTier::Late, p.between_bps)
    } else {
        (CancelTier::Early, 0)
    };

    // The fee is a share of the booking, rounded down in the customer's favour.
    // i128 so a large booking times 10_000 bps cannot overflow.
    let fee_cents = (booking_cents.max(0) as i128 * bps as i128 / 10_000) as i64;

    // The survey was paid up front and is applied to the fee first. Whatever is
    // left of everything paid comes back.
    let survey_applied_cents = fee_cents.min(survey_cents.max(0));
    let refund_cents = booking_cents.max(0) + survey_cents.max(0) - fee_cents;

    CancellationQuote { tier, fee_cents, survey_applied_cents, refund_cents }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> CancellationPolicy {
        CancellationPolicy { version: "2026-09".into(), free_until_hours: 48, late_from_hours: 24,
                             between_bps: 1500, late_bps: 1500, same_day_bps: 10_000 }
    }

    #[test]
    fn more_than_48_hours_out_is_free_and_refunds_the_survey() {
        let now = Utc::now();
        let q = quote_cancellation(&policy(), now + Duration::hours(60), now, false, 18_900, 4_500);
        assert_eq!(q.tier, CancelTier::Early);
        assert_eq!((q.fee_cents, q.survey_applied_cents, q.refund_cents), (0, 0, 18_900 + 4_500));
    }

    #[test]
    fn inside_24_hours_retains_15_percent_and_applies_the_survey_first() {
        let now = Utc::now();
        // 15% of 18_900 = 2_835. The survey (4_500) covers it in full.
        let q = quote_cancellation(&policy(), now + Duration::hours(19), now, false, 18_900, 4_500);
        assert_eq!(q.tier, CancelTier::Late);
        assert_eq!(q.fee_cents, 2_835);
        assert_eq!(q.survey_applied_cents, 2_835);
        assert_eq!(q.refund_cents, 18_900 + (4_500 - 2_835));
    }

    #[test]
    fn a_dispatched_crew_is_same_day_regardless_of_the_clock() {
        let now = Utc::now();
        let q = quote_cancellation(&policy(), now + Duration::hours(30), now, true, 18_900, 0);
        assert_eq!(q.tier, CancelTier::SameDay);
        assert_eq!(q.refund_cents, 0);
    }

    /// Whatever is retained plus whatever is refunded equals what was paid,
    /// in every tier, dispatched or not. Nothing is created or lost.
    #[test]
    fn money_is_conserved() {
        let now = Utc::now();
        for h in [0, 1, 23, 25, 47, 49, 200] {
            for dispatched in [false, true] {
                let q = quote_cancellation(&policy(), now + Duration::hours(h), now, dispatched, 18_901, 4_500);
                assert_eq!(q.fee_cents + q.refund_cents, 18_901 + 4_500, "at {h}h, dispatched={dispatched}");
                assert!(q.survey_applied_cents <= 4_500 && q.survey_applied_cents <= q.fee_cents);
            }
        }
    }

    /// 24–48 h is priced from `between_bps`, not silently free.
    #[test]
    fn the_24_to_48_hour_band_uses_its_own_configured_rate() {
        let now = Utc::now();
        let mut p = policy();
        p.between_bps = 500;
        let q = quote_cancellation(&p, now + Duration::hours(30), now, false, 20_000, 0);
        assert_eq!(q.tier, CancelTier::Late);
        assert_eq!(q.fee_cents, 1_000);
    }

    /// 23h59m out is inside 24 hours. Hour truncation would misprice it.
    #[test]
    fn the_boundary_is_measured_in_minutes() {
        let now = Utc::now();
        let q = quote_cancellation(&policy(), now + Duration::minutes(24 * 60 - 1), now, false, 20_000, 0);
        assert_eq!(q.fee_cents, 3_000);
    }
}
```

Rounding of `booking × bps / 10_000` is **down**, in the customer's favour,
until finance says otherwise.

- [ ] **Step 3: Widen `can_cancel()` and route every cancel through the resolver**

`can_cancel()` accepts dispatched and on-site statuses. `ShipmentService::cancel`
always calls `quote_cancellation` under `shipment.cancellation_policy_version`.
The `shipment.cancelled` payload gains `retained_cents`, `refunded_cents`,
`tier` and `policy_version`. **No code path cancels without these fields.**

**Unscheduled shipments** (`scheduled_pickup_at IS NULL`, meaning every parcel
booked before Part A) need a status-based rule. The recommended default: before
pickup assignment → `Early`; pickup assigned or later → `SameDay`. This is an
open decision; do not ship it silently.

- [ ] **Step 4: `GET /v1/shipments/:id/cancellation-preview`**

It returns exactly the handoff's JSON, computed by the same `quote_cancellation`
call with `now = Utc::now()` on the server, and it uses `may_act_on`. The cancel
call re-computes. It does **not** trust a preview the client echoes back.

- [ ] **Step 5: Invoice line**

Add `ChargeType::CancellationRetention` to `libs/types/src/invoice.rs`, then fix
every exhaustive `match` it breaks across the workspace:

```bash
CARGO_INCREMENTAL=0 cargo check --workspace --all-targets 2>&1 | grep -B2 -A6 'non-exhaustive'
```

- [ ] **Step 6: Workspace verification, deploy order**

```bash
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets
CARGO_INCREMENTAL=0 cargo test --workspace -j 2
```

`-j 2` because C: is full. The full-parallel run fails with
`invalid metadata files for crate core`, which is not a code error.
**Deploy payments (Task 3) before order-intake (Task 4)**, then grep
`cancellation-preview` out of the running `order_intake` binary.

---

## Phase 1: Part C2, driver penalties (own plan)

| Task | Where | Notes |
|---|---|---|
| 1.1 `POST /v1/assignments/:id/drop` | **dispatch**, which owns `/v1/assignments` at the gateway | `{reason_code, note, lat, lng}` → applied penalty. Publishes `assignment.dropped` for dispatch re-broadcast, payments' dispatch fee and driver-ops performance. |
| 1.2 Topics | `libs/events/src/topics.rs` + both create scripts + `check-kafka-topics.sh` | `ASSIGNMENT_DROPPED`, `DRIVER_NO_SHOW`. Pre-create on the live broker. |
| 1.3 `grace_expires_at` | `DriverAssignment` (dispatch) | Set server-side **at arrival**. There is no arrival signal today. Recommended: task `started_at` or the first `/v1/location` ping inside the pickup geofence, whichever lands first. Decide, don't infer. |
| 1.4 No-show detector | dispatch | Accepted, grace expired, no arrival → `driver.no_show`. **Reconcile with `DECLINE_BAN_THRESHOLD = 20` first.** The design suspends on no-show, the code on 20 declines. |
| 1.5 Penalty figures served | `GET /v1/drivers/me/penalty-policy` (driver-ops) | 20%, −2%, −6% and 85% come from config. The app shows the live rate. |
| 1.6 Acceptance rate | reuse `offers_seen` / `offers_claimed` on `/v1/drivers/me` | Do not invent a field. `/pass` is already penalty-free by design. |
| 1.7 Rating | driver-ops | **`rating_avg` is never written** (Finding 4). Build the feedback ingestion that 0011 promised, then decide: an adjustment column or a synthetic rating row. The fleet manager and the app must show the same number. |
| 1.8 Waiting fee | payments | `ChargeType::WaitingTimeFee`, accrued from `grace_expires_at` to release, on the **customer's** invoice. The mover is held harmless. |

## Phase 2: Part B, intent routing (own plan)

| Task | Where | Notes |
|---|---|---|
| 2.1 Classification | ai-layer | `POST /v1/agents/chat` returns `{session_id, reply, escalated, status}` today. **Recommendation:** a stateless `POST /v1/agents/classify` that creates no agent session, rather than a full session per keystroke-to-send. The prompt box needs the answer before the thinking screen, and a session per classification pollutes the agent dashboard. Return the handoff's `{intent, confidence, extracted}`. |
| 2.2 Offline fallback | customer-app | Keep the regex as the no-network fallback only, per the handoff. |
| 2.3 Persist intake | order-intake | `intake_source`, `intake_extracted` (JSONB) on the shipment, to measure parse quality against what the customer corrected. |

## Phase 3: Part A, whole-home moving (own plan)

| Task | Where | Notes |
|---|---|---|
| 3.1 Service type | order-intake `ServiceType` | Add `HomeMove`. `ServiceType` is a closed enum parsed in one place (`value_objects/mod.rs`), so update `parse`/`as_str` and every `match`. |
| 3.2 Job-level quote and create | order-intake | The handoff's `{property, rooms[], route, survey, move}` shape. **A third pricing path** beside the parcel tariff and #161's rate card: fixed fleet class, `trips × truck_fee`, round-trip distance, `helpers = max(4, ceil(volume/6)) × hourly`, per-item dismantle and packing. One arithmetic function whose rows sum, with the same reconciliation guard. |
| 3.3 Item catalogue | order-intake, **DB table** | Tenant config per the handoff ("not client constants"). Unlike accessorials, which are deployment config, this is per tenant. `GET /v1/catalogue?property_type=`. |
| 3.4 Slots | order-intake | Survey and move availability. **Survey ≥ 2 days before the move, enforced server-side**, with the client guard as a convenience. Writes `scheduled_pickup_at` (Phase 0 Task 4). |
| 3.5 Survey fee | payments | Charged at booking, `ChargeType::SurveyFee`. The credit is a negative `ManualAdjustment` or a line `discount` on the invoice, and applied to retention first on cancel (already in Task 4's resolver). |
| 3.6 Crew identity | order-intake + driver-ops | Firm = `carrier_id` (exists on drivers). Lead = a driver. **`crew_size` and multiple drivers per job do not exist**: a task has one `driver_id`. Decide: crew as a lead-only task with a size attribute, or a real multi-driver assignment. The second touches dispatch's whole model. |
| 3.7 Crew thread | — | "Message Idris" is the same missing driver↔customer thread as part 1's gap 4.1. Do not build it twice. |

## Phase 4: Part D, promotions service (own plan)

| Task | Where | Notes |
|---|---|---|
| 4.1 Scaffold `services/promotions` | Workspace, compose, `build-images.yml`, `ci-rust.yml`, gateway `promotions_url: Option<String>` | Resolve `/v1/promotions` **before** the flat `/v1` chain, beside field-ops and omnideliv. |
| 4.2 Routes | promotions | `GET /v1/promotions/offers`, `POST /v1/promotions/codes/:code/validate`, `GET /v1/promotions/loyalty/me`, `GET /v1/promotions/referrals/me`, `POST /v1/promotions/referrals/invite`, `GET /v1/promotions/credits/me` |
| 4.3 Redemption ledger | promotions DB | Per-account, so a one-per-account code stays that way. A budget tag on every discount (marketing / acquisition / loyalty) so finance can attribute spend. |
| 4.4 Window, stacking, ceiling | promotions, **server-side only** | Weekdays 11–24 (confirm against the Wiring Map's 11–19), one per month. Code vs corporate: larger wins, never both. Then tier, capped. Then credit, rolling over. Everything clipped by `min(20% gross, 50 local units)`. Say when the ceiling binds. "A cap the client enforces is a cap an attacker removes." |
| 4.5 Quote integration | order-intake → promotions over mesh-internal HTTP | `promo_code` in, `discounts[]` out. Discounts come off **billed** columns only (Task 1). Settlement is computed from `paid_cents` and the carriage take rate, and **never reads promotion state**. |
| 4.6 Corporate rate | order-intake | A tariff resolved at quote time, not a promotion. `POST /v1/accounts/me/corporate-code` needs a prefix rule at the gateway (none today). |
| 4.7 Tier gate | promotions | `claims.has_feature("loyalty_program")`. The key exists and has no consumers. |
| 4.8 Credit | promotions | A non-cash entitlement (decision). Extend the customer-app no-wallet guard so the credit panel can never gain a withdraw or top-up control. |
| 4.9 Campaign inbox | **engagement** | Self-pinned `GET /v1/customers/me/sends` plus `POST /v1/customers/me/sends/:id/read` and mark-all-read. Copy `/v1/notifications`' token-pinned scope. Do not grant `ENGAGEMENT_READ` to customers. |
| 4.10 Topics | events + scripts | `promotion.redeemed`, `referral.credited`. Pre-create them. |

---

## Open questions for the architect

**Money that must be decided before code ships it** (every value in the handoff
is a placeholder):

1. **The 24–48 hour band.** Free, late fee, or its own rate. The design leaves a
   hole a resolver cannot leave.
2. **Unscheduled shipments.** Which tier applies to a cancel when
   `scheduled_pickup_at` is null (every parcel today)?
3. **Rounding** of percentage fees. This plan rounds down, in the customer's favour.
4. **`driver_settlement_cents` in the consumer quote response.** The handoff puts it
   there. Settlement reveals the take rate to the customer, so this plan signs it
   into the quote token instead and keeps it out of the JSON. Confirm.
5. **Suspension trigger.** No-show (design), 20 declines (code), or both, and
   whether suspension is appealable. The penalty sheet lists that as open.
6. **Rating penalty.** It depends on a rating that nothing writes today.
7. **Crew model.** Lead-only task with a size, or real multi-driver assignment.
8. **"Haulline".** A tenant or white-label name, or a rename?
9. **Promo window.** 11th–24th (README, design code) or 11th–19th (Wiring Map).
