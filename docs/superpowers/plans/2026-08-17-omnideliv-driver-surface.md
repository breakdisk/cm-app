# OmniDeliv Driver-Facing Surface Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give a courier everything they need to accept a job and work it — a thin opaque offer card before the claim, a live vertical-aware manifest after it, and an `arrived` milestone — without field-ops learning anything about what a job *is*.

**Architecture:** The manifest splits at the claim. field-ops stores an opaque `offer_card` blob it never reads and returns it with the offer list; omnideliv serves the full manifest after the claim, authorized against a `courier_user_id` it learns from the existing `Assigned` event rather than by calling field-ops on a polled path.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Kafka (rdkafka), Tokio.

**Spec:** `docs/superpowers/specs/2026-08-17-omnideliv-driver-app-design.md` §Architecture.

**This is plan 2 of 3.** Plan 1 (`2026-08-17-omnideliv-driver-backend-hardening.md`) must be complete first — it closes the authorization holes these endpoints would otherwise widen. Plan 3 is the Kotlin app.

**Migration numbering:** field-ops is on `0007` after plan 1, so this plan starts at `0008`. omnideliv is on `0018`, so this plan starts at `0019`. Verify with `ls services/*/migrations/` before creating any file — if the numbers have moved, take the next free one and keep the ordering within this plan.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `services/omnideliv/migrations/0019_order_customer_contact.sql` | `customer_name`, `customer_phone` on orders | 1 |
| `services/omnideliv/src/domain/entities/order.rs` | Carry the contact | 1 |
| `services/omnideliv/src/api/http/orders.rs` | Derive the phone at checkout | 1 |
| `services/field-ops/migrations/0008_assignment_offer_card.sql` | Opaque `offer_card JSONB` | 2 |
| `services/field-ops/src/domain/entities/assignment.rs` | Carry it, never interpret it | 2 |
| `services/field-ops/src/api/http/couriers.rs` | Accept it on offer, return it on `mine`; `arrived` route | 2, 4 |
| `scripts/check-offer-card-opacity.sh` | CI tripwire against reading into the blob | 2 |
| `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs` | Build the card | 3 |
| `services/field-ops/src/infrastructure/messaging/mod.rs` | `courier_user_id` on `Assigned`; `Arrived` variant | 4, 5 |
| `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` | Persist the courier; tolerate `Arrived` | 5 |
| `services/omnideliv/migrations/0020_order_courier_user.sql` | `courier_user_id` on orders | 5 |
| `services/omnideliv/src/api/http/courier_jobs.rs` | The manifest endpoint (new file) | 6 |

---

## Pre-flight

- [ ] **Step 1: Confirm plan 1 landed**

```bash
export CARGO_INCREMENTAL=0
git log --oneline -8
ls services/field-ops/migrations/
cargo test -p logisticos-field-ops -p logisticos-omnideliv 2>&1 | grep "test result:" | awk '{s+=$4} END {print "TOTAL PASSING: " s}'
```

Expected: plan 1's five commits present, `0007_ledger_entry_job_idempotency.sql` exists, and the total is at least **231** (plan 1's floor). Record the number.

---

## Task 1: Snapshot the customer's contact onto the order

Today an order identifies its customer by a bare UUID. A courier standing at a
wrong gate has no name to ask for and no number to call. `CheckoutRequest`
carries no contact and the customer app sends none.

Identity mints `<digits>@customer.logisticos.app` for OTP sign-ins, so the phone
is recoverable from the authenticated caller's own claims at checkout — no
cross-service call, and no customer-app change.

**Files:**
- Create: `services/omnideliv/migrations/0019_order_customer_contact.sql`
- Modify: `services/omnideliv/src/domain/entities/order.rs` (`Order` struct, `place`)
- Modify: `services/omnideliv/src/infrastructure/db/order_repo.rs` (persist + load)
- Modify: `services/omnideliv/src/api/http/orders.rs` (`checkout`)
- Test: new module `customer_contact` in `order.rs`

- [ ] **Step 1: Write the failing test**

Append a test module to `services/omnideliv/src/domain/entities/order.rs`:

```rust
#[cfg(test)]
mod customer_contact {
    use super::*;

    /// Identity mints `<digits>@customer.logisticos.app` for OTP sign-ins, so
    /// the phone the courier needs is already in the caller's own token. It is
    /// never parsed out of an address a person chose for themselves.
    #[test]
    fn a_phone_derived_address_yields_the_phone() {
        assert_eq!(
            phone_from_login("639170000123@customer.logisticos.app"),
            Some("639170000123".to_string())
        );
        assert_eq!(
            phone_from_login("639170000123@driver.logisticos.app"),
            Some("639170000123".to_string())
        );
    }

    /// A real mailbox is not a phone. Splitting on `@` regardless would put
    /// "maria.reyes" on a courier's screen as a number to call.
    #[test]
    fn a_real_address_yields_nothing() {
        assert_eq!(phone_from_login("maria.reyes@gmail.com"), None);
        assert_eq!(phone_from_login("merchant@demo.com"), None);
    }

    /// The minted namespace is digits by construction. Anything else in that
    /// namespace did not come from the OTP path and must not be trusted as a
    /// number.
    #[test]
    fn a_non_numeric_local_part_in_the_minted_namespace_yields_nothing() {
        assert_eq!(phone_from_login("admin@customer.logisticos.app"), None);
        assert_eq!(phone_from_login("@customer.logisticos.app"), None);
    }

    #[test]
    fn an_order_carries_the_contact_it_was_placed_with() {
        let leg = VendorLeg::settle(Uuid::from_u128(1), Uuid::new_v4(), 10_000, 1_500);
        let o = Order::place(
            Uuid::from_u128(1), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 4_900, 0, 3_500, 14.5547, 121.0244,
            Some("Maria Reyes".to_string()), Some("639170000123".to_string()),
        );
        assert_eq!(o.customer_phone.as_deref(), Some("639170000123"));
        assert_eq!(o.customer_name.as_deref(), Some("Maria Reyes"));
    }

    /// Orders placed before this migration have no contact, and the manifest
    /// must render without one rather than refuse to load.
    #[test]
    fn an_order_without_a_contact_is_legal() {
        let leg = VendorLeg::settle(Uuid::from_u128(1), Uuid::new_v4(), 10_000, 1_500);
        let o = Order::place(
            Uuid::from_u128(1), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 4_900, 0, 3_500, 14.5547, 121.0244, None, None,
        );
        assert!(o.customer_phone.is_none());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p logisticos-omnideliv customer_contact
```

