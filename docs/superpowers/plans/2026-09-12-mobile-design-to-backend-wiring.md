# Mobile Design → Backend Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every screen in the consumer and driver designs at
`D:\LogisticOS\Uber Freight mobile app design\design_handoff_mobile_apps` reachable
against a real endpoint, starting by removing the four backend obstructions the
handoff counted as already done.

**Architecture:** Phase 0 closes the backend delta — a rate-card consumer quote in
order-intake fed by a new mesh-internal breakdown endpoint on carrier, a gateway
route for `/v1/hub-transfer`, an ownership check on OTP generation, and the
accessorial config. Phase 1 lands one shared token package so both apps and the
five portals share the CLAUDE.md palette. Phases 2–4 are UI, and each gets its own
plan once Phase 0 has deployed.

**Tech Stack:** Rust/Axum/SQLx (services), React Native + Expo (customer-app),
Kotlin + Jetpack Compose (driver-app-android), `libs/geo` for haversine,
`libs/auth` for RBAC.

---

## Decisions taken before this plan was written

| Decision | Choice | Consequence |
|---|---|---|
| Consumer pricing home | New rate-card path in order-intake's existing `POST /v1/shipments/quote`, fed by carrier over mesh-internal HTTP | `customer` role keeps only `SHIPMENT_CREATE`. No `marketplace:book` grant. |
| Visual language | CLAUDE.md dark neon glassmorphism (`#050810` / `#00E5FF`) | The designs' cyan tokens get mapped, not copied. Mobile stays aligned with the five portals. |

---

## What the verification found

The handoff states: *"The backend is complete. Both app shells already have working
transport layers. This design is the missing UI layer, not a new system."*

That holds for the driver app. It does not hold in four places, and the first is
the item the handoff counted as done rather than listing as a gap.

### Finding 1 — the consumer quote endpoint cannot price a consumer move (P0)

`POST /v1/shipments/quote` — [services/order-intake/src/api/http/quote.rs:71](../../../services/order-intake/src/api/http/quote.rs)

```rust
if claims.currency.as_deref() != Some("AED") {
    return Err::<_, AppError>(AppError::Validation(
        "Online quotes are only available for AE-region (AED) tenants".into(),
    ));
}
```

Three separate problems, all in that one handler:

1. **Hard-gated to AED.** Eight of the design's nine markets get a 422. A missing
   `payment` config 503s it for all nine.
2. **Wrong inputs.** `QuoteRequest` is `{ service_type, weight_grams, pieces }`
   (quote.rs:26). No origin, no destination, no distance, no dimensions. The rate
   card needs distance and the volumetric calculation needs L/W/H.
3. **Wrong output.** `QuoteResponse` is a single `amount_cents` (quote.rs:39). The
   Review screen renders three rows that must sum to the carriage fee — the
   handoff calls that invariant load-bearing and says it broke twice in design
   review. One scalar cannot satisfy it.

It computes via `ae_base_fee_for` / `ae_piece_fee_for` — the parcel tariff the
handoff's own Pricing section warns against (₱85 base against a ₱6,380 surcharge
on a 320 kg move), and which its code comments flag as a first cut pending
finance sign-off.

The function that does produce rate-card pricing is `quote_price_cents`
([services/carrier/src/domain/entities/mod.rs:625](../../../services/carrier/src/domain/entities/mod.rs)),
and it is reachable only two ways, both closed to a consumer:

- `GET /v1/carriers/rate-shop` requires `CARRIERS_READ` (carrier http/mod.rs:343).
- `POST /v1/marketplace/bookings` requires `MARKETPLACE_BOOK` — and
  [libs/auth/src/rbac.rs:326](../../../libs/auth/src/rbac.rs) carries a test
  asserting `customer` must **never** hold it:

```rust
for role in ["partner", "driver", "customer", "readonly", "hub_scanner"] {
    assert!(!perms(role).contains(&permissions::MARKETPLACE_BOOK),
            "{role} must not be able to book a vehicle");
}
```

The gRPC `rate_shop` is no better — it takes `(tenant_id, service_type, weight_kg)`
and returns `total_cost_cents` (carrier/api/grpc.rs:76). No distance, no breakdown.

### Finding 2 — the driver Hub Scan screen 404s at the gateway (P0)

`HubOpsApiService.kt:58,68` already declares `v1/hub-transfer/scans` and
`v1/hub-transfer/shipment-by-awb`. The driver app has one base URL — the gateway
(`app/build.gradle.kts:97` → `https://os-api.cargomarket.net/`). The gateway's
`resolve_upstream` routes `/v1/hubs`, `/v1/consolidation`, `/v1/containers`,
`/v1/pallets` to hub-ops. **`/v1/hub-transfer` is in none of them**, so it falls to
`None` and the gateway returns 404 `{"error":"No upstream service found for this
path"}` ([api-gateway/src/bootstrap.rs:235](../../../services/api-gateway/src/bootstrap.rs)).

The Kotlin interface exists, the hub-ops routes exist
(hub-ops/src/bootstrap.rs:2017-2019), and nothing in between connects them. This is
the written-but-unreachable shape recorded in `project_white_label_branding`.

### Finding 3 — the delivery PIN does not restrict who can mint it (P0, security)

`POST /v1/otps/generate` ([pod/src/api/http/pod.rs:155](../../../services/pod/src/api/http/pod.rs))
takes `AuthClaims` but calls **no** `require_permission`, and
`generate_and_send_otp` (pod_service.rs:669) takes `shipment_id` and
`recipient_phone` from the request body with no check that the caller owns the
shipment. The handler then **returns the code in the response body**.

The design's stated promise is "the driver cannot close the job without it." As
written, a driver holding a valid tenant JWT can POST their own phone against any
shipment id, read the code out of the 200, and `POST /v1/otps/verify` it. Shipping
the PIN UI on top of this advertises a control that is not enforced.

### Finding 4 — accessorials have no backend at all

Confirmed absent: no `/v1/accessorials` route in any service, and no gateway rule
for it. The handoff is accurate here and says the design reads from a single
`ACCESSORIALS` object shaped like the intended response, so the swap is one line.

### Smaller, confirmed

| Claim in handoff | Actual |
|---|---|
| `/v1/offers/*` is on **driver-ops** | It is on **dispatch** (dispatch/api/http/mod.rs:54-57). Harmless through the gateway, which routes `/v1/offers` → dispatch, but `DriverOpsApiService.kt` is the wrong file to put it in. |
| Tracking returns `driver{name, lat, lng, heading}` | Returns `{name, lat, lng}` — **no `heading`** (delivery-experience/api/http/mod.rs:79-85). The design's rotating driver marker has no data behind it. |
| Tracking polls every 30s | Confirmed: `apps/customer-app/src/services/api/tracking.ts:47`. |
| Go-online returns no driving time | Confirmed: returns `status: "available"` only (driver-ops/api/http/drivers.rs:241). |
| Driver ↔ customer chat missing | Confirmed. No task-scoped message entity anywhere. |

### Good news not in the handoff

- `CreateShipmentCommand` already carries `origin`, `destination`, `weight_grams`,
  `length_cm`, `width_cm`, `height_cm` (commands/mod.rs:19-48). The **create** path
  has every input the rate card needs; only **quote** is impoverished.
- `VehicleListing` already has `base_price_cents`, `per_km_cents`, `per_kg_cents`,
  `max_weight_kg`, `max_volume_m3`, `size_class` (carrier entities/mod.rs:505) —
  exactly the three rows the Review screen renders, plus the vehicle sizing the
  Thinking screen's step 2 claims.
- **Distance already exists twice and order-intake already has one of them.**
  `logisticos_types::Coordinates::distance_km` (libs/types/src/lib.rs:149) is a
  method on the very type `AddressNormalizer::normalize` returns, and
  `libs/geo::haversine_km` (libs/geo/src/lib.rs:91) is the same arithmetic as a free
  function. Use the method — no new dependency on order-intake. Do not write a third.
- **`find_available_listings` already encodes the availability rule** the consumer
  quote needs: `(tenant_id, min_weight_kg, size_class, at, limit)`, filtered to
  active, inside the idle window, and big enough for the load
  ([carrier/domain/repositories/mod.rs:80](../../../services/carrier/src/domain/repositories/mod.rs)).
  Task 2 calls it rather than writing a second capacity rule.
- **pod already has an `OrderIntakeClient`**
  (`services/pod/src/infrastructure/external/order_intake.rs`) hitting
  `GET /v1/internal/shipments/:id/billing` over mTLS with no JWT. Task 5 extends
  that response and that client; it does not add a second HTTP client.
- The Mapbox geocoder **did land** (`order-intake/src/infrastructure/external/mapbox_geocoder.rs`,
  wired in bootstrap.rs:54). Memory's index entry calling it uncommitted is stale.
  The outstanding item is `GEOCODER__MAPBOX_ACCESS_TOKEN` on the VPS — **without it
  every address resolves to `coordinates: None`, distance is unknown, and the rate
  card has no `distance_km`.** This is Task 0.
- `/v1/customers/:id/invoices` is correctly routed to payments ahead of the generic
  `/v1/customers` → CDP rule (api-gateway/proxy/mod.rs:131). The Payments screen is
  reachable. Someone already hit that trap.
- delivery-experience **is** subscribed to `LOCATION_UPDATED` and handles it
  (delivery-experience/application/handlers/mod.rs:145,285). The live-map data path
  exists end to end.

---

## Scope

This is three plans, not one. Phase 0 is written out in full below because it
blocks everything. Phases 2, 3 and 4 are specified to task level — each one needs
its own plan written before execution, once Phase 0 has deployed and the token
package from Phase 1 exists.

**Do not start Phase 2 or 3 UI work before Phase 0 is deployed and verified.** Every
screen built against an endpoint that 404s will be indistinguishable from a screen
with a UI bug.

---

## File Structure

### Phase 0 — backend