Expected: compile failure — `phone_from_login` does not exist and `Order::place` has the wrong arity.

- [ ] **Step 3: Add the helper and the fields**

In `services/omnideliv/src/domain/entities/order.rs`, above `impl Order`:

```rust
/// Namespaces identity mints from a phone number for OTP-only sign-in.
///
/// Nothing can be delivered to these addresses — they exist because the
/// platform keys accounts on an email and the thing actually verified was a
/// phone. Kept as a literal list rather than a suffix pattern so a future
/// `@partner.logisticos.app` cannot silently start yielding phone numbers.
const PHONE_DERIVED_DOMAINS: &[&str] =
    &["@customer.logisticos.app", "@driver.logisticos.app"];

/// The phone behind a login address, or `None` if that address is a real
/// mailbox somebody chose.
///
/// Never a plain `split('@')`: that would put the local part of
/// `maria.reyes@gmail.com` on a courier's screen as a number to call.
pub fn phone_from_login(email: &str) -> Option<String> {
    let local = PHONE_DERIVED_DOMAINS
        .iter()
        .find_map(|d| email.strip_suffix(*d))?;
    if local.is_empty() || !local.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(local.to_string())
}
```

Add to the `Order` struct, after `delivery_lng`:

```rust
    /// Who to hand it to, snapshotted at checkout. `None` for orders placed
    /// before migration 0019 — the manifest renders the dropoff without a name
    /// rather than refusing to load.
    pub customer_name:  Option<String>,
    /// Snapshotted rather than resolved on read: the courier needs the number
    /// that was current when the order was placed, and a lookup on a polled
    /// path would be a cross-service call per manifest refresh.
    pub customer_phone: Option<String>,
```

Add two parameters to `Order::place`, after `delivery_lng`:

```rust
        customer_name: Option<String>,
        customer_phone: Option<String>,
```

and set both in the returned struct literal.

- [ ] **Step 4: Run it to verify it passes**

```bash
cargo test -p logisticos-omnideliv customer_contact
```

Expected: 5 passed. Other call sites of `Order::place` will now fail to compile — fix each by passing `None, None` except the checkout handler, which Step 6 handles.

- [ ] **Step 5: Write the migration**

Create `services/omnideliv/migrations/0019_order_customer_contact.sql`:

```sql
-- Who the courier is delivering to.
--
-- An order identified its customer by a bare UUID, so a courier at the wrong
-- gate had no name to ask for and no number to call — the most common
-- last-mile failure, with no recovery path in the app.
--
-- Snapshotted at checkout rather than resolved on read. The manifest is polled,
-- and a cross-service identity lookup per refresh would put a courier's screen
-- on identity's availability.
--
-- Nullable, and staying nullable: orders placed before this exist and are
-- legitimate. The manifest renders a dropoff without a name rather than
-- refusing to load.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS customer_name  TEXT,
    ADD COLUMN IF NOT EXISTS customer_phone TEXT;

COMMENT ON COLUMN omnideliv.orders.customer_phone IS
  'Snapshotted at checkout from the authenticated caller. This is also what '
  'eventually unblocks SMS and WhatsApp notifications, which are push-only '
  'today because an OmniDeliv order carried no phone.';
```

- [ ] **Step 6: Persist, load, and populate at checkout**

In `services/omnideliv/src/infrastructure/db/order_repo.rs`, add `customer_name` and `customer_phone` to the orders INSERT/UPSERT column list, to the bind chain **in the same positions**, and to the row mapping on load. sqlx binds positionally and will not warn you if the order drifts.

In `services/omnideliv/src/api/http/orders.rs`, in `checkout`, derive the phone from the authenticated caller and pass both through to wherever the order is placed:

```rust
    // The courier's only way to reach the customer. Taken from the caller's own
    // validated token, never from the request body — a client-supplied number
    // would let anyone put an arbitrary phone on someone else's order.
    let customer_phone =
        crate::domain::entities::order::phone_from_login(&claims.email);
    let customer_name = None; // no display name on the OTP path yet
```

> If `phone_from_login` is not re-exported from `crate::domain::entities`, add it
> to that module's `pub use` line alongside `Order`.

- [ ] **Step 7: Verify**

```bash
cargo check -p logisticos-omnideliv && cargo test -p logisticos-omnideliv
```

Expected: `Finished`, all tests pass.

- [ ] **Step 8: Commit**