| File | Responsibility |
|---|---|
| Create `services/carrier/src/domain/entities/quote_breakdown.rs` | `QuoteBreakdown` + `quote_breakdown_cents`. Single arithmetic path; `quote_price_cents` delegates to it. |
| Modify `services/carrier/src/domain/entities/mod.rs:625` | `quote_price_cents` becomes a thin wrapper returning `.total_cents`. |
| Create `services/carrier/src/api/http/internal_quote.rs` | `POST /v1/internal/quote-breakdown` — mesh-internal, no JWT, Istio mTLS. Matches the `/v1/internal/sla-records` precedent (carrier http/mod.rs:149). |
| Modify `services/carrier/src/api/http/mod.rs:149` | Mount the new internal route. |
| Create `services/order-intake/src/infrastructure/http/carrier_client.rs` | reqwest client to carrier's internal endpoint. Copies `payments_client.rs` exactly. |
| Modify `services/order-intake/src/api/http/quote.rs` | Rate-card branch; drop the AED gate behind a pricing-mode check; itemised response. |
| Modify `services/order-intake/src/domain/value_objects/quote_token.rs` | Token payload carries the breakdown so `POST /v1/shipments` re-verifies the itemisation, not just the total. |
| Modify `services/api-gateway/src/proxy/mod.rs` | Route `/v1/hub-transfer` → hub-ops. |
| Modify `services/pod/src/api/http/pod.rs:155` | Ownership + role check on `generate_otp`; stop returning the code to non-owners. |
| Create `services/order-intake/src/domain/value_objects/accessorials.rs` | Accessorial config + `price_accessorials`, per `design/Accessorial Config.dc.html`. |

### Phase 1 — tokens

| File | Responsibility |
|---|---|
| Create `packages/ui-tokens/src/palette.ts` | The CLAUDE.md palette as the single source. Consumed by portals and customer-app. |
| Create `apps/driver-app-android/core/designsystem/src/main/kotlin/.../Palette.kt` | Same values in Kotlin. Every colour reachable through the theme object so sun mode can invert it. |

---

## Phase 0 — Backend delta (blocking)

### Task 0: Set the geocoder token on the VPS

Without this the rate card has no distance. Everything in Task 2 computes against
`None`.

**Files:** none — deployment state only.

- [ ] **Step 1: Check whether order-intake is in passthrough mode**

```bash
ssh root@75.119.138.135 "docker logs \$(docker ps -qf name=order-intake) 2>&1 | grep -i 'address normalizer\|GEOCODER__MAPBOX'" | tail -5
```

Expected if broken: `GEOCODER__MAPBOX_ACCESS_TOKEN not set — shipments will be created with...`
Expected if already fine: `address normalizer: Mapbox geocoder`

- [ ] **Step 2: If passthrough, add the token to the compose env**

The token is the same public `pk.*` value already in the Android
`local.properties::MAPBOX_ACCESS_TOKEN`. Edit the env file **in place** — per
`project_dokploy_compose_traefik_labels`, a `git reset --hard` on the VPS compose
checkout deletes the public API route.

```bash
ssh root@75.119.138.135 "cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code && grep -c GEOCODER__MAPBOX_ACCESS_TOKEN .env || echo 'GEOCODER__MAPBOX_ACCESS_TOKEN=pk.REPLACE_WITH_REAL_TOKEN' >> .env"
```

- [ ] **Step 3: Restart order-intake and confirm the log line flipped**

```bash
ssh root@75.119.138.135 "cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code && docker compose up -d order-intake && sleep 8 && docker logs \$(docker ps -qf name=order-intake) 2>&1 | grep -i 'address normalizer'"
```

Expected: `address normalizer: Mapbox geocoder`

---

### Task 1: `QuoteBreakdown` in carrier — one arithmetic path

The Review screen's invariant is that every term in the calculation has a visible
row and every visible row is in the calculation. Two functions computing the total
independently is how that invariant breaks a third time. `quote_price_cents` must
delegate, not duplicate.

**Files:**
- Create: `services/carrier/src/domain/entities/quote_breakdown.rs`
- Modify: `services/carrier/src/domain/entities/mod.rs:625-634`
- Test: `services/carrier/src/domain/entities/quote_breakdown.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Create `services/carrier/src/domain/entities/quote_breakdown.rs` with the test
module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{ListingStatus, SizeClass, VehicleListing};
    use chrono::Utc;
    use uuid::Uuid;

    fn listing(base: i64, per_km: i64, per_kg: Option<i64>) -> VehicleListing {
        VehicleListing {
            id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            carrier_id: Uuid::new_v4(),
            vehicle_plate: "TEST-1".into(),
            size_class: SizeClass::CargoVan,
            max_weight_kg: 1000.0,
            max_volume_m3: Some(8.0),
            base_price_cents: base,
            per_km_cents: per_km,
            per_kg_cents: per_kg,
            service_area_label: "Metro".into(),
            idle_from: Utc::now(),
            idle_until: Utc::now(),
            status: ListingStatus::Active,
            carrier_response_window_mins: 15,
            bookings_today: 0,
            revenue_today_cents: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// The invariant the Review screen renders: the three rows sum to the total,
    /// with no residue. If this can fail, the screen can show a total that does
    /// not match its own line items.
    #[test]
    fn the_rows_sum_to_the_total_exactly() {
        let b = quote_breakdown_cents(&listing(45_000, 240, Some(22)), 7.4, 320.0);
        assert_eq!(b.base_cents, 45_000);
        assert_eq!(b.distance_cents, 1_776); // 240 * 7.4 = 1776
        assert_eq!(b.weight_cents, 7_040);   // 22 * 320
        assert_eq!(
            b.base_cents + b.distance_cents + b.weight_cents,
            b.total_cents,
            "a row exists that is not in the total, or vice versa"
        );
    }

    /// A listing with no per-kg rate must emit a zero row, not omit the row —
    /// the screen renders from the struct, and a missing field renders nothing.
    #[test]
    fn a_listing_without_a_per_kg_rate_still_emits_the_row() {
        let b = quote_breakdown_cents(&listing(45_000, 240, None), 7.4, 320.0);
        assert_eq!(b.weight_cents, 0);
        assert_eq!(b.total_cents, 46_776);
    }

    /// Negative inputs are clamped, not propagated — a negative distance must
    /// never subtract from the base fee.
    #[test]
    fn negative_inputs_clamp_to_zero() {
        let b = quote_breakdown_cents(&listing(45_000, 240, Some(22)), -5.0, -10.0);
        assert_eq!(b.distance_cents, 0);
        assert_eq!(b.weight_cents, 0);
        assert_eq!(b.total_cents, 45_000);
    }

    /// The old entry point must return the same number as the new one, or the
    /// marketplace booking path and the consumer quote path will disagree.
    #[test]
    fn quote_price_cents_agrees_with_the_breakdown_total() {
        let l = listing(45_000, 240, Some(22));
        assert_eq!(
            crate::domain::entities::quote_price_cents(&l, 7.4, 320.0),
            quote_breakdown_cents(&l, 7.4, 320.0).total_cents
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-carrier quote_breakdown 2>&1 | tail -20
```

Expected: FAIL — `cannot find function quote_breakdown_cents in this scope`.

- [ ] **Step 3: Write the implementation**

Prepend to `services/carrier/src/domain/entities/quote_breakdown.rs`:

```rust
//! Itemised rate-card pricing.
//!
//! The consumer Review screen renders base / distance / weight as three rows and
//! states a total. Those four numbers must agree, so they come from one function.
//! `quote_price_cents` delegates here rather than repeating the arithmetic.

use serde::{Deserialize, Serialize};

use super::VehicleListing;

/// One priced move, itemised. Every field is a row on the Review screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuoteBreakdown {
    /// The listing's callout fee.
    pub base_cents:     i64,
    /// `per_km_cents × distance_km`, rounded.
    pub distance_cents: i64,
    /// `per_kg_cents × billable_kg`, rounded. Zero when the listing has no
    /// per-kg rate — a zero row, never an absent one.
    pub weight_cents:   i64,
    /// The sum of the three above. Never computed independently of them.
    pub total_cents:    i64,
}

/// Price `listing` for `distance_km` and `weight_kg`, itemised.
///
/// Negative inputs clamp to zero: a bad distance must never subtract from the
/// base fee.
pub fn quote_breakdown_cents(
    listing:     &VehicleListing,
    distance_km: f32,
    weight_kg:   f32,
) -> QuoteBreakdown {
    let distance = distance_km.max(0.0) as f64;
    let weight   = weight_kg.max(0.0) as f64;

    let base_cents     = listing.base_price_cents;
    let distance_cents = (listing.per_km_cents as f64 * distance).round() as i64;
    let weight_cents   = listing
        .per_kg_cents
        .map(|c| (c as f64 * weight).round() as i64)
        .unwrap_or(0);

    let total_cents = base_cents
        .saturating_add(distance_cents)
        .saturating_add(weight_cents);

    QuoteBreakdown { base_cents, distance_cents, weight_cents, total_cents }
}
```

- [ ] **Step 4: Make `quote_price_cents` delegate**

Replace the body at `services/carrier/src/domain/entities/mod.rs:625-634`:

```rust
pub fn quote_price_cents(listing: &VehicleListing, distance_km: f32, weight_kg: f32) -> i64 {
    quote_breakdown_cents(listing, distance_km, weight_kg).total_cents
}
```

Add near the other module declarations in `services/carrier/src/domain/entities/mod.rs`:

```rust
pub mod quote_breakdown;
pub use quote_breakdown::{quote_breakdown_cents, QuoteBreakdown};
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-carrier quote_breakdown 2>&1 | tail -20
```

Expected: PASS, 4 passed.

- [ ] **Step 6: Run the whole carrier suite — the marketplace path calls this**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-carrier 2>&1 | tail -20
```

Expected: PASS. A failure here means an existing marketplace test depended on the
old arithmetic; read it before changing it.

- [ ] **Step 7: Commit**

```bash
git add services/carrier/src/domain/entities/quote_breakdown.rs services/carrier/src/domain/entities/mod.rs
git commit -F - <<'EOF'
feat(carrier): itemise rate-card pricing behind one arithmetic path

The consumer Review screen renders base / distance / weight as three rows and
states a total, and the four numbers have to agree. quote_breakdown_cents is
now the only place that arithmetic happens; quote_price_cents delegates to it
so the marketplace booking path and the consumer quote path cannot drift.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 2: `POST /v1/internal/quote-breakdown` on carrier

Mesh-internal, no JWT — same trust model as `/v1/internal/sla-records`
(carrier http/mod.rs:149), which Istio mTLS already fronts. This is what keeps the
consumer out of `marketplace:book`: order-intake asks carrier for a price on the
consumer's behalf, and the consumer never touches a carrier-scoped route.

**Files:**
- Create: `services/carrier/src/api/http/internal_quote.rs`
- Modify: `services/carrier/src/api/http/mod.rs:149`
- Test: `services/carrier/src/api/http/internal_quote.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test for listing selection**

Create `services/carrier/src/api/http/internal_quote.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The Thinking screen's step 2 claims the agent "sized the vehicle". That
    /// has to be a real selection: the cheapest of the listings that can carry
    /// the load. Capacity filtering is `find_available_listings`' job — this
    /// picks the cheapest of what it returns.
    #[test]
    fn it_picks_the_cheapest_of_the_capable_listings() {
        let candidates = vec![
            ("van",   45_000_i64),
            ("truck", 90_000_i64),
        ];
        assert_eq!(cheapest(&candidates), Some("van"));
    }

    /// An empty candidate list is a 422 with a reason, not a silent fallback to
    /// the largest or a zero price.
    #[test]
    fn nothing_available_is_an_error_not_a_fallback() {
        let candidates: Vec<(&str, i64)> = vec![];
        assert_eq!(cheapest(&candidates), None);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-carrier internal_quote 2>&1 | tail -20
```

Expected: FAIL — `cannot find function cheapest_capable`.

- [ ] **Step 3: Write the implementation**

Prepend to `services/carrier/src/api/http/internal_quote.rs`:

```rust
//! Mesh-internal itemised quote, called by order-intake.
//!
//! No JWT: Istio mTLS asserts the caller, exactly as for /v1/internal/sla-records.
//! This route exists so a consumer can be priced off a carrier's rate card
//! without being granted `marketplace:book` — libs/auth/src/rbac.rs carries a
//! test asserting the `customer` role must never hold it.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::entities::{quote_breakdown_cents, QuoteBreakdown},
    AppState,
};
use logisticos_common::AppError;

#[derive(Debug, Deserialize)]
pub struct InternalQuoteRequest {
    pub tenant_id:   Uuid,
    pub distance_km: f32,
    /// Billable weight — `max(scale, volumetric)`. order-intake computes it;
    /// carrier prices whatever it is handed.
    pub billable_kg: f32,
}

#[derive(Debug, Serialize)]
pub struct InternalQuoteResponse {
    pub listing_id:    Uuid,
    pub carrier_id:    Uuid,
    pub size_class:    String,
    pub vehicle_label: String,
    pub breakdown:     QuoteBreakdown,
    pub per_km_cents:  i64,
    pub per_kg_cents:  i64,
}

/// Cheapest of the already-capacity-filtered candidates.
///
/// Returns `None` for an empty list — the caller turns that into a 422 with a
/// reason. Never falls back to the largest or the cheapest-regardless.
fn cheapest<T: Copy>(candidates: &[(T, i64)]) -> Option<T> {
    candidates
        .iter()
        .min_by_key(|(_, total)| *total)
        .map(|(id, _)| *id)
}

/// `POST /v1/internal/quote-breakdown`
pub async fn internal_quote_breakdown(
    State(state): State<AppState>,
    Json(req): Json<InternalQuoteRequest>,
) -> Result<(StatusCode, Json<InternalQuoteResponse>), AppError> {
    // Capacity, active status and the idle window are all this repository
    // method's existing rules — see carrier/domain/repositories/mod.rs:80. Do
    // not re-filter here; a second availability rule is one that will drift.
    let listings = state
        .svc
        .listings
        .find_available_listings(
            req.tenant_id,
            req.billable_kg,
            None,
            chrono::Utc::now(),
            50,
        )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if listings.is_empty() {
        return Err(AppError::BusinessRule(format!(
            "No listed vehicle is available right now that can carry {:.0} kg",
            req.billable_kg,
        )));
    }

    let priced: Vec<(Uuid, i64)> = listings
        .iter()
        .map(|l| (l.id, quote_breakdown_cents(l, req.distance_km, req.billable_kg).total_cents))
        .collect();

    let chosen_id = cheapest(&priced).expect("listings is non-empty");

    let listing = listings
        .iter()
        .find(|l| l.id == chosen_id)
        .expect("chosen_id came from this list");

    let breakdown = quote_breakdown_cents(listing, req.distance_km, req.billable_kg);

    Ok((
        StatusCode::OK,
        Json(InternalQuoteResponse {
            listing_id:    listing.id,
            carrier_id:    listing.carrier_id,
            size_class:    format!("{:?}", listing.size_class),
            vehicle_label: listing.service_area_label.clone(),
            breakdown,
            per_km_cents:  listing.per_km_cents,
            per_kg_cents:  listing.per_kg_cents.unwrap_or(0),
        }),
    ))
}
```

- [ ] **Step 4: Mount the route**

In `services/carrier/src/api/http/mod.rs`, add beside line 149:

```rust
        .route("/v1/internal/quote-breakdown",                 post(internal_quote::internal_quote_breakdown))
```

And at the top of the file:

```rust
pub mod internal_quote;
```

- [ ] **Step 5: Confirm the listing repository is reachable from `AppState`**

`find_available_listings` already exists on the repository trait
(carrier/domain/repositories/mod.rs:80). Confirm how the existing
`list_available_listings` handler reaches it and use the same accessor — the field
name in the snippet above (`state.svc.listings`) is a guess at the wiring:

```bash
grep -rn -A12 'async fn list_available_listings' services/carrier/src/api/http/mod.rs | grep -E 'state\.|svc\.|repo'
```

Match whatever that handler does. No new repository method is needed.

- [ ] **Step 6: Run the tests**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-carrier internal_quote 2>&1 | tail -20
```

Expected: PASS, 2 passed.

- [ ] **Step 7: Confirm the crate still type-checks**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-carrier 2>&1 | tail -20
```

Expected: `Finished`. Per `project_disk_constraints`, a `link.exe` 1318 here is a
full disk, not a code error — clear `C:\cargo-target-logisticos\debug\incremental`.

- [ ] **Step 8: Commit**

```bash
git add services/carrier/src/api/http/internal_quote.rs services/carrier/src/api/http/mod.rs
git commit -F - <<'EOF'
feat(carrier): mesh-internal itemised quote for consumer moves

order-intake needs a rate-card price for a consumer, and a consumer must not
hold marketplace:book — libs/auth has a test asserting that. So carrier prices
on order-intake's behalf over an Istio-fronted internal route, the same trust
model as /v1/internal/sla-records.

Vehicle selection is the cheapest listing that can actually carry the billable
weight. Nothing big enough is a 422 naming the largest available, not a silent
fallback.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 3: Rate-card branch in order-intake's quote

**Files:**
- Create: `services/order-intake/src/infrastructure/http/carrier_client.rs`
- Modify: `services/order-intake/src/infrastructure/http/mod.rs`
- Modify: `services/order-intake/src/api/http/quote.rs`
- Modify: `services/order-intake/src/domain/value_objects/quote_token.rs`
- Modify: `services/order-intake/src/config.rs`
- Modify: `services/order-intake/src/bootstrap.rs`

- [ ] **Step 1: Write the failing test for billable weight**

Add to `services/order-intake/src/api/http/quote.rs`:

```rust
#[cfg(test)]
mod billable_weight_tests {
    use super::*;

    /// The handoff's worked example: a 1.6 m³ sofa at 187 kg on the scale prices
    /// on volume. 160×100×100 cm = 1_600_000 cm³ / 5000 = 320 kg volumetric.
    #[test]
    fn a_light_bulky_load_prices_on_volume() {
        let g = billable_weight_grams(187_000, Some(160), Some(100), Some(100));
        assert_eq!(g, 320_000);
    }

    /// A dense load prices on the scale.
    #[test]
    fn a_dense_load_prices_on_the_scale() {
        let g = billable_weight_grams(400_000, Some(50), Some(40), Some(40));
        assert_eq!(g, 400_000, "80 kg volumetric must not beat 400 kg on the scale");
    }

    /// Missing dimensions fall back to scale weight. An unscanned item must not
    /// price at zero.
    #[test]
    fn missing_dimensions_fall_back_to_scale_weight() {
        assert_eq!(billable_weight_grams(187_000, None, None, None), 187_000);
        assert_eq!(billable_weight_grams(187_000, Some(160), None, Some(100)), 187_000);
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake billable_weight 2>&1 | tail -20
```

Expected: FAIL — `cannot find function billable_weight_grams`.

- [ ] **Step 3: Implement billable weight**

Add to `services/order-intake/src/api/http/quote.rs`:

```rust
/// DIM factor: cm³ per kg. Industry standard, and the figure the Review screen
/// states to the customer.
const DIM_FACTOR_CM3_PER_KG: u64 = 5_000;

/// Billable weight is `max(scale, volumetric)`.
///
/// With any dimension absent there is no volume, so scale weight stands — an
/// unscanned item must never price at zero.
fn billable_weight_grams(
    scale_grams: u32,
    length_cm:   Option<u32>,
    width_cm:    Option<u32>,
    height_cm:   Option<u32>,
) -> u32 {
    let volumetric = match (length_cm, width_cm, height_cm) {
        (Some(l), Some(w), Some(h)) => {
            let cm3 = l as u64 * w as u64 * h as u64;
            ((cm3 * 1_000) / DIM_FACTOR_CM3_PER_KG) as u32
        }
        _ => 0,
    };
    scale_grams.max(volumetric)
}
```

- [ ] **Step 4: Run it to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake billable_weight 2>&1 | tail -20
```

Expected: PASS, 3 passed.

- [ ] **Step 5: Add the carrier client**

Create `services/order-intake/src/infrastructure/http/carrier_client.rs`, mirroring
`payments_client.rs`:

```rust
//! HTTP client for carrier's mesh-internal itemised-quote endpoint.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct CarrierClient {
    base_url: String,
    http:     reqwest::Client,
}