```bash
git add services/omnideliv/migrations/0019_order_customer_contact.sql services/omnideliv/src/domain/entities/order.rs services/omnideliv/src/infrastructure/db/order_repo.rs services/omnideliv/src/api/http/orders.rs
git commit -m "feat(omnideliv): an order carries who to hand it to"
```

---

## Task 2: An opaque offer card on the assignment

A courier deciding whether to take a job sees an id, a product string and a pay
figure. They cannot tell three stops from one, or a chilled pharmacy run from a
coffee.

`offer_to_nearest` fans out to the N nearest couriers, so **anything on the
offer is disclosed to everyone merely considered for it** — which is why the
card carries no customer and no street addresses at all. Those arrive with the
manifest, after the claim.

**Files:**
- Create: `services/field-ops/migrations/0008_assignment_offer_card.sql`
- Create: `scripts/check-offer-card-opacity.sh`
- Modify: `services/field-ops/src/domain/entities/assignment.rs`
- Modify: `services/field-ops/src/infrastructure/db/assignment_repo.rs`
- Modify: `services/field-ops/src/api/http/couriers.rs` (`OfferRequest`, `OfferSummary`)
- Modify: `services/field-ops/src/application/services/dispatch_service.rs` (`offer_to_nearest`)
- Modify: `.github/workflows/ci-rust.yml` (run the tripwire)

- [ ] **Step 1: Write the failing test**

Add to the test module at the end of `services/field-ops/src/domain/entities/assignment.rs` (create one if absent):

```rust
#[cfg(test)]
mod offer_card_opacity {
    use super::*;

    /// The card is a blob this tier stores and returns. The moment field-ops
    /// reads a key of it, it knows what a product's job *is* and stops being
    /// product-agnostic — the property ADR-0015 says defines a platform tier.
    #[test]
    fn the_card_round_trips_without_being_interpreted() {
        let card = serde_json::json!({
            "v": 1, "stops": 3, "pickups": 2, "distance_m": 4200,
            "vendors": ["Kuya's Lutong Bahay", "Mercury Drug"],
            "verticals": ["restaurant", "pharmacy"],
            "temperature": ["hot", "chilled"],
            "deadline_hint_mins": 38
        });

        let a = CourierAssignment::offer_with_card(
            Uuid::from_u128(1), Uuid::new_v4(),
            ProductKey::new("omnideliv".to_string()), Uuid::new_v4(),
            3_500, 0, 38_900, Some(card.clone()),
        );

        assert_eq!(a.offer_card.as_ref(), Some(&card));
    }

    /// A product that supplies nothing still gets an offer. The card is an
    /// affordance for the courier, never a precondition for dispatch.
    #[test]
    fn an_offer_without_a_card_is_legal() {
        let a = CourierAssignment::offer_with_card(
            Uuid::from_u128(1), Uuid::new_v4(),
            ProductKey::new("logisticos".to_string()), Uuid::new_v4(),
            0, 0, 0, None,
        );
        assert!(a.offer_card.is_none());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p logisticos-field-ops offer_card_opacity
```

Expected: compile failure — `offer_card` and `offer_with_card` do not exist.

- [ ] **Step 3: Add the field and constructor**

In `services/field-ops/src/domain/entities/assignment.rs`, add to `CourierAssignment` after `cod_amount_cents`:

```rust
    /// What the offering product wants a courier to see *before* they claim.
    ///
    /// Opaque, exactly like `external_ref`: this tier stores it, returns it and
    /// never reads a key of it. Columns named `vertical` or `temperature_class`
    /// would be field-ops naming product concepts in its own schema — which is
    /// interpretation, and would foreclose a third product with different ones.
    ///
    /// `offer_to_nearest` fans out, so everything in here is disclosed to every
    /// courier *considered* for the job. It therefore carries no customer and
    /// no street addresses; those arrive with the product's own manifest, after
    /// the claim.
    pub offer_card: Option<serde_json::Value>,
```

Add a constructor alongside `offer_with_earnings` that takes the card, and have `offer_with_earnings` delegate to it with `None` so existing callers are unchanged:

```rust
    /// Offer a job at a stated rate, with a card for the courier to judge it by.
    #[allow(clippy::too_many_arguments)]
    pub fn offer_with_card(
        tenant_id: Uuid,
        courier_id: Uuid,
        product: ProductKey,
        external_ref: Uuid,
        trip_cents: i64,
        tip_cents: i64,
        cod_amount_cents: i64,
        offer_card: Option<serde_json::Value>,
    ) -> Self {
        let mut a = Self::offer_with_earnings(
            tenant_id, courier_id, product, external_ref,
            trip_cents, tip_cents, cod_amount_cents,
        );
        a.offer_card = offer_card;
        a
    }
```

and set `offer_card: None` in `offer_with_earnings`'s struct literal.

- [ ] **Step 4: Run it to verify it passes**

```bash
cargo test -p logisticos-field-ops offer_card_opacity
```

Expected: 2 passed.

- [ ] **Step 5: Write the migration**

Create `services/field-ops/migrations/0008_assignment_offer_card.sql`:

```sql
-- What a courier sees before deciding to take a job.
--
-- A blob, not columns. Columns named `vertical` or `temperature_class` would be
-- this tier naming a product's concepts in its own schema, which is exactly the
-- interpretation ADR-0015 says a platform tier must not do -- and would
-- foreclose a third product whose concepts differ.
--
-- No index, deliberately. An index would imply something here is queried, and
-- nothing in this service may query into it. `scripts/check-offer-card-opacity.sh`
-- fails the build if anything ever does.
ALTER TABLE field_ops.courier_assignments
    ADD COLUMN IF NOT EXISTS offer_card JSONB;

COMMENT ON COLUMN field_ops.courier_assignments.offer_card IS
  'Opaque product-supplied summary, stored and returned verbatim, never read '
  'by field-ops. Disclosed to every courier in the fanout, so it carries no '
  'customer identity and no street addresses.';
```

- [ ] **Step 6: Persist and return it**

In `services/field-ops/src/infrastructure/db/assignment_repo.rs`: add `offer_card` to the INSERT/UPSERT column list, to the bind chain in the matching position, and to the row mapping (`r.get("offer_card")`).

In `services/field-ops/src/api/http/couriers.rs`:
- add to `OfferRequest`: `#[serde(default)] pub offer_card: Option<serde_json::Value>,`
- add to `OfferSummary`: `offer_card: Option<serde_json::Value>,` and populate it from `a.offer_card.clone()` in `my_offers`
- also add `cod_amount_cents: a.cod_amount_cents` to `OfferSummary` — a courier deciding whether to take a job needs to know they will be holding the platform's cash, and that figure is already on the assignment

In `services/field-ops/src/application/services/dispatch_service.rs`: thread `offer_card: Option<serde_json::Value>` through `offer_to_nearest` to `CourierAssignment::offer_with_card`, and pass it from the handler.

- [ ] **Step 7: Write the CI tripwire**

Create `scripts/check-offer-card-opacity.sh`:

```bash
#!/usr/bin/env bash
# field-ops must never read into `offer_card`. It stores the blob and returns
# it; the moment a query reaches inside, this tier knows what a product's job
# is and has stopped being product-agnostic.
#
# The column is written and read whole, so any JSON path operator against it is
# the failure this guards.
set -euo pipefail

if grep -rInE "offer_card[[:space:]]*(->>|->|#>|@>|\?)" \
     --include='*.rs' --include='*.sql' services/field-ops/; then
  echo
  echo "ERROR: field-ops reads into offer_card."
  echo "That column is opaque by design -- see ADR-0015 and migration 0008."
  echo "If a product needs field-ops to act on something, it belongs in a"
  echo "first-class column that this tier declares and owns."
  exit 1
fi

echo "OK: offer_card is still opaque to field-ops."
```

Make it executable and register it. **Every `.sh` in this repo was committed from Windows as mode 100644**, and `ci-rust.yml` runs scripts before the toolchain install, so a non-executable script dies with exit 126 and everything after it silently never runs:

```bash
git update-index --chmod=+x scripts/check-offer-card-opacity.sh
```

Add it to `.github/workflows/ci-rust.yml` next to the existing `./scripts/check-runtime-boundary.sh` invocation.

- [ ] **Step 8: Verify the tripwire actually trips**

```bash
./scripts/check-offer-card-opacity.sh
```

Expected: `OK`. Then temporarily add `-- WHERE offer_card->>'v' = '1'` to a comment in `assignment_repo.rs`, re-run, and confirm it **exits 1**. Remove it.

A guard that has never been seen to fail is not a guard — three in this repo's history each passed the bug they were written for.

- [ ] **Step 9: Commit**

```bash
git add services/field-ops/migrations/0008_assignment_offer_card.sql scripts/check-offer-card-opacity.sh .github/workflows/ci-rust.yml services/field-ops/src
git commit -m "feat(field-ops): carry an opaque offer card the tier never reads"
```

---

## Task 3: omnideliv builds the card

**Files:**
- Modify: `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs`
- Modify: `services/omnideliv/src/application/services/checkout_service.rs` (or wherever `CourierDispatch::offer` is called)
- Test: new module `offer_card` in `field_ops_dispatch.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod offer_card {
    use super::*;

    fn stop(name: &str, vertical: &str, temp: &str) -> CardStop {
        CardStop { vendor_name: name.to_string(), vertical: vertical.to_string(),
                   temperature: temp.to_string() }
    }

    /// Enough to judge the job: how much work, how far, what kind, what it pays
    /// (pay rides on the assignment itself).
    #[test]
    fn the_card_describes_the_shape_of_the_job() {
        let card = build_offer_card(
            &[stop("Kuya's Lutong Bahay", "restaurant", "hot"),
              stop("Mercury Drug", "pharmacy", "chilled")],
            4_200,
            38,
        );

        assert_eq!(card["v"], 1);
        assert_eq!(card["pickups"], 2);
        assert_eq!(card["stops"], 3, "two pickups plus the dropoff");
        assert_eq!(card["distance_m"], 4_200);
        assert_eq!(card["deadline_hint_mins"], 38);
        assert_eq!(card["verticals"], serde_json::json!(["restaurant", "pharmacy"]));
        assert_eq!(card["temperature"], serde_json::json!(["hot", "chilled"]));
    }

    /// The rule the fanout forces. `offer_to_nearest` offers to N couriers, so
    /// a customer's address on the card is a customer's address handed to every
    /// courier who was merely considered and declined.
    #[test]
    fn the_card_discloses_nothing_about_the_customer() {
        let card = build_offer_card(
            &[stop("Kuya's Lutong Bahay", "restaurant", "hot")], 1_200, 15,
        );
        let text = serde_json::to_string(&card).unwrap().to_lowercase();

        for leaked in ["lat", "lng", "address", "customer", "phone", "name\":\"maria"] {
            assert!(!text.contains(leaked), "offer card must not carry `{leaked}`");
        }
    }

    /// Duplicates would tell a courier the run is more varied than it is.
    #[test]
    fn repeated_verticals_appear_once() {
        let card = build_offer_card(
            &[stop("Kuya's", "restaurant", "hot"), stop("Jollibee", "restaurant", "hot")],
            2_000, 20,
        );
        assert_eq!(card["verticals"], serde_json::json!(["restaurant"]));
        assert_eq!(card["temperature"], serde_json::json!(["hot"]));
        assert_eq!(card["pickups"], 2);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p logisticos-omnideliv offer_card
```