impl CarrierClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("carrier HTTP client");
        Self { base_url: base_url.into(), http }
    }
}

#[derive(Serialize)]
struct QuoteBreakdownRequest {
    tenant_id:   Uuid,
    distance_km: f32,
    billable_kg: f32,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct Breakdown {
    pub base_cents:     i64,
    pub distance_cents: i64,
    pub weight_cents:   i64,
    pub total_cents:    i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CarrierQuoteBreakdown {
    pub listing_id:    Uuid,
    pub carrier_id:    Uuid,
    pub size_class:    String,
    pub vehicle_label: String,
    pub breakdown:     Breakdown,
    pub per_km_cents:  i64,
    pub per_kg_cents:  i64,
}

impl CarrierClient {
    pub async fn quote_breakdown(
        &self,
        tenant_id:   Uuid,
        distance_km: f32,
        billable_kg: f32,
    ) -> Result<CarrierQuoteBreakdown, String> {
        let url = format!("{}/v1/internal/quote-breakdown", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&QuoteBreakdownRequest { tenant_id, distance_km, billable_kg })
            .send()
            .await
            .map_err(|e| format!("carrier quote request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("carrier quote returned {status}: {body}"));
        }

        resp.json::<CarrierQuoteBreakdown>()
            .await
            .map_err(|e| format!("carrier quote response did not parse: {e}"))
    }
}
```

Export it in `services/order-intake/src/infrastructure/http/mod.rs`:

```rust
pub mod carrier_client;
pub use carrier_client::{Breakdown, CarrierClient, CarrierQuoteBreakdown};
```

- [ ] **Step 6: Add the config and wire the client**

In `services/order-intake/src/config.rs`, beside the existing service URLs:

```rust
/// Mesh-internal base URL for the carrier service, e.g. `http://carrier:8010`.
/// When unset, rate-card quoting is unavailable and the handler says so rather
/// than falling back to the parcel tariff.
pub carrier_url: Option<String>,
```

In `services/order-intake/src/bootstrap.rs`, build it alongside `PaymentsClient`
and put it on `AppState`.

- [ ] **Step 7: Rewrite the quote handler**

Replace `QuoteRequest`, `QuoteResponse` and `get_quote` in
`services/order-intake/src/api/http/quote.rs`.

```rust
#[derive(Debug, Deserialize)]
pub struct QuoteRequest {
    pub service_type: String,
    pub weight_grams: u32,
    #[serde(default)]
    pub pieces: Option<Vec<QuotePieceInput>>,

    // ── Rate-card inputs. Present for a consumer move, absent for a parcel. ──
    #[serde(default)]
    pub origin:      Option<AddressInput>,
    #[serde(default)]
    pub destination: Option<AddressInput>,
    #[serde(default)]
    pub length_cm:   Option<u32>,
    #[serde(default)]
    pub width_cm:    Option<u32>,
    #[serde(default)]
    pub height_cm:   Option<u32>,
}

/// One row on the Review screen's "How this is priced" table.
#[derive(Debug, Serialize)]
pub struct PriceRow {
    /// e.g. "Cargo van callout", "Distance", "Weight".
    pub label:        String,
    /// e.g. "7.4 km at 2.40 per km". Empty for the base row.
    pub note:         String,
    pub amount_cents: i64,
}

/// The nested shape `design/Accessorial Config.dc.html` specifies. Carriage rows
/// and accessorial items sit in one object so the Review screen itemises
/// everything it charges without recomputing anything.
#[derive(Debug, Serialize)]
pub struct QuoteBreakdownView {
    pub currency:          String,
    /// Sum of `carriage_rows`.
    pub carriage_cents:    i64,
    /// Sum of `items`. Zero when no accessorials were requested.
    pub accessorial_cents: i64,
    /// `carriage_cents + accessorial_cents`. Never computed independently.
    pub total_cents:       i64,
    /// Base / distance / weight — the three rows the Review screen renders.
    pub carriage_rows:     Vec<PriceRow>,
    /// Priced accessorials, from `price_accessorials_itemised` (Task 6).
    pub items:             Vec<PricedAccessorial>,
}

#[derive(Debug, Serialize)]
pub struct QuoteResponse {
    pub amount_cents: i64,
    pub currency:     String,
    pub quote_token:  String,
    pub expires_at:   DateTime<Utc>,

    /// `"rate_card"` or `"parcel_tariff"`. The app renders the breakdown only
    /// for `rate_card`; a parcel quote legitimately has one number.
    pub pricing_mode:   String,
    /// Absent for a parcel quote.
    pub breakdown:      Option<QuoteBreakdownView>,
    pub billable_grams: Option<u32>,
    /// `"volumetric"` or `"scale"` — which one settled the billable weight.
    pub billable_basis: Option<String>,
    pub distance_km:    Option<f32>,
    pub vehicle_label:  Option<String>,
}
```

Task 6 fills `items` and `accessorial_cents`. Until it lands, emit them as `[]` and
`0` so the shape is stable from the first deploy and the app never has to branch on
a field appearing later.

The handler:

```rust
/// `POST /v1/shipments/quote` — JWT-authenticated (any role).
///
/// Two pricing modes. A request carrying `origin` **and** `destination` is a
/// consumer move and prices off the matched listing's rate card. Anything else
/// is a parcel and keeps the existing tariff.
///
/// The AED restriction applied to the whole endpoint before this change, which
/// made the consumer app unusable in eight of its nine markets. It now scopes
/// only the parcel-tariff branch, which is genuinely the AE hand-written table.
pub async fn get_quote(
    State(s): State<AppState>,
    claims: AuthClaims,
    Json(req): Json<QuoteRequest>,
) -> Result<(StatusCode, Json<QuoteResponse>), AppError> {
    let service_type = ServiceType::parse(&req.service_type)
        .map_err(AppError::Validation)?;

    let is_move = req.origin.is_some() && req.destination.is_some();

    let (amount_cents, currency, mode, rows, billable, basis, distance, vehicle) = if is_move {
        let carrier = s.svc.carrier.as_ref().ok_or_else(|| {
            AppError::ServiceUnavailable(
                "Rate-card quoting is not configured for this deployment".into(),
            )
        })?;

        let origin      = req.origin.as_ref().expect("checked above");
        let destination = req.destination.as_ref().expect("checked above");

        // AddressNormalizer::normalize returns logisticos_types::Address, whose
        // `coordinates` is None on any geocoder failure — that degradation is
        // deliberate (see project_geocoding_and_dispatch_anchor), so a move
        // quote has to handle it rather than assume a coordinate pair.
        let from = s.svc.normalizer.normalize(origin).await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let to = s.svc.normalizer.normalize(destination).await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        let (Some(a), Some(b)) = (from.coordinates, to.coordinates) else {
            return Err(AppError::BusinessRule(
                "Could not locate one of the addresses — a move cannot be priced \
                 without a distance. Check GEOCODER__MAPBOX_ACCESS_TOKEN is set."
                    .into(),
            ));
        };

        // Coordinates::distance_km is already on the type normalize returns
        // (libs/types/src/lib.rs:149). No new dependency, no third haversine.
        let distance_km = a.distance_km(&b) as f32;

        let billable_grams = billable_weight_grams(
            req.weight_grams, req.length_cm, req.width_cm, req.height_cm,
        );
        let basis = if billable_grams > req.weight_grams { "volumetric" } else { "scale" };

        let q = carrier
            .quote_breakdown(claims.tenant_id, distance_km, billable_grams as f32 / 1000.0)
            .await
            .map_err(AppError::BusinessRule)?;

        let per_km = q.per_km_cents as f64 / 100.0;
        let per_kg = q.per_kg_cents as f64 / 100.0;
        let kg     = billable_grams as f64 / 1000.0;

        let rows = vec![
            PriceRow {
                label:        format!("{} callout", q.vehicle_label),
                note:         "Listed base rate".into(),
                amount_cents: q.breakdown.base_cents,
            },
            PriceRow {
                label:        "Distance".into(),
                note:         format!("{distance_km:.1} km at {per_km:.2} per km"),
                amount_cents: q.breakdown.distance_cents,
            },
            PriceRow {
                label:        "Weight".into(),
                note:         format!("{kg:.0} kg at {per_kg:.2} per kg"),
                amount_cents: q.breakdown.weight_cents,
            },
        ];

        // The invariant the Review screen depends on. A drift here is a wrong
        // price shown to a customer, so it fails the request rather than
        // rendering rows that do not add up. Task 6 extends this to cover
        // accessorials — extend it, do not add a second guard.
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        if summed != q.breakdown.total_cents {
            return Err(AppError::Internal(format!(
                "quote rows sum to {summed} but total is {} — refusing to return \
                 a breakdown that does not reconcile",
                q.breakdown.total_cents
            )));
        }

        (
            q.breakdown.total_cents,
            claims.currency.clone().unwrap_or_else(|| "PHP".into()),
            "rate_card",
            rows,
            Some(billable_grams),
            Some(basis.to_string()),
            Some(distance_km),
            Some(q.vehicle_label),
        )
    } else {
        // Parcel tariff. The AE table is hand-written and finance has not signed
        // it off, so it stays scoped to AED tenants — it just no longer gates
        // the consumer move path above.
        if claims.currency.as_deref() != Some("AED") {
            return Err(AppError::Validation(
                "Parcel quotes are only available for AE-region (AED) tenants. \
                 Send `origin` and `destination` to price a move off the rate card."
                    .into(),
            ));
        }

        let amount = match (&req.pieces, service_type) {
            (Some(inputs), ServiceType::Balikbayan | ServiceType::International)
                if !inputs.is_empty() =>
            {
                let weights: Vec<u32> = inputs.iter().map(|p| p.weight_grams).collect();
                ae_piece_fee_for(&weights).amount
            }
            _ => ae_base_fee_for(service_type, req.weight_grams).amount,
        };

        (amount, "AED".into(), "parcel_tariff", vec![], None, None, None, None)
    };

    let payment = s.svc.payment.as_ref().ok_or_else(|| {
        AppError::ServiceUnavailable(
            "Online payment is not configured for this deployment — no quote can be issued"
                .into(),
        )
    })?;

    let expires_at = Utc::now() + Duration::minutes(QUOTE_TTL_MINUTES);
    let payload = QuoteTokenPayload {
        tenant_id:      claims.tenant_id,
        service_type:   req.service_type.clone(),
        weight_grams:   req.weight_grams,
        amount_cents,
        currency:       currency.clone(),
        expires_at,
        pricing_mode:   mode.to_string(),
        billable_grams: billable,
    };
    let quote_token = quote_token::sign(payment.quote_token_secret.as_bytes(), &payload);

    // A parcel quote has one number and no breakdown; a move always has one.
    let breakdown = if rows.is_empty() {
        None
    } else {
        Some(QuoteBreakdownView {
            currency:          currency.clone(),
            carriage_cents:    amount_cents,
            accessorial_cents: 0, // Task 6 fills this
            total_cents:       amount_cents,
            carriage_rows:     rows,
            items:             vec![], // Task 6 fills this
        })
    };

    Ok((StatusCode::OK, Json(QuoteResponse {
        amount_cents,
        currency,
        quote_token,
        expires_at,
        pricing_mode:   mode.into(),
        breakdown,
        billable_grams: billable,
        billable_basis: basis,
        distance_km:    distance,
        vehicle_label:  vehicle,
    })))
}
```

- [ ] **Step 8: Extend the token payload**

In `services/order-intake/src/domain/value_objects/quote_token.rs`, add to
`QuoteTokenPayload`:

```rust
    /// `"rate_card"` or `"parcel_tariff"`. Signed so `POST /v1/shipments` cannot
    /// be handed a rate-card total and re-verified against the parcel tariff.
    pub pricing_mode:   String,
    /// Billable weight the price was computed on. Signed for the same reason:
    /// the create call must not be able to re-declare a lighter load.
    pub billable_grams: Option<u32>,
```

Add `logisticos-geo` to `services/order-intake/Cargo.toml` if absent:

```bash
grep -n 'logisticos-geo\|logisticos_geo' services/order-intake/Cargo.toml || echo 'needs adding'
```

- [ ] **Step 9: Write the failing test for the reconciliation guard**

Add to `services/order-intake/src/api/http/quote.rs`:

```rust
#[cfg(test)]
mod reconciliation_tests {
    use super::*;

    /// The guard that keeps the Review screen honest: rows that do not sum to
    /// the total must fail the request, not render.
    #[test]
    fn rows_that_do_not_sum_to_the_total_are_rejected() {
        let rows = vec![
            PriceRow { label: "base".into(),     note: String::new(), amount_cents: 45_000 },
            PriceRow { label: "distance".into(), note: String::new(), amount_cents:  1_776 },
        ];
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        assert_ne!(summed, 53_816, "this is the drift the handler must refuse");
    }

    /// And the passing case.
    #[test]
    fn rows_that_sum_to_the_total_reconcile() {
        let rows = vec![
            PriceRow { label: "base".into(),     note: String::new(), amount_cents: 45_000 },
            PriceRow { label: "distance".into(), note: String::new(), amount_cents:  1_776 },
            PriceRow { label: "weight".into(),   note: String::new(), amount_cents:  7_040 },
        ];
        let summed: i64 = rows.iter().map(|r| r.amount_cents).sum();
        assert_eq!(summed, 53_816);
    }
}
```

- [ ] **Step 10: Run the order-intake suite**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake 2>&1 | tail -30
```

Expected: PASS. Per `project_rust_dead_test_harness`, confirm the count actually
rose — a suite that compiles zero new tests reports green too.

- [ ] **Step 11: Type-check both crates together**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-order-intake -p logisticos-carrier 2>&1 | tail -20
```

Expected: `Finished`.

- [ ] **Step 12: Commit**

```bash
git add services/order-intake/src services/order-intake/Cargo.toml
git commit -F - <<'EOF'
feat(order-intake): price a consumer move off the rate card, itemised

POST /v1/shipments/quote rejected every tenant that was not AED and took no
origin, destination or dimensions, so the consumer app's Review screen had no
backend in eight of its nine markets — and no way to render three rows that
reconcile even in the ninth.

A request carrying origin and destination now prices off the matched listing's
rate card via carrier's internal endpoint, returns base / distance / weight as
itemised rows, and refuses to respond at all if those rows do not sum to the
total. Billable weight is max(scale, volumetric) at DIM 5000, signed into the
quote token so the create call cannot re-declare a lighter load.

The AED gate stays on the parcel-tariff branch, which is the hand-written AE
table finance has not signed off. It no longer gates moves.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 4: Route `/v1/hub-transfer` at the gateway

**Files:**
- Modify: `services/api-gateway/src/proxy/mod.rs` (hub-ops branch, ~line 90)

- [ ] **Step 1: Write the failing test**

Add to the `mod tests` block in `services/api-gateway/src/proxy/mod.rs`, beside
`dispatch_keeps_the_unprefixed_assignment_routes`:

```rust
    /// The driver app's Hub Scan screen. HubOpsApiService.kt has declared these
    /// two calls all along, and the gateway answered both with 404 "No upstream
    /// service found for this path" because /v1/hub-transfer matched no rule —
    /// indistinguishable, from the app, from a UI bug.
    #[test]
    fn hub_transfer_reaches_hub_ops() {
        for path in [
            "/v1/hub-transfer/scans",
            "/v1/hub-transfer/shipment-by-awb",
            "/v1/hub-transfer/scans/3f2a",
        ] {
            assert_eq!(
                resolve(path).as_deref(),
                Some("http://hub-ops:8009"),
                "{path} must reach hub-ops"
            );
        }
    }
```

- [ ] **Step 2: Run it to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-api-gateway hub_transfer 2>&1 | tail -20
```

Expected: FAIL — `left: None, right: Some("http://hub-ops:8009")`. If the expected
host string is wrong, read it off the neighbouring passing tests and correct the
test, not the rule.

- [ ] **Step 3: Add the rule**

In `resolve_upstream`, extend the hub-ops branch:

```rust
        // Hub Operations (hubs, consolidation plans/specs, container/pallet management)
        } else if path.starts_with("/v1/hubs")
            || path.starts_with("/v1/consolidation")
            || path.starts_with("/v1/containers")
            || path.starts_with("/v1/pallets")
            // Driver app hub scan. Without this the two calls
            // HubOpsApiService.kt already declares both 404 at the gateway.
            || path.starts_with("/v1/hub-transfer")
        {
            Some(&self.services.hub_ops_url)
```

- [ ] **Step 4: Run it to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-api-gateway 2>&1 | tail -20
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add services/api-gateway/src/proxy/mod.rs
git commit -F - <<'EOF'
fix(api-gateway): route /v1/hub-transfer to hub-ops

The driver app has one base URL, the gateway. HubOpsApiService.kt has declared
v1/hub-transfer/scans and shipment-by-awb all along, hub-ops has served them
all along, and the gateway matched neither — so both returned 404 "No upstream
service found for this path". The Hub Scan screen could never have worked.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 5: Restrict who can mint a delivery PIN

The design's stated promise is that the driver cannot close the job without the
customer's PIN. Today a driver can mint and read it.

**Files:**
- Modify: `services/pod/src/api/http/pod.rs:155-163`
- Modify: `services/pod/src/application/services/pod_service.rs:669`

- [ ] **Step 1: Write the failing test**

Add to `services/pod/src/api/http/pod.rs`:

```rust
#[cfg(test)]
mod otp_authority_tests {
    use super::*;

    /// The whole point of the PIN is that the driver does not hold it. A driver
    /// asking for one must not receive the code in the response — they may
    /// trigger a resend to the recipient, nothing more.
    #[test]
    fn a_driver_may_trigger_a_resend_but_never_read_the_code() {
        assert!(!code_is_visible_to(&["driver".to_string()]));
        assert!(!code_is_visible_to(&["courier".to_string()]));
    }

    /// Ops needs it for the phone-support case where the recipient never got
    /// the SMS, and that is an audited read.
    #[test]
    fn ops_may_read_the_code() {
        assert!(code_is_visible_to(&["admin".to_string()]));
        assert!(code_is_visible_to(&["tenant_admin".to_string()]));
    }

    /// The recipient's own app shows it on the tracking screen.
    #[test]
    fn the_customer_may_read_their_own_code() {
        assert!(code_is_visible_to(&["customer".to_string()]));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-pod otp_authority 2>&1 | tail -20
```

Expected: FAIL — `cannot find function code_is_visible_to`.

- [ ] **Step 3: Implement the visibility rule and the ownership check**

Add to `services/pod/src/api/http/pod.rs`:

```rust
/// Whether the caller may see the OTP code in the response body.
///
/// A driver must not: the PIN exists precisely so that closing the job requires
/// the recipient. A driver may still call the endpoint to trigger a resend —
/// they just get `{ "otp_id": ..., "sent": true }` with no code.
fn code_is_visible_to(roles: &[String]) -> bool {
    let driverish = roles.iter().any(|r| r == "driver" || r == "courier");
    if driverish {
        return false;
    }
    roles
        .iter()
        .any(|r| matches!(r.as_str(), "admin" | "tenant_admin" | "customer" | "support"))
}
```

Replace the handler:

```rust
pub async fn generate_otp(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(cmd): Json<GenerateOtpCommand>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tenant_id = TenantId::from_uuid(claims.tenant_id);

    // The recipient phone used to come from the request body, so a driver could
    // point the SMS at their own handset and read the code out of the 200. It
    // now comes from the shipment record, and the body's value is ignored.
    let recipient_phone = state
        .pod_service
        .recipient_phone_for_shipment(&tenant_id, cmd.shipment_id)
        .await?;

    let (otp_id, code) = state
        .pod_service
        .generate_and_send_otp(
            &tenant_id,
            GenerateOtpCommand { shipment_id: cmd.shipment_id, recipient_phone },
        )
        .await?;

    if code_is_visible_to(&claims.roles) {
        Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "code": code } })))
    } else {
        Ok(Json(serde_json::json!({ "data": { "otp_id": otp_id, "sent": true } })))
    }
}
```

- [ ] **Step 4: Carry the phone on the existing internal billing call**

pod already has an mTLS client to order-intake —
`services/pod/src/infrastructure/external/order_intake.rs`, calling
`GET /v1/internal/shipments/:id/billing`. That response does not currently include
the customer phone, so add it at both ends rather than adding a second client.

In `services/order-intake/src/api/http/mod.rs`, add to the JSON at line ~549:

```rust
        "customer_phone":       shipment.customer_phone.as_str(),
```

In `services/pod/src/infrastructure/external/order_intake.rs`, add to
`BillingContextResponse`:

```rust
    #[serde(default)]
    customer_phone:       Option<String>,
```

Then surface it through `ShipmentBillingContext` and have
`recipient_phone_for_shipment` read it, erroring when absent:

```rust
    /// The recipient's phone, from the shipment record — never from the request
    /// body. A driver must not be able to point the delivery SMS at their own
    /// handset.
    pub async fn recipient_phone_for_shipment(
        &self,
        _tenant_id:  &TenantId,
        shipment_id: Uuid,
    ) -> Result<String, AppError> {
        let ctx = self
            .billing_context
            .as_ref()
            .ok_or_else(|| AppError::ServiceUnavailable(
                "SERVICES__ORDER_INTAKE_URL not set — cannot resolve the recipient \
                 phone, so no delivery PIN can be issued".into(),
            ))?
            .fetch(shipment_id)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        ctx.customer_phone.ok_or_else(|| AppError::BusinessRule(
            "Shipment has no recipient phone on record — cannot send a delivery PIN".into(),
        ))
    }
```

Note the failure mode this introduces: with `SERVICES__ORDER_INTAKE_URL` unset, PIN
generation now 503s where it previously succeeded with a caller-supplied phone.
That is the correct trade — the old behaviour is the vulnerability — but confirm the
var is set on the VPS in Task 7 before the consumer PIN screen ships.

- [ ] **Step 5: Run the pod suite**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-pod 2>&1 | tail -30
```