Expected: compile failure — `CardStop` and `build_offer_card` do not exist.

- [ ] **Step 3: Implement**

In `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs`:

```rust
/// One pickup, reduced to what a courier needs before they commit.
pub struct CardStop {
    pub vendor_name: String,
    pub vertical:    String,
    pub temperature: String,
}

/// The pre-claim summary handed to field-ops as an opaque blob.
///
/// Names businesses but never the customer: `offer_to_nearest` fans out to the
/// nearest N couriers, so every field here is disclosed to people who will
/// decline this job. A vendor is a public storefront; a delivery address is not.
pub fn build_offer_card(
    stops: &[CardStop],
    distance_m: i64,
    deadline_hint_mins: i64,
) -> serde_json::Value {
    let mut verticals: Vec<&str> = Vec::new();
    let mut temperature: Vec<&str> = Vec::new();
    for s in stops {
        if !verticals.contains(&s.vertical.as_str()) { verticals.push(&s.vertical); }
        if !temperature.contains(&s.temperature.as_str()) { temperature.push(&s.temperature); }
    }

    serde_json::json!({
        // Bumped only on a breaking change. The app renders defensively on an
        // unknown version rather than failing to draw the offer at all.
        "v": 1,
        "stops": stops.len() + 1,          // pickups plus the single dropoff
        "pickups": stops.len(),
        "distance_m": distance_m,
        "deadline_hint_mins": deadline_hint_mins,
        "vendors": stops.iter().map(|s| s.vendor_name.clone()).collect::<Vec<_>>(),
        "verticals": verticals,
        "temperature": temperature,
    })
}
```

Then thread it: `CourierDispatch::offer` gains an `offer_card: Option<serde_json::Value>` parameter, `FieldOpsDispatch::offer` serialises it into the request body, and the checkout path builds one from the order's legs and the consolidation plan's stops.

- [ ] **Step 4: Run it to verify it passes**

```bash
cargo test -p logisticos-omnideliv offer_card && cargo check -p logisticos-omnideliv
```

Expected: 3 passed, `Finished`.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src
git commit -m "feat(omnideliv): describe a job to a courier without naming the customer"
```

---

## Task 4: The `arrived` milestone

"En Route" is derivable — claimed, and not yet collected at the next stop.
"Arrived" is not: a geofence cannot distinguish parked outside from at the door,
and it is the event a customer most wants pushed.

**Files:**
- Modify: `services/field-ops/src/infrastructure/messaging/mod.rs` (`CourierEvent::Arrived`)
- Modify: `services/field-ops/src/application/services/dispatch_service.rs` (`mark_arrived`)
- Modify: `services/field-ops/src/api/http/couriers.rs` (route + handler)
- Modify: `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` (tolerate it)
- Test: extend `milestone_authorization` in `dispatch_service.rs`

- [ ] **Step 1: Write the failing test**

Add to the `milestone_authorization` module created by plan 1 Task 1 — it already has `fixture()`, `RecordingEvents` and the mocks:

```rust
    #[tokio::test]
    async fn the_holder_can_mark_arrived() {
        let (svc, id, holder, _, _, _, events) = fixture();
        assert!(svc.mark_arrived(TENANT, holder, id, Uuid::new_v4(), None).await.unwrap());
        assert_eq!(*events.emitted.lock().unwrap(), vec!["arrived"]);
    }

    /// Same rule as every other milestone: assignment ids are not secret.
    #[tokio::test]
    async fn another_courier_cannot_mark_arrived() {
        let (svc, id, _, other, _, _, events) = fixture();
        assert!(!svc.mark_arrived(TENANT, other, id, Uuid::new_v4(), None).await.unwrap());
        assert!(events.emitted.lock().unwrap().is_empty());
    }

    /// The fan-out case. One job is offered to five couriers and only the
    /// winner's row is claimed; the four losers keep a readable id.
    #[tokio::test]
    async fn a_courier_who_only_received_an_offer_cannot_mark_arrived() {
        let (svc, id, holder, _, assignments, _, events) = offered_fixture();
        assert!(!svc.mark_arrived(TENANT, holder, id, Uuid::new_v4(), None).await.unwrap());
        assert!(events.emitted.lock().unwrap().is_empty());
        assert_eq!(assignments.rows.lock().unwrap()[0].status, AssignmentStatus::Offered);
    }
```

> `offered_fixture()` is the same-status-chooser helper plan 1's Task 1 fix
> added alongside `fixture()`. If it is named differently in the landed code,
> use whatever that commit introduced rather than adding a second one.

Extend `RecordingEvents`' match with `CourierEvent::Arrived { .. } => "arrived",`.

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p logisticos-field-ops milestone_authorization
```

Expected: compile failure — no `Arrived` variant, no `mark_arrived`.

- [ ] **Step 3: Add the variant, keyed like its siblings**