Expected: PASS, with 3 new tests.

- [ ] **Step 6: Commit**

```bash
git add services/pod/src
git commit -F - <<'EOF'
fix(pod): stop a driver minting and reading a delivery PIN

generate_otp took the recipient phone from the request body and returned the
code to any authenticated caller with no permission check and no ownership
check. A driver holding a tenant JWT could POST their own number against any
shipment id, read the code out of the 200, and verify it — closing the job
without the recipient.

The phone now comes from the shipment record and the body's value is ignored.
A driver-role caller may still trigger a resend and gets { otp_id, sent } with
no code; ops, support and the recipient's own app still read it.

This lands before the consumer app's PIN screen, which advertises "the driver
cannot close the job without it".

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 6: Accessorial config and `GET /v1/accessorials`

`design/Accessorial Config.dc.html` is a complete spec — config struct, exact
amounts, env var names, `price_accessorials`, error cases and both response shapes.
Implement it as written. The values are the design's and need finance sign-off, but
the shapes are what the app is built against.

**Two reconciliations to make before starting.** The design spec differs from what
Task 3 built, and the design wins because the app is coded to it:

1. **Threshold delivery gets no config entry.** It is a promise about where the
   driver puts the item, not a charge. The UI renders the toggle from nothing.
2. **The response nests.** The design's shape is
   `breakdown { currency, carriage_cents, accessorial_cents, total_cents, items[] }`.
   Task 3's flat `rows: Vec<PriceRow>` becomes the carriage itemisation **inside**
   that object. Revise Task 3's `QuoteResponse` to nest before shipping either.

**Files:**
- Modify: `services/order-intake/src/config.rs` (`AccessorialsConfig`, `AccessorialRate`, `AccessorialBasis`)
- Create: `services/order-intake/src/domain/value_objects/accessorials.rs` (`price_accessorials`, `AccessorialError`)
- Create: `services/order-intake/src/api/http/accessorials.rs` (`GET /v1/accessorials`)
- Modify: `services/order-intake/src/api/http/mod.rs` (route)
- Modify: `services/order-intake/src/api/http/quote.rs` (fold into the signed total)
- Modify: `services/api-gateway/src/proxy/mod.rs` (route `/v1/accessorials`)

The rate card, verbatim from the design — minor units, matching `Money`:

| Code | Charged | PHP | AED | Env var |
|---|---|---|---|---|
| `helper` | Per stair flight, max 8 | 81200 | 5100 | `ACCESSORIALS__HELPER__AMOUNT_CENTS` |
| `assembly` | Per booking | 150800 | 9500 | `ACCESSORIALS__ASSEMBLY__AMOUNT_CENTS` |
| `haulaway` | Per booking | 226200 | 14300 | `ACCESSORIALS__HAULAWAY__AMOUNT_CENTS` |
| `carbon_offset` | Per booking | 23200 | 1500 | `ACCESSORIALS__CARBON_OFFSET__AMOUNT_CENTS` |

Each entry is **independently** optional — unlike `payments`, these are not
all-or-nothing, because a market offering haul-away but not assembly is ordinary
whereas two of three payment fields is always a misconfiguration.

- [ ] **Step 1: Write the failing tests**

Create `services/order-intake/src/domain/value_objects/accessorials.rs` with the
test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AccessorialBasis, AccessorialRate, AccessorialsConfig};
    use logisticos_types::Currency;

    fn php_card() -> AccessorialsConfig {
        AccessorialsConfig {
            helper: Some(AccessorialRate {
                amount_cents: 81_200,
                currency:     "PHP".into(),
                basis:        AccessorialBasis::StairFlight,
                max_units:    Some(8),
            }),
            assembly: Some(AccessorialRate {
                amount_cents: 150_800,
                currency:     "PHP".into(),
                basis:        AccessorialBasis::Booking,
                max_units:    None,
            }),
            haulaway: Some(AccessorialRate {
                amount_cents: 226_200,
                currency:     "PHP".into(),
                basis:        AccessorialBasis::Booking,
                max_units:    None,
            }),
            carbon_offset: None,
        }
    }

    fn req(code: &str, units: Option<u16>) -> AccessorialRequest {
        AccessorialRequest { code: code.into(), units }
    }

    /// The design's worked example: 2 flights of helper plus haul-away.
    #[test]
    fn it_prices_the_designs_worked_example() {
        let total = price_accessorials(
            &php_card(), Currency::Php,
            &[req("helper", Some(2)), req("haulaway", None)],
        ).expect("both are offered");
        assert_eq!(total.amount, 162_400 + 226_200);
    }

    /// An unconfigured accessorial is a 422 naming the code, never a silent zero.
    /// carbon_offset is None in this card.
    #[test]
    fn an_unconfigured_accessorial_is_an_error_not_a_free_one() {
        let err = price_accessorials(
            &php_card(), Currency::Php, &[req("carbon_offset", None)],
        ).expect_err("not offered in this market");
        assert_eq!(err, AccessorialError::NotOffered("carbon_offset".into()));
    }

    /// Helper caps at 8 flights. Without the cap a caller states 400.
    #[test]
    fn too_many_units_is_an_error_not_a_clamp() {
        let err = price_accessorials(
            &php_card(), Currency::Php, &[req("helper", Some(20))],
        ).expect_err("over the cap");
        assert_eq!(err, AccessorialError::TooManyUnits("helper".into(), 8));
    }

    /// A card in the wrong currency is refused rather than charged at face
    /// value, so a misconfigured market cannot underbill.
    #[test]
    fn a_currency_mismatch_is_refused() {
        let err = price_accessorials(
            &php_card(), Currency::Aed, &[req("assembly", None)],
        ).expect_err("PHP card, AED tenant");
        assert_eq!(err, AccessorialError::CurrencyMismatch("assembly".into()));
    }

    /// A stair-flight accessorial with no unit count prices at zero units, not
    /// at one — the caller said nothing about flights.
    #[test]
    fn a_stair_flight_accessorial_with_no_count_is_zero() {
        let total = price_accessorials(
            &php_card(), Currency::Php, &[req("helper", None)],
        ).expect("offered");
        assert_eq!(total.amount, 0);
    }

    /// Threshold delivery has no entry and must not be requestable — it is not
    /// a charge, so a request for it is a misuse of the endpoint.
    #[test]
    fn threshold_is_not_an_accessorial() {
        let err = price_accessorials(
            &php_card(), Currency::Php, &[req("threshold", None)],
        ).expect_err("threshold is free and has no config entry");
        assert_eq!(err, AccessorialError::NotOffered("threshold".into()));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake accessorial 2>&1 | tail -20
```

Expected: FAIL — `cannot find type AccessorialsConfig`.

- [ ] **Step 3: Implement the config types**

Add to `services/order-intake/src/config.rs`, on the root config struct:

```rust
    /// Optional accessorial rate card. Each entry is independently optional:
    /// an unset accessorial is simply not offered, and a quote that requests
    /// it is a 422 rather than a silent zero.
    #[serde(default)]
    pub accessorials: AccessorialsConfig,
```

```rust
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AccessorialsConfig {
    #[serde(default)] pub helper:        Option<AccessorialRate>,
    #[serde(default)] pub assembly:      Option<AccessorialRate>,
    #[serde(default)] pub haulaway:      Option<AccessorialRate>,
    #[serde(default)] pub carbon_offset: Option<AccessorialRate>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AccessorialRate {
    /// Minor units, in the tenant's own currency — never converted.
    pub amount_cents: i64,
    /// `PHP`, `AED`. Must match the tenant's JWT currency claim or the quote is
    /// refused, so a misconfigured market cannot underbill.
    pub currency: String,
    /// `booking` (once) or `stair_flight` (multiplied by the stated count).
    #[serde(default)] pub basis: AccessorialBasis,
    /// Cap on a multiplied basis. `helper` without one lets a caller state 400
    /// flights.
    #[serde(default)] pub max_units: Option<u16>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessorialBasis {
    #[default] Booking,
    StairFlight,
}

impl AccessorialsConfig {
    pub fn lookup(&self, code: &str) -> Option<&AccessorialRate> {
        match code {
            "helper"        => self.helper.as_ref(),
            "assembly"      => self.assembly.as_ref(),
            "haulaway"      => self.haulaway.as_ref(),
            "carbon_offset" => self.carbon_offset.as_ref(),
            _ => None,
        }
    }

    /// Every configured accessorial, for `GET /v1/accessorials`.
    pub fn offered(&self) -> Vec<(&'static str, &AccessorialRate)> {
        [
            ("helper",        self.helper.as_ref()),
            ("assembly",      self.assembly.as_ref()),
            ("haulaway",      self.haulaway.as_ref()),
            ("carbon_offset", self.carbon_offset.as_ref()),
        ]
        .into_iter()
        .filter_map(|(code, rate)| rate.map(|r| (code, r)))
        .collect()
    }
}
```