In `services/field-ops/src/infrastructure/messaging/mod.rs`, add to `CourierEvent`:

```rust
    /// The courier is at a stop. `stop_ref` is opaque — the offering product
    /// sets it (OmniDeliv uses the vendor id for a pickup and the order id for
    /// the dropoff) and this tier never resolves it, exactly as it never
    /// resolves `external_ref`.
    Arrived   { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                stop_ref: Uuid,
                device_timestamp: Option<chrono::DateTime<chrono::Utc>> },
```

and add `CourierEvent::Arrived { external_ref, .. }` to the `key()` match arm, so it partitions with the rest of its job's events and cannot arrive out of order.

- [ ] **Step 4: Add `mark_arrived`**

In `dispatch_service.rs`, next to `mark_collected`:

```rust
    /// The courier is at a stop. Published, never persisted: it changes no
    /// assignment state, and a milestone that only informs does not need a row.
    ///
    /// Gated on a live claim like `mark_collected`, for the same reason — see
    /// `assignment_for_courier`. Being *offered* a job is not carrying it.
    pub async fn mark_arrived(
        &self,
        tenant_id: Uuid,
        user_id: Uuid,
        assignment_id: Uuid,
        stop_ref: Uuid,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<bool> {
        let Some(a) = self.assignment_for_courier(tenant_id, user_id, assignment_id).await? else {
            return Ok(false);
        };
        // `offer_to_nearest` addresses one job to five couriers and only the
        // winner's row is claimed. The losers keep a readable assignment id and
        // must not be able to report against it.
        if a.status != AssignmentStatus::Claimed {
            return Ok(false);
        }

        self.emit(CourierEvent::Arrived {
            tenant_id,
            product: a.product.as_str().to_string(),
            external_ref: a.external_ref,
            courier_id: a.courier_id,
            stop_ref,
            device_timestamp,
        })
        .await;
        Ok(true)
    }
```

- [ ] **Step 5: Add the route**

In `couriers.rs`, a request type and handler mirroring `collected`:

```rust
#[derive(Debug, Deserialize)]
pub struct ArrivedRequest {
    /// Opaque to this tier. The product that offered the job knows what it means.
    pub stop_ref: Uuid,
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}
```

```rust
async fn arrived(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(req): Json<ArrivedRequest>,
) -> Result<StatusCode, StatusCode> {
    let found = st
        .dispatch
        .mark_arrived(claims.tenant_id, claims.user_id, id, req.stop_ref, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "arrived failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if !found {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::ACCEPTED)
}
```

Register it on the router, on its own `.route()` call:

```rust
        .route("/v1/field-ops/assignments/:id/arrived", post(arrived))
```

**Do not merge it into an existing `.route()` for a different path.** Two `.route()` calls on the *same* path panic at startup in axum; different paths are fine.

- [ ] **Step 6: Teach omnideliv's consumer to tolerate it**

`services/omnideliv/src/infrastructure/messaging/courier_consumer.rs` mirrors `CourierEvent` as its own tagged enum, and an existing test pins that the wire tags match. A new variant field-ops publishes that omnideliv cannot deserialise makes **every** message on that topic fail, not just the new one.

Add the matching variant to the mirrored enum and a `handle` arm that records it on the timeline without advancing status:

```rust
            CourierEvent::Arrived { stop_ref, courier_id, device_timestamp, .. } => {
                // No status change: arrival is progress a tracking screen shows,
                // not a lifecycle transition. Recorded so the timeline can show
                // it and so SLA maths has the device clock.
                self.append(tenant_id, order_id, event_type::COURIER_ARRIVED,
                            device_timestamp, Some(courier_id),
                            serde_json::json!({ "stop_ref": stop_ref })).await;
            }
```

Add `COURIER_ARRIVED` to the `event_type` module. Extend the existing wire-tag test with an `"arrived"` payload so a rename on either side fails loudly.

- [ ] **Step 7: Verify**

```bash
cargo test -p logisticos-field-ops -p logisticos-omnideliv
```

Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add services/field-ops/src services/omnideliv/src
git commit -m "feat(field-ops): a courier can report arriving at a stop"
```

---

## Task 5: Persist the courier on the order

`CourierEvent::Assigned` already carries `courier_id`; omnideliv keeps the
assignment id and throws the courier away. The manifest needs to authorize a
caller's `user_id`, and `courier_id` is field-ops' own key — a different UUID.
Adding the user id to the event lets omnideliv authorize locally, with no
cross-service call on a polled path.

**Files:**
- Create: `services/omnideliv/migrations/0020_order_courier_user.sql`
- Modify: `services/field-ops/src/infrastructure/messaging/mod.rs`, `dispatch_service.rs` (`claim`)
- Modify: `services/omnideliv/src/infrastructure/messaging/courier_consumer.rs`
- Modify: `services/omnideliv/src/domain/entities/order.rs` (`courier_claimed`)

- [ ] **Step 1: Write the failing test**

In `courier_consumer.rs`'s test module:

```rust
    /// Backward compatibility, and it is load-bearing: messages published
    /// before this field existed are still on the topic and in the retention
    /// window. Without a default they fail to deserialise and take every
    /// message on the partition down with them.
    #[test]
    fn an_assigned_event_without_a_courier_user_still_parses() {
        let raw = serde_json::json!({
            "event": "assigned",
            "tenant_id": Uuid::nil(), "product": "omnideliv",
            "external_ref": Uuid::nil(), "courier_id": Uuid::nil(),
            "assignment_id": Uuid::nil()
        });
        let parsed: CourierEvent = serde_json::from_value(raw).unwrap();
        match parsed {
            CourierEvent::Assigned { courier_user_id, .. } => assert!(courier_user_id.is_none()),
            other => panic!("wrong variant: {other:?}"),
        }
    }