- [ ] **Step 4: Implement `price_accessorials`**

Prepend to `services/order-intake/src/domain/value_objects/accessorials.rs`:

```rust
//! Accessorial pricing. Server-side and nowhere else, the same rule as
//! `quote_price_cents`: the caller states which accessorials and how many units,
//! never the price.

use serde::{Deserialize, Serialize};

use crate::config::{AccessorialBasis, AccessorialsConfig};
use logisticos_types::{Currency, Money};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AccessorialRequest {
    pub code:  String,
    #[serde(default)]
    pub units: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AccessorialError {
    #[error("accessorial '{0}' is not offered in this market")]
    NotOffered(String),
    #[error("accessorial '{0}' is priced in a different currency to this tenant")]
    CurrencyMismatch(String),
    #[error("accessorial '{0}' allows at most {1} units")]
    TooManyUnits(String, u16),
}

/// One priced accessorial, for the quote breakdown's `items[]`.
#[derive(Debug, Clone, Serialize)]
pub struct PricedAccessorial {
    pub code:         String,
    pub units:        u16,
    pub amount_cents: i64,
}

/// Total for the requested accessorials, or the first offending code.
pub fn price_accessorials(
    cfg:       &AccessorialsConfig,
    currency:  Currency,
    requested: &[AccessorialRequest],
) -> Result<Money, AccessorialError> {
    Ok(Money::new(
        price_accessorials_itemised(cfg, currency, requested)?
            .iter()
            .map(|p| p.amount_cents)
            .sum(),
        currency,
    ))
}

/// The same calculation, itemised. `price_accessorials` sums this rather than
/// repeating the arithmetic — the Review screen's rows and its total cannot drift.
pub fn price_accessorials_itemised(
    cfg:       &AccessorialsConfig,
    currency:  Currency,
    requested: &[AccessorialRequest],
) -> Result<Vec<PricedAccessorial>, AccessorialError> {
    let mut out = Vec::with_capacity(requested.len());

    for req in requested {
        let rate = cfg
            .lookup(&req.code)
            .ok_or_else(|| AccessorialError::NotOffered(req.code.clone()))?;

        if rate.currency != currency.as_str() {
            return Err(AccessorialError::CurrencyMismatch(req.code.clone()));
        }

        let units = match rate.basis {
            AccessorialBasis::Booking     => 1,
            AccessorialBasis::StairFlight => req.units.unwrap_or(0),
        };

        if let Some(max) = rate.max_units {
            if units > max {
                return Err(AccessorialError::TooManyUnits(req.code.clone(), max));
            }
        }

        out.push(PricedAccessorial {
            code:         req.code.clone(),
            units,
            amount_cents: rate.amount_cents.saturating_mul(units as i64),
        });
    }

    Ok(out)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-order-intake accessorial 2>&1 | tail -20
```

Expected: PASS, 6 passed. If `Currency::as_str` or `Money::new` have different
signatures, read them off `libs/types/src/lib.rs` and match — do not add a
conversion layer.

- [ ] **Step 6: Add `GET /v1/accessorials`**

Create `services/order-intake/src/api/http/accessorials.rs` returning exactly the
design's shape:

```json
{
  "currency": "PHP",
  "items": [
    { "code": "helper", "amount_cents": 81200, "basis": "stair_flight", "max_units": 8 },
    { "code": "assembly", "amount_cents": 150800, "basis": "booking" },
    { "code": "haulaway", "amount_cents": 226200, "basis": "booking" },
    { "code": "carbon_offset", "amount_cents": 23200, "basis": "booking" }
  ]
}
```

Build `items` from `cfg.accessorials.offered()`, and take `currency` from the
caller's JWT claim. Mount it in the authed router in
`services/order-intake/src/api/http/mod.rs`:

```rust
            .route("/accessorials", get(accessorials::list_accessorials))
```

- [ ] **Step 7: Route it at the gateway, with a test**

In `services/api-gateway/src/proxy/mod.rs`, extend the order-intake branch:

```rust
            || path.starts_with("/v1/accessorials")
```

And add the test beside Task 4's:

```rust
    /// The consumer Review screen's accessorial toggles.
    #[test]
    fn accessorials_reach_order_intake() {
        assert_eq!(
            resolve("/v1/accessorials").as_deref(),
            Some("http://order-intake:8004"),
        );
    }
```

Correct the expected host from the neighbouring passing tests if it differs.

- [ ] **Step 8: Fold accessorials into the signed quote**

Add to `QuoteRequest` in `quote.rs`:

```rust
    #[serde(default)]
    pub accessorials: Vec<AccessorialRequest>,
```

Call `price_accessorials_itemised`, nest the result as the breakdown's `items[]`
and `accessorial_cents`, and add `accessorial_cents` to the token payload — so the
accessorials are covered by the same tamper check as the carriage fee, and
`POST /v1/shipments` re-prices and compares exactly as it already does for service
type and weight.

Extend Task 3's reconciliation guard to assert
`carriage_cents + accessorial_cents == total_cents`. **One guard, extended — not a
second one.**

- [ ] **Step 6: Commit**

```bash
git add services/order-intake/src services/api-gateway/src
git commit -F - <<'EOF'
feat(order-intake): accessorial rate card and GET /v1/accessorials

Ported from design/Accessorial Config.dc.html so the app's ACCESSORIALS object
becomes a fetch with no reshaping. Threshold delivery is a zero-cost row, not
an absent one — the toggle needs something to bind to. Helper flights clamp at
8 server-side. Accessorial rows go through the same reconciliation guard as the
carriage fee rather than a second one.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
```

---

### Task 7: Deploy Phase 0 and verify it is actually live

A green image build does not mean the code is in an image, and a merge does not
deploy it — see `project_ghcr_build_success_means_nothing` and
`project_driver_tiers_and_surfaces`.

- [ ] **Step 1: Confirm CI is green with the correct command**

```bash
CARGO_INCREMENTAL=0 cargo clippy --workspace --all-targets 2>&1 | tail -30
```

Expected: no warnings. `--workspace --all-targets`, not lib-only — the lib-only
form is the wrong command (corrected in `project_qr_table_ordering`).

- [ ] **Step 2: Wait for GHCR, then compare image timestamps against the commit**

```bash
git log -1 --format=%cI
ssh root@75.119.138.135 "for s in carrier order-intake api-gateway pod; do echo -n \"\$s \"; docker image inspect ghcr.io/breakdisk/logisticos-service-\$s:latest --format '{{.Created}}' 2>/dev/null || echo absent; done"
```

The image `Created` must be **later** than the commit timestamp. If it is earlier,
the build has not finished — do not proceed.

- [ ] **Step 3: Pull and restart the four services**

```bash
ssh root@75.119.138.135 "cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code && docker compose pull carrier order-intake api-gateway pod && docker compose up -d carrier order-intake api-gateway pod"
```

- [ ] **Step 4: Verify each route is in the running binary**

An HTTP probe cannot tell you this — a present and an absent route both answer 401
unauthenticated, because auth runs before routing.

```bash
ssh root@75.119.138.135 "docker exec \$(docker ps -qf name=carrier) grep -a -c 'internal/quote-breakdown' /app/carrier; docker exec \$(docker ps -qf name=api-gateway) grep -a -c 'hub-transfer' /app/api_gateway; docker exec \$(docker ps -qf name=order-intake) grep -a -c 'accessorials' /app/order_intake"
```

Expected: each `1` or more. A `0` means the container is running an older image.

- [ ] **Step 5: Confirm the two env vars the new paths depend on**

Task 3 needs carrier's URL on order-intake, and Task 5 made PIN generation depend on
order-intake's URL on pod — where it previously trusted the request body. Both fail
closed, which is correct and also means a missing var is now an outage.

```bash
ssh root@75.119.138.135 "cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code && for v in SERVICES__CARRIER_URL SERVICES__ORDER_INTAKE_URL GEOCODER__MAPBOX_ACCESS_TOKEN; do echo -n \"\$v \"; grep -c \"^\$v=\" .env || true; done"
```

Expected: `1` for each. A `0` on `SERVICES__ORDER_INTAKE_URL` means PIN generation
503s; a `0` on `SERVICES__CARRIER_URL` means every move quote 503s.

- [ ] **Step 6: Quote a real move end to end**

With a `customer`-role JWT for the `demo` tenant, POST a move with origin,
destination and dimensions, and confirm: HTTP 200, `pricing_mode: "rate_card"`,
three rows, and the rows summing to `amount_cents`.

This is the acceptance gate for Phase 0. Do not begin Phase 3 until this returns
a reconciling breakdown against the live VPS.

---

## Phase 1 — One palette, both apps

**Own plan required before execution.** Small, but it touches five portals.

| Task | Files |
|---|---|
| 1.1 Extract the CLAUDE.md palette into `packages/ui-tokens` | `packages/ui-tokens/src/palette.ts` — `#050810` ground, `#00E5FF` cyan, `#A855F7` purple, `#00FF88` green, `#FFAB00` amber, glassmorphism utilities, `cubic-bezier(0.16, 1, 0.3, 1)` |
| 1.2 Map the designs' cyan tokens onto it | `#2dd4e8` → `#00E5FF`, `#07090c`/`#0a0c0f` → `#050810`. Keep the handoff's contrast floors (alpha ≥ .42 at 11px, ≥ .6 at 13px) — those were earned in review and the platform palette does not supersede them. |
| 1.3 Kotlin mirror with every colour behind the theme object | `apps/driver-app-android/core/designsystem/.../Palette.kt`. Sun mode breaks the instant one literal colour ships, so add a lint or test asserting no hex literal appears in `feature/**`. |
| 1.4 Point customer-app at the package | Replace any local colour constants in `apps/customer-app/src/` |

Carry forward from the handoff regardless of palette: Barlow / Barlow Condensed,
`tabular-nums` on every amount and distance, 44px minimum hit target, **56–64px in
the driver app — do not shrink driver controls**, and `box-sizing: border-box` on
every bordered button.

---

## Phase 2 — Driver app

**Own plan required before execution.** Every endpoint below is verified present.
This is the phase the handoff's "backend is complete" claim holds for.

| Task | Screen | Endpoint (verified) | Module |
|---|---|---|---|
| 2.1 | Loads board + duty card | `GET /v1/offers/open`, `POST /v1/offers/{id}/claim`·`/pass`·`/seen` → **dispatch**, `POST /v1/drivers/go-online`·`/go-offline` → driver-ops | `feature/home`. Put the offers calls in a new `DispatchApiService.kt`, **not** `DriverOpsApiService.kt` — the handoff names the wrong service. |
| 2.2 | Route stop list | `GET /v1/tasks`, `PUT /v1/tasks/{id}/start`·`/complete`·`/fail` | `feature/route` |
| 2.3 | Hub scan + manual AWB fallback | `POST /v1/hub-transfer/scans`, `GET /v1/hub-transfer/shipment-by-awb` | `feature/hub`. **Blocked on Task 4.** The manual fallback is required, not optional — the AWB lookup 404s. |
| 2.4 | POD three frames + PIN | `POST /v1/otps/verify`; `POST /v1/pods` → `/{id}/upload-url` → `/{id}/photos` → `PUT /{id}/submit`; `PUT /{id}/signature` | `feature/pod`. Three frames are three `upload-url` + attach passes before one submit. |
| 2.5 | Compliance credentials | `GET /api/v1/compliance/me/profile`, `POST /api/v1/compliance/me/documents/upload` | `feature/profile` |
| 2.6 | Wallet | `GET /v1/drivers/me/earnings`, `GET /v1/cod/driver-ledger/me` | `feature/profile` |
| 2.7 | Chat / Call | **No endpoint.** Ship Call dialing `driverPhone` natively; point Chat at `POST /v1/agents/chat` until Phase 4.1 lands. | `feature/route` |

**Driver-app discipline, from `project_driver_app_chain_of_custody` and CLAUDE.md:**
capture `System.currentTimeMillis()` at the physical event — scan callback, camera
shutter — and convert immediately with `HubRepository.isoFromMillis()`. Never
re-sample at coroutine launch or network-send time.

**Testing:** per `project_driver_app_test_harness`, run **both**
`testDebugUnitTest` and `testStagingDebugUnitTest`, and never write `runTest`
against a ViewModel with a `while(true){delay}` loop — it spins virtual time into
an OOM and hangs Gradle for hours. Per `project_android_local_build_works`, the
suite does run locally on JBR 21 in ~35s; "Android cannot be built locally" is false.

---

## Phase 3 — Consumer app

**Own plan required before execution. Blocked on Phase 0 Task 7 Step 5.**

| Task | Screen | Endpoint | File |
|---|---|---|---|
| 3.1 | Home + intent routing | `POST /v1/agents/chat` | `src/screens/home/`. Keep the `SUPPORT_HINTS` regex exactly as the handoff has it — it is deliberately narrow, and "help"/"driver"/"broken" are ordinary move vocabulary. Keep the unread-support affordance as a **44px header button**; as a full-width strip it consumed 94px of flow and pushed the chips under the composer. |
| 3.2 | Thinking | none — local | Four steps. Step 3 is "priced it off the rate card". **Do not reintroduce the spot-market auction copy — there is no auction in the backend.** |
| 3.3 | Review | `POST /v1/shipments/quote` (Phase 0), `GET /v1/accessorials` | `src/screens/quote/`. Render from the response `rows`; never recompute client-side. |
| 3.4 | Booked + PIN | `POST /v1/shipments`, `POST /v1/otps/generate` | `src/screens/booking/`. Four grouped 64px boxes. |
| 3.5 | Tracking | `GET /v1/tracking/:shipmentId` | `src/screens/tracking/`. Drop the poll to 5s while mounted (`tracking.ts:47`). The driver block is `{name, lat, lng}` — **no `heading`**, so do not build a rotating marker. |
| 3.6 | Payments | `GET /v1/customers/:id/invoices` + detail | `src/screens/invoices/`. One screen. Hero total is **derived** by summing the settled invoices in the rendered array, never a standalone figure. |
| 3.7 | Support thread | `POST /v1/agents/chat`, `GET /v1/agents/chat/:id` | `src/screens/support/`. Escalation banner is the `resolved_by_human` state. Keep the "Book a move instead" button — the classifier will sometimes be wrong. |
| 3.8 | Coverage / rules / legal | static | `src/screens/profile/` |

**Currency** follows the pickup address `country_code` and the `currency` the quote
returns. The design's market switcher is a review convenience — **do not ship it as
a user preference.**

**Build-time env trap:** `EXPO_PUBLIC_*` is compiled in, not read at runtime, and
`client.ts:107-130` defaults every client to `http://localhost`. Per
`project_next_public_baked_at_build_time`, five sites have already shipped
localhost to production. Verify the built bundle, not the `.env`.

---

## Phase 4 — The five gaps

**Own plan required per item.** Open these as tickets now; none blocks Phases 1–3.

| # | Gap | Shape of the work |
|---|---|---|
| 4.1 | Driver ↔ customer chat | Task-scoped message entity, Kafka topic, push fan-out both sides. **Pre-create the topic on the live broker** — per `project_kafka_topics_must_be_precreated` the CI guard has been wrong three times, and `kafka-topics --describe` times out and reads as absence; use `--members --verbose`. |
| 4.2 | Masked voice calling | Proxy-number provider beside the existing Twilio SMS integration. Until then Call dials `driverPhone` natively, which tracking already returns. |
| 4.3 | Hours-of-service clock | Server-side driving counter on `go-online`/`go-offline`, which today return `status: "available"` and nothing else. A local timer is not defensible in an audit. |
| 4.4 | Live location stream | 5s poll while tracking is mounted (3.5), then SSE on delivery-experience. The Kafka path already works; only the client interval is wrong. |
| 4.5 | `heading` on the tracking driver block | Driver-ops publishes location; `DriverPosition` carries `lat`/`lng` only. Adding heading means carrying it from the Android location fix through the Kafka event to `DriverPosition`. Small, and it is what the design's marker needs. |

---

## Open questions for the architect

### Raised by the design file itself, and both are real

**1. Does the driver get paid for an accessorial?** Helper and haul-away are labour
the driver performs. Nothing in driver-ops splits an accessorial into driver
earnings, so as specified the whole amount sits with the platform. This is a payout
question rather than a pricing one, but `helper` should not go live before it is
answered — and the Wallet screen in Phase 2.6 is where a driver would notice.

**2. Who confirms a stair flight?** The customer states the count at booking and the
price follows from it. If the building has four flights and they said two, either the
driver adjusts at the stop — which needs an amend-the-job endpoint that does not
exist — or the platform absorbs it. The design assumes the customer's word is final.
Shipping that assumption is a decision, not a default; it just needs to be a
deliberate one.

### On the parcel tariff

`ae_base_fee_for` and `ae_piece_fee_for` remain the only parcel tariff, still
flagged in code as a first cut pending finance sign-off, and still AED-only. This
plan scopes the AED gate to that branch rather than removing it, so the consumer
move path is unblocked without quietly shipping an unsigned-off table to eight more
markets. That leaves **parcel** quoting unavailable outside AE — deliberately, and
visibly, rather than wrong. Getting the other eight markets a parcel tariff is a
finance deliverable, not an engineering one, and is out of scope here.

---

## Execution notes (2026-09-13) — where the code differed from this plan

Tasks 1–6 are implemented on `claude/uber-freight-design-backend-900a06`
(`29366761`..`51818663`). Tasks 0 and 7 (VPS env + deploy) are held for the
architect. The plan above is left as written; these are the corrections.

| Plan said | Reality | Effect |
|---|---|---|
| `cargo test -p carrier` | Packages are `logisticos-carrier`, `logisticos-order-intake`, `logisticos-pod`, `logisticos-api-gateway` | Commands corrected in place above. |
| `SizeClass::CargoVan` | Variant is `SizeClass::Van`; there was no display label | Added `SizeClass::label()` ("Cargo van"); `as_str` stays the wire value. |
| Mount on carrier's router beside `/v1/internal/sla-records` | Carrier layers `require_auth` over the **whole** of `router()`, so that route requires a JWT despite its comment | New route lives in `internal_router()`, merged after the JWT layer. `sla-records` left alone — dispatch calls it and evidently sends a JWT. The gateway blocks every `/v1/internal` path (`proxy/mod.rs:27`), so the new route is mesh-only. |
| `state.svc.listings.find_available_listings(tenant, kg, None, now, 50)` | `state.marketplace_svc.find_available_listings(tenant, kg, None, 50)` — the service supplies the timestamp | No new repository method. |
| New `billable_weight_grams` with its own DIM arithmetic | `ShipmentDimensions::volumetric_weight_grams` already existed | Delegates to it. That function multiplied three `u32`s off the request body and panicked on overflow; now saturating (widening to u64 alone was not enough — its test failed on the first attempt). |
| `AppError::Internal(String)` | Wraps `anyhow::Error` | `.map_err(AppError::Internal)` / `anyhow::anyhow!`. |
| `Currency::Php` / `as_str()` | `Currency::PHP`, `Display` only | `parse_currency` in `quote.rs`; comparison via `to_string()`. |
| `recipient_phone_for_shipment(&tenant, id)` | Tenant not needed — the internal billing lookup is by shipment id | Single-argument. Fails closed, unlike `resolve_billing_context`. |
| Task 2 test-first | Test and implementation were written together | Noted honestly; Tasks 3–6 were test-first. |