```

In `order.rs`'s test module:

```rust
    #[test]
    fn claiming_records_which_user_is_carrying_it() {
        let leg = VendorLeg::settle(Uuid::from_u128(1), Uuid::new_v4(), 10_000, 1_500);
        let mut o = Order::place(
            Uuid::from_u128(1), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            vec![leg], 4_900, 0, 3_500, 14.5547, 121.0244, None, None,
        );
        let user = Uuid::new_v4();
        o.courier_claimed(Uuid::new_v4(), Some(user)).unwrap();
        assert_eq!(o.courier_user_id, Some(user));
    }
```

- [ ] **Step 2: Run to verify both fail**

```bash
cargo test -p logisticos-omnideliv an_assigned_event_without_a_courier_user claiming_records_which_user
```

Expected: compile failures.

- [ ] **Step 3: Add the field on both sides of the wire**

field-ops `messaging/mod.rs`, on the `Assigned` variant:

```rust
    Assigned  { tenant_id: Uuid, product: String, external_ref: Uuid, courier_id: Uuid,
                assignment_id: Uuid,
                /// The identity user behind the courier, so a consuming product
                /// can authorize that user against this job without asking us.
                /// `Option` + `serde(default)` because messages published before
                /// this field are still within the retention window.
                #[serde(default)] courier_user_id: Option<Uuid> },
```

Mirror it exactly in omnideliv's copy, including `#[serde(default)]`.

In `dispatch_service.rs`'s `claim`, the `Assigned` emit already re-reads the assignment; look the courier up and populate the field:

```rust
                if let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? {
                    self.emit(CourierEvent::Assigned {
                        tenant_id,
                        product: a.product.as_str().to_string(),
                        external_ref: a.external_ref,
                        courier_id: a.courier_id,
                        assignment_id: a.id,
                        // The caller is the courier — `claim` refused above if not.
                        courier_user_id: Some(user_id),
                    })
                    .await;
                }
```

- [ ] **Step 4: Persist it**

Create `services/omnideliv/migrations/0020_order_courier_user.sql`:

```sql
-- Which identity user is carrying this order.
--
-- `courier_task_id` is a field-ops *assignment* id and `courier_id` is
-- field-ops' own key for the person; neither can be compared against the
-- `user_id` in a courier's JWT. The driver manifest authorizes on exactly that
-- comparison, and resolving it per request would put a polled endpoint on
-- another service's availability.
--
-- Nullable: orders claimed before this column existed have none, and the
-- manifest refuses those rather than guessing.
ALTER TABLE omnideliv.orders
    ADD COLUMN IF NOT EXISTS courier_user_id UUID;
```

Add `pub courier_user_id: Option<Uuid>` to `Order`, give `courier_claimed` a second parameter, and set it. Persist and load it in `order_repo.rs` — **COALESCE it in the upsert**, matching how `delivery_lat`/`delivery_lng` are handled, so a later status change cannot erase it.

Wire the consumer's `Assigned` arm to pass `courier_user_id` through.

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p logisticos-field-ops -p logisticos-omnideliv
git add services/omnideliv/migrations/0020_order_courier_user.sql services/field-ops/src services/omnideliv/src
git commit -m "feat(omnideliv): an order knows which user is carrying it"
```

---

## Task 6: The manifest endpoint

**Files:**
- Create: `services/omnideliv/src/api/http/courier_jobs.rs`
- Modify: `services/omnideliv/src/api/http/mod.rs` (register), `services/api-gateway/src/proxy/mod.rs` if the prefix is not already routed

- [ ] **Step 1: Write the failing test**

In the new file:

```rust
#[cfg(test)]
mod authorization {
    use super::*;

    /// The whole access rule. Assignment ids reach couriers' phones, so a
    /// manifest keyed on anything a caller can name is a manifest any courier
    /// can read.
    #[test]
    fn only_the_carrying_courier_may_read_a_manifest() {
        let carrying = Uuid::new_v4();
        assert!(may_read_manifest(Some(carrying), carrying));
        assert!(!may_read_manifest(Some(carrying), Uuid::new_v4()));
    }

    /// Orders claimed before migration 0020 have no courier recorded. Refuse
    /// rather than fall open — "we do not know who is carrying this" must not
    /// read as "anyone may look".
    #[test]
    fn an_order_with_no_recorded_courier_is_refused() {
        assert!(!may_read_manifest(None, Uuid::new_v4()));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cargo test -p logisticos-omnideliv only_the_carrying_courier
```

Expected: compile failure — `may_read_manifest` does not exist.

- [ ] **Step 3: Implement the predicate, then the handler**

```rust
/// May this caller read this order's manifest?
///
/// A pure function so the rule is testable without a database, and so the
/// fall-open case has a test of its own.
fn may_read_manifest(order_courier_user_id: Option<Uuid>, caller: Uuid) -> bool {
    matches!(order_courier_user_id, Some(c) if c == caller)
}
```

Handler `GET /v1/omnideliv/courier/jobs/:order_id`:

1. Load the order for `claims.tenant_id`; `None` → 404.
2. `may_read_manifest(order.courier_user_id, claims.user_id)` → false → **404**, not 403, so a courier cannot probe which order ids exist.
3. Load vendors via `st.vendors.find_by_ids(tenant_id, &vendor_ids)` — the pattern `tracking.rs` already uses.
4. Load the basket by `order.basket_id` and group its lines by `vendor_id`; resolve item names through the catalog.
5. Assemble and return:

```rust
#[derive(Debug, Serialize)]
pub struct ManifestResponse {
    pub order_id:          Uuid,
    pub status:            String,
    /// What the courier collects at the door. 0 for a prepaid order once that
    /// rail exists.
    pub cod_amount_cents:  i64,
    pub trip_cents:        i64,
    pub stops:             Vec<ManifestStop>,
    pub dropoff:           Dropoff,
}

#[derive(Debug, Serialize)]
pub struct ManifestStop {
    /// What the app sends back on `arrived` / `collected`. Opaque to field-ops.
    pub stop_ref:          Uuid,
    pub seq:               i32,
    pub vendor_name:       String,
    pub address:           String,
    pub lat:               f64,
    pub lng:               f64,
    pub vertical:          String,
    pub prep_time_minutes: i32,
    pub picked_up:         bool,
    pub lines:             Vec<ManifestLine>,
}

#[derive(Debug, Serialize)]
pub struct ManifestLine {
    pub qty:       i32,
    pub item_name: String,
    /// Chosen options, so the courier can check the bag against what was ordered.
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Dropoff {
    /// `stop_ref` for the dropoff is the order id.
    pub stop_ref: Uuid,
    pub lat:      f64,
    pub lng:      f64,
    /// `None` for orders placed before migration 0019. The app renders the
    /// dropoff without a name rather than inventing one.
    pub customer_name:  Option<String>,
    pub customer_phone: Option<String>,
    /// Always `None` today — a free-text delivery note needs a field on the
    /// customer's checkout screen, which is customer-app work. Present in the
    /// contract so adding it later is not a breaking change.
    pub notes: Option<String>,
}
```

An order with no `delivery_lat`/`delivery_lng` (pre-migration-0013) returns **409**, not a guessed point: sending a courier to the wrong address is worse than telling them this job cannot be worked.

- [ ] **Step 4: Register the route and check the gateway**

Add `pub mod courier_jobs;` and merge its router in `services/omnideliv/src/api/http/mod.rs`.

`/v1/omnideliv/*` is already routed to omnideliv by `resolve_upstream`, so no gateway change should be needed — confirm:

```bash
grep -n "v1/omnideliv" services/api-gateway/src/proxy/mod.rs
```

- [ ] **Step 5: Verify and commit**

```bash
cargo test -p logisticos-omnideliv && cargo clippy -p logisticos-omnideliv --all-targets -- -D warnings
git add services/omnideliv/src
git commit -m "feat(omnideliv): serve a courier the manifest for the job they hold"
```

---

## Task 7: Full verification

- [ ] **Step 1: Clippy and tests**

```bash
export CARGO_INCREMENTAL=0
cargo clippy -p logisticos-field-ops -p logisticos-omnideliv --all-targets -- -D warnings
cargo test -p logisticos-field-ops -p logisticos-omnideliv 2>&1 | grep "test result:" | awk '{s+=$4} END {print "TOTAL: " s}'
```

- [ ] **Step 2: The opacity tripwire**

```bash
./scripts/check-offer-card-opacity.sh
```

Expected `OK`, and confirmed to exit 1 when violated (Task 2 Step 8).

- [ ] **Step 3: Migrations against a scratch database**

Create a scratch Postgres **with extensions** — a hand-created database has only `plpgsql`, and field-ops' first migration dies on `st_makepoint`:

```bash
docker run --rm -d --name od-scratch -e POSTGRES_PASSWORD=x -p 55433:5432 postgres:16
sleep 5
PGPASSWORD=x psql -h localhost -p 55433 -U postgres -c "CREATE DATABASE svc_omnideliv_scratch; CREATE DATABASE svc_field_ops_scratch;"
for db in svc_omnideliv_scratch svc_field_ops_scratch; do
  PGPASSWORD=x psql -h localhost -p 55433 -U postgres -d $db -c "CREATE EXTENSION IF NOT EXISTS postgis; CREATE EXTENSION IF NOT EXISTS pgcrypto; CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\"; CREATE EXTENSION IF NOT EXISTS pg_trgm;"
done
```

Run both services' migrations against them and confirm `0008`, `0019` and `0020` apply.

```bash
docker rm -f od-scratch
```

- [ ] **Step 4: The Kafka wire contract**

Both mirrored enums must agree. Confirm the wire-tag test covers `assigned`, `collected`, `delivered` **and** `arrived`, and that `assigned` parses with and without `courier_user_id`:

```bash
cargo test -p logisticos-omnideliv the_wire_tags_match_field_ops an_assigned_event_without_a_courier_user
```

A variant one side publishes and the other cannot deserialise fails **every** message on the partition, not only the new kind.

---

## Done when

- [ ] Seven commits, each with tests written first
- [ ] Clippy clean on both services
- [ ] `check-offer-card-opacity.sh` registered in CI, executable bit set, and observed to fail when violated
- [ ] Migrations 0008 / 0019 / 0020 apply to a scratch database with extensions
- [ ] `Assigned` parses both with and without `courier_user_id`
- [ ] A courier can be offered a job, see its shape, claim it, read its manifest, and report arriving — all via HTTP, with no app in existence

Then plan 3 — the Kotlin app — can begin.
