# OmniDeliv Live Telemetry & Status Map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a customer where their courier is, with milestone text and an honest narrowing ETA, on the OmniDeliv order tracking screen.

**Architecture:** field-ops gains one service-token read keyed on the *assignment* id, so it never learns what an order is. omnideliv gains an outbound port mirroring `CourierDispatch`, a hard status gate that stops a courier's location being readable from a finished order, and a pure ETA module with no database or broker dependency. The app renders a coordinate plot in pure React Native behind a component interface that a tile map can later replace without touching the data path.

**Tech Stack:** Rust (axum, sqlx, reqwest, async-trait), React Native 0.81 + Expo 54 + expo-router, TypeScript, jest-expo.

**Spec:** `docs/superpowers/specs/2026-08-16-omnideliv-telemetry-and-money-surface-design.md`

**Companion plan:** `docs/superpowers/plans/2026-08-16-omnideliv-money-surface.md` — independent, either order.

---

## Orientation for someone new to this codebase

Read these before Task 1. They are short and they explain choices this plan depends on.

- `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs` — how omnideliv calls field-ops. **It mints a 60-second JWT per call** from `AUTH__JWT_SECRET` rather than using a static token, because a static token expires and carries one fixed tenant. You will reuse its private `mint()` by adding a trait impl **in that same file**.
- `services/omnideliv/src/api/http/tracking.rs` — the handler you extend. Note its existing habit: a missing timeline degrades the response, it does not fail it. Keep that.
- `services/field-ops/src/domain/repositories/mod.rs` — the tenancy rule: every repository method takes `tenant_id` first, because there is no row-level security in this schema. The signature *is* the enforcement.

**Two names that already exist and must not be shadowed:**
- `AppState.telemetry` in omnideliv is the **order telemetry log** repository, nothing to do with couriers. The new field is `courier_telemetry`.
- `CourierSupply` and `CourierDispatch` are existing traits on `FieldOpsDispatch`. You are adding a third: `CourierTelemetry`.

**Build commands.** Always set `CARGO_INCREMENTAL=0` — the incremental cache fills the C: drive on this machine and `link.exe` exit code 1318 is a disk-full error, not a code error.

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops -p logisticos-omnideliv
```

If a crate name differs, read the `name` field in that service's `Cargo.toml` and use it.

---

## File structure

| File | Responsibility |
|---|---|
| `services/field-ops/src/domain/entities/location.rs` *(modify)* | Add fix age, staleness, and smoothed speed — pure functions on data field-ops already owns |
| `services/field-ops/src/infrastructure/db/location_repo.rs` *(modify)* | Add `recent()` — the history query that makes smoothing possible |
| `services/field-ops/src/application/services/dispatch_service.rs` *(modify)* | Add `position_for_assignment` — the assignment → courier → fix join |
| `services/field-ops/src/api/http/couriers.rs` *(modify)* | Add the `GET /assignments/:id/position` route |
| `services/omnideliv/src/application/services/telemetry.rs` *(create)* | `CourierTelemetry` port + `CourierFix` DTO + `NoopCourierTelemetry` |
| `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs` *(modify)* | `impl CourierTelemetry for FieldOpsDispatch`, reusing `mint()` |
| `services/omnideliv/src/domain/entities/eta.rs` *(create)* | Pure: `LatLng`, `EtaEstimate`, `estimate_eta`, `haversine_km` |
| `services/omnideliv/src/api/http/tracking.rs` *(modify)* | Status gate, stop coordinates, extended `TrackResponse` |
| `services/omnideliv/src/bootstrap.rs` *(modify)* | Wire `courier_telemetry` into `AppState` |
| `apps/omnideliv-app/src/api/tracking.ts` *(modify)* | New response types |
| `apps/omnideliv-app/src/components/map/types.ts` *(create)* | `MapSurfaceProps` alone — the seam's contract, in a leaf module so nothing imports in a circle |
| `apps/omnideliv-app/src/components/map/MapSurface.tsx` *(create)* | Chooses the implementation; the only name the screens import |
| `apps/omnideliv-app/src/components/map/CanvasPlot.tsx` *(create)* | Pure-RN implementation of that seam |
| `apps/omnideliv-app/src/components/map/project.ts` *(create)* | Pure lat/lng → screen projection, unit-testable without rendering |
| `apps/omnideliv-app/app/track/[id].tsx` *(modify)* | Assemble; wire `pollIntervalMs`; four degrade states |

---

## Task 1: Fix age, staleness, and smoothed speed (field-ops, pure)

Pure functions first, because these are the only parts of the telemetry path provable without a database. The DB-backed tests in these services have never run against a real Postgres, so anything that *can* be a pure test *must* be.

**Files:**
- Modify: `services/field-ops/src/domain/entities/location.rs`

- [ ] **Step 1: Write the failing tests**

Append to the existing `mod tests` block at the bottom of `location.rs`:

```rust
    fn at(secs_ago: i64, speed: Option<f32>) -> CourierLocation {
        let mut l = CourierLocation::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            14.5995,
            120.9842,
            Some(Utc::now() - chrono::Duration::seconds(secs_ago)),
        );
        l.speed_kph = speed;
        l
    }

    #[test]
    fn age_is_measured_from_the_device_clock() {
        let now = Utc::now();
        let l = at(45, None);
        assert!((l.age_seconds(now) - 45).abs() <= 1);
    }

    /// A clock skewed into the future must not read as a negative age, which
    /// would make a stale fix look fresher than a live one.
    #[test]
    fn a_future_fix_reads_as_zero_age_not_negative() {
        let now = Utc::now();
        let l = at(-30, None);
        assert_eq!(l.age_seconds(now), 0);
    }

    #[test]
    fn a_fix_older_than_the_window_is_stale() {
        let now = Utc::now();
        assert!(!at(FIX_STALE_AFTER_SECS - 10, None).is_stale(now));
        assert!(at(FIX_STALE_AFTER_SECS + 10, None).is_stale(now));
    }

    /// Weighted towards the newest reading, so pulling away from a kerb shows
    /// up faster than it would in a flat mean.
    #[test]
    fn smoothing_weights_the_newest_reading_hardest() {
        // newest first, as the repository returns them
        let fixes = vec![at(0, Some(30.0)), at(10, Some(10.0)), at(20, Some(10.0))];
        let s = smoothed_speed_kph(&fixes).expect("some speed");
        let flat_mean = (30.0 + 10.0 + 10.0) / 3.0;
        assert!(s > flat_mean, "EWMA {s} should exceed the flat mean {flat_mean}");
        assert!(s < 30.0, "and still be pulled down by the older readings");
    }

    /// The common case today: the ingest route may carry no speed at all, so
    /// this must answer "unknown" rather than zero. Zero would read as a
    /// stopped courier and push every ETA to its clamp.
    #[test]
    fn no_speed_readings_at_all_is_unknown_not_zero() {
        assert_eq!(smoothed_speed_kph(&[at(0, None), at(10, None)]), None);
        assert_eq!(smoothed_speed_kph(&[]), None);
    }

    #[test]
    fn readings_without_speed_are_skipped_not_counted_as_zero() {
        let fixes = vec![at(0, Some(20.0)), at(10, None), at(20, Some(20.0))];
        assert_eq!(smoothed_speed_kph(&fixes), Some(20.0));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops location
```

Expected: FAIL to compile — `cannot find function 'smoothed_speed_kph'`, `no method named 'age_seconds'`, `cannot find value 'FIX_STALE_AFTER_SECS'`.

- [ ] **Step 3: Write the implementation**

Add to `location.rs`, above the `#[cfg(test)]` block:

```rust
/// How old a fix may be before it stops being "live".
///
/// Two minutes is chosen against the app's own poll cadence: the tracking
/// screen refreshes every 5s while delivering, so a fix this old means many
/// consecutive missed reports, not one dropped packet.
pub const FIX_STALE_AFTER_SECS: i64 = 120;

/// Weight given to the newest reading in the speed EWMA.
const SPEED_SMOOTHING_ALPHA: f64 = 0.5;

impl CourierLocation {
    /// Seconds since this fix was taken, floored at zero.
    ///
    /// Floored deliberately: a device clock running fast would otherwise
    /// produce a negative age, and a negative age compares as fresher than a
    /// live fix in every staleness check.
    pub fn age_seconds(&self, now: DateTime<Utc>) -> i64 {
        (now - self.sla_timestamp()).num_seconds().max(0)
    }

    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        self.age_seconds(now) > FIX_STALE_AFTER_SECS
    }
}

/// Exponentially weighted mean speed over recent fixes, newest first.
///
/// Returns `None` when no reading carries a speed — which is the common case
/// while the ingest path does not populate it. `None` means "unknown" and the
/// caller falls back to a default; returning `0.0` would instead assert the
/// courier is stopped and drive every ETA to its slowest clamp.
pub fn smoothed_speed_kph(fixes: &[CourierLocation]) -> Option<f64> {
    let mut acc: Option<f64> = None;
    // Oldest first, so each newer reading gets the heavier weight.
    for f in fixes.iter().rev() {
        let Some(s) = f.speed_kph else { continue };
        let s = s as f64;
        acc = Some(match acc {
            None => s,
            Some(prev) => SPEED_SMOOTHING_ALPHA * s + (1.0 - SPEED_SMOOTHING_ALPHA) * prev,
        });
    }
    acc
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops location
```

Expected: PASS, 8 tests in this module (2 pre-existing + 6 new).

- [ ] **Step 5: Mutation-check the staleness boundary**

A guard that has not been seen to fail has not been shown to work. Temporarily change `.max(0)` to nothing (`(now - self.sla_timestamp()).num_seconds()`) and re-run.

Expected: `a_future_fix_reads_as_zero_age_not_negative` FAILS. Revert the change and confirm the suite is green again.

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/src/domain/entities/location.rs
git commit -m "feat(field-ops): fix age, staleness, and smoothed speed on courier locations"
```

---

## Task 2: The location history query

**Files:**
- Modify: `services/field-ops/src/infrastructure/db/location_repo.rs`

- [ ] **Step 1: Add the trait method**

Extend the existing `LocationRepository` trait (currently `record` and `latest`):

```rust
#[async_trait]
pub trait LocationRepository: Send + Sync {
    async fn record(&self, l: &CourierLocation) -> anyhow::Result<()>;
    async fn latest(&self, tenant_id: Uuid, courier_id: Uuid) -> anyhow::Result<Option<CourierLocation>>;

    /// The most recent `limit` fixes, newest first.
    ///
    /// Smoothing a speed needs a series and `latest` is one point. The history
    /// is already in this table — `record` appends and never updates — so this
    /// is a read of data we hold, not new capture.
    async fn recent(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<CourierLocation>>;
}
```

- [ ] **Step 2: Run the build to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops
```

Expected: FAIL — `not all trait items implemented, missing 'recent'` on `PgLocationRepository`.

- [ ] **Step 3: Implement it**

Open `location_repo.rs` and read the existing `latest` implementation first — copy its column list and its use of the module-level `map_row` helper exactly, so the two queries cannot drift. Add to `impl LocationRepository for PgLocationRepository`:

```rust
    async fn recent(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        limit: i64,
    ) -> anyhow::Result<Vec<CourierLocation>> {
        // Ordered by the same column `latest` orders by, so the newest row here
        // is the same row `latest` returns. Two orderings would let the map show
        // one point and the ETA smooth around a different one.
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, courier_id, lat, lng, accuracy_m, speed_kph,
                   heading_deg, device_timestamp, recorded_at
            FROM   field_ops.courier_locations
            WHERE  tenant_id = $1 AND courier_id = $2
            ORDER  BY recorded_at DESC
            LIMIT  $3
            "#,
        )
        .bind(tenant_id)
        .bind(courier_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(map_row).collect())
    }
```

**Before running:** confirm the table name, the column list and the `ORDER BY` column against the existing `latest` query in the same file. If `latest` orders by something other than `recorded_at`, match it and update the comment. A mapper reading a column no `SELECT` names is a known failure mode in this repo.

- [ ] **Step 4: Run the build to verify it passes**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add services/field-ops/src/infrastructure/db/location_repo.rs
git commit -m "feat(field-ops): read recent courier fixes for speed smoothing"
```

---

## Task 3: `position_for_assignment` on DispatchService

`DispatchService` already holds both `assignments` and `locations`, so the join belongs there and needs no new dependency.

**Files:**
- Modify: `services/field-ops/src/application/services/dispatch_service.rs`

- [ ] **Step 1: Add the method**

Add inside `impl DispatchService`:

```rust
    /// Where the courier holding this assignment is, with enough recent history
    /// to smooth a speed.
    ///
    /// Keyed on the assignment rather than the courier so a caller never needs
    /// to hold a courier id. field-ops therefore stays product-agnostic — it is
    /// answering "where is the courier on this job", not "where is this person".
    ///
    /// `None` for an unknown assignment and `None` for a courier with no fix on
    /// record. Both are a 404 to the caller: distinguishing them would confirm
    /// that an assignment id is real to someone who guessed it.
    pub async fn position_for_assignment(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<(Uuid, CourierLocation, Option<f64>)>> {
        const SMOOTHING_WINDOW: i64 = 5;

        let Some(a) = self.assignments.find_by_id(tenant_id, assignment_id).await? else {
            return Ok(None);
        };

        let recent = self
            .locations
            .recent(tenant_id, a.courier_id, SMOOTHING_WINDOW)
            .await?;

        let Some(latest) = recent.first().cloned() else {
            return Ok(None);
        };

        let smoothed = crate::domain::entities::smoothed_speed_kph(&recent);
        Ok(Some((a.courier_id, latest, smoothed)))
    }
```

Add `CourierLocation` to the file's existing `use crate::domain::entities::{...}` import if it is not already there.

- [ ] **Step 2: Verify `smoothed_speed_kph` is exported**

```bash
grep -n "smoothed_speed_kph\|pub use" services/field-ops/src/domain/entities/mod.rs
```

If `mod.rs` re-exports items from `location` with an explicit list rather than `pub use location::*;`, add `smoothed_speed_kph` and `FIX_STALE_AFTER_SECS` to that list.

- [ ] **Step 3: Run the build**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add services/field-ops/src/application/services/dispatch_service.rs services/field-ops/src/domain/entities/mod.rs
git commit -m "feat(field-ops): resolve an assignment to its courier's latest fix"
```

---

## Task 4: The `GET /assignments/:id/position` route

**Files:**
- Modify: `services/field-ops/src/api/http/couriers.rs`

- [ ] **Step 1: Add the route and handler**

Add to the `routes()` function, alongside the existing assignment routes:

```rust
        .route("/v1/field-ops/assignments/:id/position", get(assignment_position))
```

Add the response type and handler. Match the file's existing handler style — read `my_offers` in the same file first and mirror its error handling.

```rust
/// One courier's live position on a job.
///
/// `age_seconds` travels with the fix so the caller can decide what counts as
/// stale without needing the server's clock. Sending the freshness alongside
/// the point is what lets a consumer refuse to draw a dot rather than drawing
/// an old one and calling it live.
#[derive(Debug, serde::Serialize)]
pub struct PositionResponse {
    pub courier_id:         uuid::Uuid,
    pub lat:                f64,
    pub lng:                f64,
    /// Instantaneous reading from the most recent fix, when the device sent one.
    pub speed_kph:          Option<f32>,
    /// EWMA across recent fixes. `None` when no fix carried a speed.
    pub smoothed_speed_kph: Option<f64>,
    pub heading_deg:        Option<f32>,
    pub device_timestamp:   Option<chrono::DateTime<chrono::Utc>>,
    pub recorded_at:        chrono::DateTime<chrono::Utc>,
    pub age_seconds:        i64,
}

/// Service-to-service only. Mounted inside the auth layer with the other
/// operational routes, so an unauthenticated caller never reaches it.
async fn assignment_position(
    State(st): State<Arc<AppState>>,
    claims: AuthClaims,
    Path(assignment_id): Path<uuid::Uuid>,
) -> Result<Json<PositionResponse>, StatusCode> {
    let (courier_id, fix, smoothed) = st
        .dispatch
        .position_for_assignment(claims.tenant_id, assignment_id)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, %assignment_id, "position lookup failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(PositionResponse {
        courier_id,
        lat:                fix.lat,
        lng:                fix.lng,
        speed_kph:          fix.speed_kph,
        smoothed_speed_kph: smoothed,
        heading_deg:        fix.heading_deg,
        device_timestamp:   fix.device_timestamp,
        recorded_at:        fix.recorded_at,
        age_seconds:        fix.age_seconds(chrono::Utc::now()),
    }))
}
```

- [ ] **Step 2: Confirm the route is inside the auth layer**

```bash
grep -n "couriers::routes\|auth_layer\|route_layer\|merge" services/field-ops/src/api/http/mod.rs
```

Expected: `couriers::routes()` is merged **inside** the `auth_layer`, not alongside `health::routes()`. If it is outside, stop and fix that — an unauthenticated courier-position endpoint is a live location leak. Health being outside the layer is deliberate and correct; this must not be.

- [ ] **Step 3: Check for a duplicate path**

```bash
grep -n "assignments/:id" services/field-ops/src/api/http/couriers.rs
```

Expected: `claim`, `collected`, `delivered`, and the new `position` — each a distinct path. Two `.route()` calls on the *same* path panic at startup in axum; different suffixes are fine.

- [ ] **Step 4: Build**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops
```

Expected: clean. Add `get` to the file's `axum::routing` import if the compiler asks.

- [ ] **Step 5: Commit**

```bash
git add services/field-ops/src/api/http/couriers.rs
git commit -m "feat(field-ops): expose a courier's position by assignment id"
```

---

## Task 5: The omnideliv telemetry port

**Files:**
- Create: `services/omnideliv/src/application/services/telemetry.rs`
- Modify: `services/omnideliv/src/application/services/mod.rs`
- Modify: `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs`

- [ ] **Step 1: Create the port**

`services/omnideliv/src/application/services/telemetry.rs`:

```rust
//! Where the courier is, as omnideliv needs it.
//!
//! The trait belongs to omnideliv and the implementation calls field-ops, so
//! the dependency points inward exactly as `CourierDispatch` does. field-ops
//! knows nothing about this caller.

use async_trait::async_trait;
use uuid::Uuid;

/// A courier position, already judged for freshness.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CourierFix {
    pub lat:                f64,
    pub lng:                f64,
    pub heading_deg:        Option<f32>,
    pub smoothed_speed_kph: Option<f64>,
    pub age_seconds:        i64,
}

#[async_trait]
pub trait CourierTelemetry: Send + Sync {
    /// `None` when there is no assignment, no fix, or field-ops cannot answer.
    async fn position(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<CourierFix>>;
}

/// Used when field-ops is unreachable at startup, mirroring `NoopOrderEvents`.
///
/// A tracking screen without a dot is a worse screen. A tracking screen that
/// 500s is no screen at all, and the order state, the timeline and the amount
/// owed are all still worth serving.
pub struct NoopCourierTelemetry;

#[async_trait]
impl CourierTelemetry for NoopCourierTelemetry {
    async fn position(&self, _: Uuid, _: Uuid) -> anyhow::Result<Option<CourierFix>> {
        Ok(None)
    }
}
```

- [ ] **Step 2: Export it**

In `services/omnideliv/src/application/services/mod.rs`, follow whatever pattern the neighbouring modules use. If they are declared and re-exported:

```rust
pub mod telemetry;
pub use telemetry::{CourierFix, CourierTelemetry, NoopCourierTelemetry};
```

- [ ] **Step 3: Implement it on FieldOpsDispatch**

In `services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs`, add alongside the existing `CourierSupply` and `CourierDispatch` impls. Putting it here is what lets it reuse the private `mint()`.

```rust
#[derive(Debug, Deserialize)]
struct PositionResponse {
    lat:                f64,
    lng:                f64,
    heading_deg:        Option<f32>,
    smoothed_speed_kph: Option<f64>,
    age_seconds:        i64,
}

#[async_trait]
impl crate::application::services::CourierTelemetry for FieldOpsDispatch {
    async fn position(
        &self,
        tenant_id: Uuid,
        assignment_id: Uuid,
    ) -> anyhow::Result<Option<crate::application::services::CourierFix>> {
        let token = self.mint(tenant_id)?;

        let res = self
            .http
            .get(format!(
                "{}/v1/field-ops/assignments/{assignment_id}/position",
                self.base_url
            ))
            .bearer_auth(token)
            .send()
            .await?;

        // 404 is the ordinary answer for a courier who has not reported yet.
        // It is not an error and must not be logged as one, or a normal early
        // minute of every order fills the log with noise that hides real faults.
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("field-ops position lookup failed: {status} {body}");
        }

        let p = res.json::<PositionResponse>().await?;
        Ok(Some(crate::application::services::CourierFix {
            lat:                p.lat,
            lng:                p.lng,
            heading_deg:        p.heading_deg,
            smoothed_speed_kph: p.smoothed_speed_kph,
            age_seconds:        p.age_seconds,
        }))
    }
}
```

- [ ] **Step 4: Build**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add services/omnideliv/src/application/services/telemetry.rs services/omnideliv/src/application/services/mod.rs services/omnideliv/src/infrastructure/external/field_ops_dispatch.rs
git commit -m "feat(omnideliv): a courier telemetry port over field-ops"
```

---

## Task 6: The ETA module (pure)

This is the task with the most tests, because it is the only part of the feature whose correctness is fully checkable without infrastructure.

**Files:**
- Create: `services/omnideliv/src/domain/entities/eta.rs`
- Modify: `services/omnideliv/src/domain/entities/mod.rs`

- [ ] **Step 1: Write the failing tests**

Create `eta.rs` containing **only** the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::services::CourierFix;

    const MANILA: LatLng = LatLng { lat: 14.5995, lng: 120.9842 };

    fn fix(lat: f64, lng: f64, speed: Option<f64>, age: i64) -> CourierFix {
        CourierFix { lat, lng, heading_deg: None, smoothed_speed_kph: speed, age_seconds: age }
    }

    #[test]
    fn haversine_matches_a_known_distance() {
        // Manila to Quezon City city hall, ~12 km apart.
        let d = haversine_km(MANILA, LatLng { lat: 14.6760, lng: 121.0437 });
        assert!((d - 11.5).abs() < 1.5, "expected ~11.5 km, got {d}");
    }

    #[test]
    fn a_stale_fix_yields_no_estimate() {
        let f = fix(14.60, 120.99, Some(20.0), FIX_STALE_AFTER_SECS + 1);
        assert!(estimate_eta(&f, &[], MANILA).is_none());
    }

    #[test]
    fn a_fresh_fix_yields_an_estimate() {
        let f = fix(14.60, 120.99, Some(20.0), 10);
        assert!(estimate_eta(&f, &[], MANILA).is_some());
    }

    /// A stopped courier must not produce an unbounded ETA. The clamp is what
    /// turns a red light into a slightly longer wait rather than "4 hours".
    #[test]
    fn a_stopped_courier_clamps_rather_than_diverging() {
        let f = fix(14.6760, 121.0437, Some(0.0), 5);
        let e = estimate_eta(&f, &[], MANILA).expect("an estimate");
        assert!(e.high_minutes < 24 * 60, "clamped, not divergent");
        // 11.5 km x 1.3 at the 8 km/h floor is under two hours.
        assert!(e.high_minutes <= 120, "got {} minutes", e.high_minutes);
    }

    /// The common case while the ingest path carries no speed.
    #[test]
    fn an_unknown_speed_still_estimates() {
        let f = fix(14.60, 120.99, None, 5);
        assert!(estimate_eta(&f, &[], MANILA).is_some());
    }

    /// A courier flagged at an implausible speed must not produce a
    /// one-minute ETA from across the city.
    #[test]
    fn an_implausible_speed_is_clamped_down() {
        let far = fix(14.6760, 121.0437, Some(900.0), 5);
        let e = estimate_eta(&far, &[], MANILA).expect("an estimate");
        assert!(e.low_minutes >= 15, "11.5 km cannot take {} min", e.low_minutes);
    }

    #[test]
    fn the_range_narrows_as_the_courier_closes() {
        let far  = estimate_eta(&fix(14.6760, 121.0437, Some(20.0), 5), &[], MANILA).unwrap();
        let near = estimate_eta(&fix(14.6050, 120.9890, Some(20.0), 5), &[], MANILA).unwrap();

        let far_width  = far.high_minutes - far.low_minutes;
        let near_width = near.high_minutes - near.low_minutes;
        assert!(near_width < far_width, "near {near_width} should be tighter than far {far_width}");
        assert!(near.high_minutes < far.high_minutes);
    }

    /// Uncollected stops are time, not just distance — a shop pickup is not
    /// instantaneous, and an ETA that ignores it is always early.
    #[test]
    fn each_remaining_stop_adds_dwell_time() {
        let f = fix(14.60, 120.99, Some(20.0), 5);
        let none = estimate_eta(&f, &[], MANILA).unwrap();
        let two  = estimate_eta(&f, &[LatLng { lat: 14.601, lng: 120.991 },
                                      LatLng { lat: 14.602, lng: 120.992 }], MANILA).unwrap();
        assert!(two.low_minutes >= none.low_minutes + (2.0 * DWELL_PER_STOP_MINS) as i64);
    }

    /// Distance is measured along the route through the stops, not as one hop
    /// to the door — otherwise a detour to a shop is free.
    #[test]
    fn distance_follows_the_stop_sequence() {
        let f = fix(14.60, 120.99, Some(20.0), 5);
        let direct  = estimate_eta(&f, &[], MANILA).unwrap();
        let detour  = estimate_eta(&f, &[LatLng { lat: 14.70, lng: 121.10 }], MANILA).unwrap();
        assert!(detour.low_minutes > direct.low_minutes + (DWELL_PER_STOP_MINS) as i64,
                "a 20 km detour must cost more than its dwell alone");
    }

    #[test]
    fn an_estimate_is_never_negative_or_zero_width() {
        let f = fix(14.5995, 120.9842, Some(20.0), 0); // already at the door
        let e = estimate_eta(&f, &[], MANILA).unwrap();
        assert!(e.low_minutes >= 1);
        assert!(e.high_minutes >= e.low_minutes);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv eta
```

Expected: FAIL to compile — `LatLng`, `haversine_km`, `estimate_eta`, `FIX_STALE_AFTER_SECS`, `DWELL_PER_STOP_MINS` all undefined. (You must also add `pub mod eta;` to `services/omnideliv/src/domain/entities/mod.rs` for the module to compile at all — do that now.)

- [ ] **Step 3: Write the implementation**

Prepend to `eta.rs`, above the test module:

```rust
//! Turning a courier position into what a waiting customer is told.
//!
//! Pure by design: no database, no broker, no clock beyond what is passed in.
//! The database-backed tests in this service have never executed against a real
//! Postgres, so the arithmetic a customer reads must be provable without one.
//!
//! Every constant here is a reasoned starting value, not a measured one. They
//! live together in this module so calibrating against real deliveries is a
//! one-file change.

use serde::{Deserialize, Serialize};

use crate::application::services::CourierFix;

/// Straight-line distance flatters a road network. 1.3 is the common urban
/// detour ratio and errs towards over-estimating the wait, which is the right
/// direction to be wrong in.
pub const ROAD_FACTOR: f64 = 1.3;

/// A courier is never usefully slower than this in traffic and never legally
/// faster than this on a city scooter. The floor is what stops a red light
/// reading as an infinite wait; the ceiling stops a bad GPS reading promising
/// a delivery that cannot happen.
pub const MIN_SPEED_KPH: f64 = 8.0;
pub const MAX_SPEED_KPH: f64 = 40.0;

/// Used when no fix carries a speed — the common case today.
pub const DEFAULT_SPEED_KPH: f64 = 18.0;

/// Parking, queueing, and waiting for a bag at each shop still to visit.
pub const DWELL_PER_STOP_MINS: f64 = 4.0;

/// Must match `field_ops::domain::entities::FIX_STALE_AFTER_SECS`. Duplicated
/// rather than shared because the two services do not depend on each other, and
/// a shared crate for one integer would couple them for no gain.
pub const FIX_STALE_AFTER_SECS: i64 = 120;

/// Half-width of the estimate as a fraction of the midpoint, so the range
/// narrows on its own as the distance falls out of the arithmetic.
const SPREAD: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct EtaEstimate {
    pub low_minutes:  i64,
    pub high_minutes: i64,
}

/// Great-circle distance in kilometres.
pub fn haversine_km(a: LatLng, b: LatLng) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0088;

    let (lat1, lat2) = (a.lat.to_radians(), b.lat.to_radians());
    let dlat = (b.lat - a.lat).to_radians();
    let dlng = (b.lng - a.lng).to_radians();

    let h = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlng / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * h.sqrt().asin()
}

/// How long until this order reaches the door.
///
/// `remaining_stops` are the pickups not yet collected, in visit order.
/// Returns `None` when the fix is too old to reason from — the caller shows no
/// number at all rather than a stale one, which is the same rule that makes
/// `courier_supply` return null instead of a fabricated count.
pub fn estimate_eta(
    fix: &CourierFix,
    remaining_stops: &[LatLng],
    destination: LatLng,
) -> Option<EtaEstimate> {
    if fix.age_seconds > FIX_STALE_AFTER_SECS {
        return None;
    }

    // Along the route, not straight to the door: a detour to a shop is time.
    let mut km = 0.0;
    let mut from = LatLng { lat: fix.lat, lng: fix.lng };
    for stop in remaining_stops {
        km += haversine_km(from, *stop);
        from = *stop;
    }
    km += haversine_km(from, destination);
    km *= ROAD_FACTOR;

    let speed = fix
        .smoothed_speed_kph
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(DEFAULT_SPEED_KPH)
        .clamp(MIN_SPEED_KPH, MAX_SPEED_KPH);

    let travel_mins = (km / speed) * 60.0;
    let dwell_mins  = DWELL_PER_STOP_MINS * remaining_stops.len() as f64;
    let mid         = travel_mins + dwell_mins;

    // The spread is a fraction of the travel component only. Dwell is the part
    // we are most confident about, so padding it would widen the range exactly
    // as the courier arrives — the opposite of narrowing.
    let half = travel_mins * SPREAD;

    let low  = ((mid - half).round() as i64).max(1);
    let high = ((mid + half).round() as i64).max(low);

    Some(EtaEstimate { low_minutes: low, high_minutes: high })
}
```

- [ ] **Step 4: Run the tests**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv eta
```

Expected: PASS, 10 tests.

- [ ] **Step 5: Mutation-check the staleness gate**

Comment out the `if fix.age_seconds > FIX_STALE_AFTER_SECS { return None; }` block and re-run.

Expected: `a_stale_fix_yields_no_estimate` FAILS. Restore it and confirm green.

- [ ] **Step 6: Mutation-check the clamp**

Change `.clamp(MIN_SPEED_KPH, MAX_SPEED_KPH)` to `.max(MIN_SPEED_KPH)` and re-run.

Expected: `an_implausible_speed_is_clamped_down` FAILS. Restore it and confirm green.

- [ ] **Step 7: Commit**

```bash
git add services/omnideliv/src/domain/entities/eta.rs services/omnideliv/src/domain/entities/mod.rs
git commit -m "feat(omnideliv): a pure, clamped, narrowing ETA estimate"
```

---

## Task 7: Extend the track handler

**Files:**
- Modify: `services/omnideliv/src/api/http/tracking.rs`
- Modify: `services/omnideliv/src/api/http/mod.rs` (AppState field)
- Modify: `services/omnideliv/src/bootstrap.rs` (wiring)

- [ ] **Step 1: Add the AppState field**

In `services/omnideliv/src/api/http/mod.rs`, add to `struct AppState`. **The name is `courier_telemetry`** — `telemetry` is already taken by the order telemetry log repository and shadowing it would silently point the map at the wrong thing.

```rust
    /// Where the courier is. Distinct from `telemetry` above, which is the
    /// order event log.
    pub courier_telemetry: Arc<dyn crate::application::services::CourierTelemetry>,
```

- [ ] **Step 2: Wire it in bootstrap**

In `services/omnideliv/src/bootstrap.rs`, the `field_ops` Arc already exists (`let field_ops = Arc::new(FieldOpsDispatch::new(...))`). Add to the `AppState { ... }` construction:

```rust
        courier_telemetry: field_ops.clone(),
```

- [ ] **Step 3: Add the response types**

In `tracking.rs`, add above `TrackResponse`:

```rust
/// One pickup on the way, for the map.
#[derive(Debug, Serialize)]
pub struct StopView {
    pub vendor_name: String,
    pub lat:         f64,
    pub lng:         f64,
    pub picked_up:   bool,
}
```

Extend `TrackResponse` with:

```rust
    /// `None` unless the order is in motion and the fix is fresh. Never a
    /// last-known point presented as live — a frozen dot a customer believes is
    /// moving is worse than an honest gap.
    pub courier:     Option<crate::application::services::CourierFix>,
    pub eta:         Option<crate::domain::entities::eta::EtaEstimate>,
    /// `None` for orders placed before migration 0013, which carry no
    /// destination. Those degrade to a timeline rather than a guessed point.
    pub destination: Option<crate::domain::entities::eta::LatLng>,
    pub stops:       Vec<StopView>,
```

- [ ] **Step 4: Add the status gate and the lookup**

In the `track` handler, after `stops_collected` is computed and before the `Ok(Json(...))`:

```rust
    // Live position is readable only while the order is actually in motion.
    //
    // Not `placed` or `awaiting_courier` (nobody is carrying it yet), and
    // deliberately not after `delivered` or `cancelled` — a courier's live
    // location must not stay readable from a finished order for the rest of
    // their shift. Checked before the outbound call, so it is one gate rather
    // than a rule spread across two services, and it saves the call.
    let in_motion = matches!(
        order.status,
        crate::domain::entities::OrderStatus::Collecting
            | crate::domain::entities::OrderStatus::Delivering
    );

    let courier = match (in_motion, order.courier_task_id) {
        (true, Some(assignment_id)) => st
            .courier_telemetry
            .position(claims.tenant_id, assignment_id)
            .await
            // A telemetry failure degrades the map and never fails the screen,
            // exactly as a missing timeline degrades rather than 404s above.
            .unwrap_or_else(|e| {
                tracing::error!(err = %e, %order_id, "courier position lookup failed");
                None
            }),
        _ => None,
    };

    // Stop coordinates need the vendors, which the legs reference by id only.
    // One query for all of them — a lookup per leg is the N+1 that
    // `CatalogRepository::find_items` was introduced to avoid on the basket.
    let vendor_ids: Vec<uuid::Uuid> = order.legs.iter().map(|l| l.vendor_id).collect();
    let vendors = st
        .catalog
        .find_vendors_by_ids(claims.tenant_id, &vendor_ids)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(err = %e, %order_id, "vendor lookup failed; map will omit stops");
            Vec::new()
        });

    let stops: Vec<StopView> = order
        .legs
        .iter()
        .filter_map(|l| {
            let v = vendors.iter().find(|v| v.id == l.vendor_id)?;
            Some(StopView {
                vendor_name: v.name.clone(),
                lat:         v.lat,
                lng:         v.lng,
                picked_up:   l.status == crate::domain::entities::LegStatus::PickedUp,
            })
        })
        .collect();

    let destination = match (order.delivery_lat, order.delivery_lng) {
        (Some(lat), Some(lng)) => Some(crate::domain::entities::eta::LatLng { lat, lng }),
        _ => None,
    };

    // Only stops still to collect count towards the remaining journey.
    let remaining: Vec<crate::domain::entities::eta::LatLng> = stops
        .iter()
        .filter(|s| !s.picked_up)
        .map(|s| crate::domain::entities::eta::LatLng { lat: s.lat, lng: s.lng })
        .collect();

    let eta = match (&courier, destination) {
        (Some(fix), Some(dest)) => {
            crate::domain::entities::eta::estimate_eta(fix, &remaining, dest)
        }
        _ => None,
    };
```

Then add `courier`, `eta`, `destination`, `stops` to the `TrackResponse { ... }` construction.

- [ ] **Step 5: Add the batch vendor lookup**

First check whether one already exists:

```bash
grep -rn "find_vendors_by_ids\|fn find_vendors\|fn find_by_ids" services/omnideliv/src --include=*.rs
```

If a batch lookup by id already exists, use its real name and update Step 4 to match. If not, add it to the `VendorRepository` trait:

```rust
    /// Several vendors in one round trip.
    ///
    /// One query, not a lookup per leg. An order with four stops would
    /// otherwise issue four queries to draw four dots — the same N+1 that
    /// `CatalogRepository::find_items` was introduced to avoid on the basket.
    async fn find_by_ids(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<Vendor>>;
```

And implement it on the Postgres repository. Read the existing single-vendor `find_by_id` in the same file first and copy its column list and row mapper exactly, so the two cannot drift:

```rust
    async fn find_by_ids(&self, tenant_id: Uuid, ids: &[Uuid]) -> anyhow::Result<Vec<Vendor>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // `= ANY($2)` rather than a built IN-list: one prepared statement
        // regardless of how many stops an order has, and no string building
        // anywhere near a query.
        let rows = sqlx::query(
            r#"
            SELECT id, tenant_id, name, lat, lng
            FROM   omnideliv.vendors
            WHERE  tenant_id = $1 AND id = ANY($2)
            "#,
        )
        .bind(tenant_id)
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(map_vendor_row).collect())
    }
```

**Adjust the `SELECT` list to whatever the existing `find_by_id` selects** — `Vendor` carries more than these five columns and a mapper reading a column no `SELECT` names is a known failure mode in this repo. If the file has no shared row mapper, inline the same field reads `find_by_id` uses.

Then expose it on `CatalogService` as `find_vendors_by_ids`, following how that service wraps its other repository calls. Do not add a per-leg lookup.

- [ ] **Step 6: Build**

```bash
CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv
```

Expected: clean. Fix enum variant paths if `OrderStatus` or `LegStatus` live at different paths — check with `grep -n "enum OrderStatus" -A 10 services/omnideliv/src/domain/entities/order.rs`.

- [ ] **Step 7: Write the gate test**

Add to `tracking.rs` a test module asserting the gate is a pure predicate. Extract the `matches!` into a named function first so it is testable:

```rust
/// Whether a live courier position may be disclosed for an order in this state.
fn discloses_position(status: crate::domain::entities::OrderStatus) -> bool {
    use crate::domain::entities::OrderStatus::*;
    matches!(status, Collecting | Delivering)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::OrderStatus::*;

    #[test]
    fn position_is_disclosed_only_while_the_order_is_in_motion() {
        assert!(discloses_position(Collecting));
        assert!(discloses_position(Delivering));
    }

    /// The one that matters: a courier's live location must not remain
    /// readable from an order that is already finished.
    #[test]
    fn a_finished_order_never_discloses_a_position() {
        assert!(!discloses_position(Delivered));
        assert!(!discloses_position(Cancelled));
    }

    #[test]
    fn an_order_with_no_courier_yet_discloses_nothing() {
        assert!(!discloses_position(Placed));
        assert!(!discloses_position(AwaitingCourier));
    }
}
```

Replace the inline `matches!` in Step 4 with `discloses_position(order.status)`. Confirm the `OrderStatus` variant names against the enum before writing the test — use the real ones.

- [ ] **Step 8: Run and mutation-check**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv tracking
```

Expected: PASS. Then add `| Delivered` to `discloses_position` and re-run.

Expected: `a_finished_order_never_discloses_a_position` FAILS. Revert and confirm green.

- [ ] **Step 9: Commit**

```bash
git add services/omnideliv/src/api/http/tracking.rs services/omnideliv/src/api/http/mod.rs services/omnideliv/src/bootstrap.rs
git commit -m "feat(omnideliv): serve courier position, stops and ETA on the tracking read"
```

---

## Task 8: App types and the poll schedule

**Files:**
- Modify: `apps/omnideliv-app/src/api/tracking.ts`
- Modify: `apps/omnideliv-app/app/track/[id].tsx`
- Create: `apps/omnideliv-app/src/api/__tests__/tracking.test.ts`

- [ ] **Step 1: Write the failing test**

`apps/omnideliv-app/src/api/__tests__/tracking.test.ts`:

```ts
import { pollIntervalMs } from "../tracking";

describe("pollIntervalMs", () => {
  it("stops polling once nothing more can change", () => {
    expect(pollIntervalMs("delivered")).toBeNull();
    expect(pollIntervalMs("cancelled")).toBeNull();
  });

  it("polls fastest while the courier is moving", () => {
    const moving = pollIntervalMs("delivering")!;
    const waiting = pollIntervalMs("awaiting_courier")!;
    expect(moving).toBeLessThan(waiting);
  });

  it("never returns a zero or negative interval", () => {
    for (const s of ["placed", "awaiting_courier", "collecting", "delivering"] as const) {
      expect(pollIntervalMs(s)!).toBeGreaterThan(0);
    }
  });
});
```

- [ ] **Step 2: Run it**

```bash
cd apps/omnideliv-app && npx jest src/api/__tests__/tracking.test.ts
```

Expected: PASS immediately — `pollIntervalMs` already exists and is correct. **This is the point:** the function was written, is right, and has never been called. The test pins it before Step 3 gives it a caller.

- [ ] **Step 3: Add the new response types**

In `apps/omnideliv-app/src/api/tracking.ts`, add and extend:

```ts
export interface LatLng {
  lat: number;
  lng: number;
}

export interface CourierFix extends LatLng {
  heading_deg: number | null;
  smoothed_speed_kph: number | null;
  age_seconds: number;
}

export interface EtaEstimate {
  low_minutes: number;
  high_minutes: number;
}

export interface StopView extends LatLng {
  vendor_name: string;
  picked_up: boolean;
}
```

Add to `TrackResponse`:

```ts
  /** Null unless the order is in motion and the fix is fresh. Never a stale point. */
  courier: CourierFix | null;
  eta: EtaEstimate | null;
  /** Null for orders placed before the destination was recorded. */
  destination: LatLng | null;
  stops: StopView[];
```

- [ ] **Step 4: Give `pollIntervalMs` its first caller**

In `apps/omnideliv-app/app/track/[id].tsx`, delete the `const POLL_MS = 8000;` line and import the schedule:

```tsx
import { trackOrder, pollIntervalMs, type TrackResponse } from "@/api/tracking";
```

In the success branch of `poll`, replace the hardcoded reschedule:

```tsx
      // The schedule this screen should always have used: 5s while the courier
      // is moving and the map wants freshness, 15s otherwise, and nothing at
      // all once the order is terminal.
      const next_ms = pollIntervalMs(next.status);
      if (next_ms !== null) {
        timer.current = setTimeout(() => void poll(), next_ms);
      }
```

In the `catch` branch, keep retrying but on the slow cadence — a failing request is not a reason to poll faster:

```tsx
      setError("Couldn't refresh just now — still trying.");
      timer.current = setTimeout(() => void poll(), 15_000);
```

- [ ] **Step 5: Typecheck and test**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add apps/omnideliv-app/src/api/tracking.ts apps/omnideliv-app/app/track/\[id\].tsx apps/omnideliv-app/src/api/__tests__/tracking.test.ts
git commit -m "feat(omnideliv-app): telemetry response types, and a caller for pollIntervalMs"
```

---

## Task 9: The map seam and the canvas plot

**Files:**
- Create: `apps/omnideliv-app/src/components/map/project.ts`
- Create: `apps/omnideliv-app/src/components/map/__tests__/project.test.ts`
- Create: `apps/omnideliv-app/src/components/map/types.ts`
- Create: `apps/omnideliv-app/src/components/map/MapSurface.tsx`
- Create: `apps/omnideliv-app/src/components/map/CanvasPlot.tsx`

The projection is a separate pure module so it can be tested without rendering anything. Testing geometry through a component tree is slow and tests the tree, not the geometry.

- [ ] **Step 1: Write the failing projection tests**

`apps/omnideliv-app/src/components/map/__tests__/project.test.ts`:

```ts
import { projectAll } from "../project";

const BOX = { width: 300, height: 200, padding: 20 };

describe("projectAll", () => {
  it("places every point inside the padded box", () => {
    const pts = [
      { lat: 14.60, lng: 120.98 },
      { lat: 14.62, lng: 121.02 },
      { lat: 14.58, lng: 120.95 },
    ];
    for (const p of projectAll(pts, BOX)) {
      expect(p.x).toBeGreaterThanOrEqual(BOX.padding);
      expect(p.x).toBeLessThanOrEqual(BOX.width - BOX.padding);
      expect(p.y).toBeGreaterThanOrEqual(BOX.padding);
      expect(p.y).toBeLessThanOrEqual(BOX.height - BOX.padding);
    }
  });

  /** Screen y grows downwards; latitude grows northwards. */
  it("puts the northern point above the southern one", () => {
    const [north, south] = projectAll(
      [{ lat: 14.70, lng: 120.98 }, { lat: 14.50, lng: 120.98 }],
      BOX,
    );
    expect(north.y).toBeLessThan(south.y);
  });

  it("puts the eastern point right of the western one", () => {
    const [west, east] = projectAll(
      [{ lat: 14.60, lng: 120.90 }, { lat: 14.60, lng: 121.10 }],
      BOX,
    );
    expect(east.x).toBeGreaterThan(west.x);
  });

  /** One point, or several identical ones, must not divide by a zero span. */
  it("centres a degenerate set instead of dividing by zero", () => {
    for (const pts of [
      [{ lat: 14.6, lng: 120.98 }],
      [{ lat: 14.6, lng: 120.98 }, { lat: 14.6, lng: 120.98 }],
    ]) {
      for (const p of projectAll(pts, BOX)) {
        expect(Number.isFinite(p.x)).toBe(true);
        expect(Number.isFinite(p.y)).toBe(true);
        expect(p.x).toBeCloseTo(BOX.width / 2, 5);
        expect(p.y).toBeCloseTo(BOX.height / 2, 5);
      }
    }
  });

  it("returns nothing for no points", () => {
    expect(projectAll([], BOX)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd apps/omnideliv-app && npx jest src/components/map
```

Expected: FAIL — `Cannot find module '../project'`.

- [ ] **Step 3: Write the projection**

`apps/omnideliv-app/src/components/map/project.ts`:

```ts
/**
 * Lat/lng to screen coordinates inside a padded box.
 *
 * An equirectangular fit to the bounding box of the points, not a real map
 * projection — over a city-sized extent the distortion is invisible, and the
 * alternative pulls in a projection library for a picture with four dots on it.
 *
 * Pure and separate from the component so the geometry is testable without
 * rendering a tree.
 */
export interface LatLngLike {
  lat: number;
  lng: number;
}

export interface Box {
  width: number;
  height: number;
  padding: number;
}

export interface Point {
  x: number;
  y: number;
}

export function projectAll(points: LatLngLike[], box: Box): Point[] {
  if (points.length === 0) return [];

  const lats = points.map((p) => p.lat);
  const lngs = points.map((p) => p.lng);
  const minLat = Math.min(...lats);
  const maxLat = Math.max(...lats);
  const minLng = Math.min(...lngs);
  const maxLng = Math.max(...lngs);

  const usableW = box.width - box.padding * 2;
  const usableH = box.height - box.padding * 2;

  // A single point, or several at the same place, has zero span in one or both
  // axes. Scaling by that is a division by zero and renders NaN — which in
  // React Native is a silently invisible view, not an error anyone sees.
  const spanLat = maxLat - minLat;
  const spanLng = maxLng - minLng;

  return points.map((p) => ({
    x:
      spanLng === 0
        ? box.width / 2
        : box.padding + ((p.lng - minLng) / spanLng) * usableW,
    // Inverted: screen y grows downwards, latitude grows northwards.
    y:
      spanLat === 0
        ? box.height / 2
        : box.padding + ((maxLat - p.lat) / spanLat) * usableH,
  }));
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd apps/omnideliv-app && npx jest src/components/map
```

Expected: PASS, 5 tests.

- [ ] **Step 5: Define the seam**

Two files, because the contract must live in a leaf module. If `MapSurface.tsx`
both re-exported the implementation *and* owned the props type that the
implementation imports, the two would import each other in a cycle.

`apps/omnideliv-app/src/components/map/types.ts`:

```ts
/**
 * The map seam's contract.
 *
 * One interface, so a tile-backed implementation can replace the canvas plot
 * without the data path changing. This mirrors the admin portal, which renders
 * Mapbox when a token is configured and a canvas GPS plot when it is not — the
 * difference being that here the fallback ships first and the tiles come later.
 *
 * Alone in a leaf module on purpose: both the chooser and every implementation
 * import it, and nothing here imports either of them.
 */
import type { CourierFix, LatLng, StopView } from "@/api/tracking";

export interface MapSurfaceProps {
  /** Null when there is no fresh fix. Implementations must draw nothing. */
  courier: CourierFix | null;
  stops: StopView[];
  destination: LatLng | null;
}
```

`apps/omnideliv-app/src/components/map/MapSurface.tsx`:

```tsx
/**
 * The only map name a screen imports.
 *
 * Swapping in a tile-backed surface is a change to this file alone — the track
 * screen and the data path both stay as they are.
 */
import { CanvasPlot } from "./CanvasPlot";
import type { MapSurfaceProps } from "./types";

export type { MapSurfaceProps };

export function MapSurface(props: MapSurfaceProps) {
  return <CanvasPlot {...props} />;
}
```

- [ ] **Step 6: Write the canvas plot**

`apps/omnideliv-app/src/components/map/CanvasPlot.tsx`, complete:

```tsx
/**
 * A coordinate plot in pure React Native — no native module, no API key.
 *
 * `Animated` rather than Reanimated: Reanimated was deliberately dropped from
 * this app's dependencies (it needs a separate worklets package on this Expo
 * generation) and one pulsing dot is not a reason to bring it back.
 *
 * The width is measured at layout rather than assumed, so the plot is correct
 * on a 360 dp phone and on a tablet without a breakpoint.
 */
import { useEffect, useRef, useState } from "react";
import { Animated, Easing, Text, View } from "react-native";

import type { CourierFix, StopView } from "@/api/tracking";
import { theme } from "@/theme";
import { projectAll } from "./project";
import type { MapSurfaceProps } from "./types";

const HEIGHT = 220;
const MIN_HEIGHT = 160;
const PADDING = 28;

export function CanvasPlot({ courier, stops, destination }: MapSurfaceProps) {
  const pulse = useRef(new Animated.Value(0)).current;
  const [width, setWidth] = useState(0);

  useEffect(() => {
    if (!courier) return;
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulse, {
          toValue: 1,
          duration: 1100,
          easing: Easing.out(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(pulse, { toValue: 0, duration: 0, useNativeDriver: true }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [courier, pulse]);

  // Nothing to anchor the picture to. A lone courier dot in an empty frame
  // tells a customer less than the screen saying so in words.
  if (!destination) return null;

  const points = [
    ...stops.map((s) => ({ lat: s.lat, lng: s.lng })),
    { lat: destination.lat, lng: destination.lng },
    ...(courier ? [{ lat: courier.lat, lng: courier.lng }] : []),
  ];

  return (
    <View
      accessibilityLabel="Delivery map"
      onLayout={(e) => setWidth(e.nativeEvent.layout.width)}
      style={{
        height: HEIGHT,
        minHeight: MIN_HEIGHT,
        borderRadius: theme.radius.md,
        borderColor: theme.border,
        borderWidth: 1,
        backgroundColor: "rgba(255,255,255,0.03)",
        overflow: "hidden",
      }}
    >
      {/* Nothing is drawn until the container has been measured — projecting
          into a zero-width box would put every marker in the same place. */}
      {width > 0 && (
        <PlotBody
          points={points}
          stops={stops}
          courier={courier}
          pulse={pulse}
          box={{ width, height: HEIGHT, padding: PADDING }}
        />
      )}
    </View>
  );
}

function PlotBody({
  points,
  stops,
  courier,
  pulse,
  box,
}: {
  points: { lat: number; lng: number }[];
  stops: StopView[];
  courier: CourierFix | null;
  pulse: Animated.Value;
  box: { width: number; height: number; padding: number };
}) {
  // Same order the points array was built in: stops, destination, courier.
  const xy = projectAll(points, box);
  const stopPts = xy.slice(0, stops.length);
  const destPt = xy[stops.length];
  const courierPt = courier ? xy[stops.length + 1] : null;

  return (
    <>
      {stopPts.map((p, i) => (
        <View
          key={i}
          style={{ position: "absolute", left: p.x - 6, top: p.y - 6, alignItems: "center" }}
        >
          <View
            style={{
              width: 12,
              height: 12,
              borderRadius: 6,
              borderWidth: 2,
              borderColor: stops[i].picked_up ? theme.green : theme.muted,
              backgroundColor: stops[i].picked_up ? theme.green : "transparent",
            }}
          />
          {/* Truncated, not wrapped: a long shop name must not reflow the plot. */}
          <Text
            numberOfLines={1}
            style={{ color: theme.faint, fontSize: 9, marginTop: 2, maxWidth: 70 }}
          >
            {stops[i].vendor_name}
          </Text>
        </View>
      ))}

      {destPt && (
        <View
          style={{ position: "absolute", left: destPt.x - 7, top: destPt.y - 7, alignItems: "center" }}
        >
          <View style={{ width: 14, height: 14, borderRadius: 3, backgroundColor: theme.cyan }} />
          <Text style={{ color: theme.faint, fontSize: 9, marginTop: 2 }}>You</Text>
        </View>
      )}

      {courierPt && (
        <Animated.View
          accessibilityLabel="Courier location"
          style={{
            position: "absolute",
            left: courierPt.x - 8,
            top: courierPt.y - 8,
            width: 16,
            height: 16,
            borderRadius: 8,
            backgroundColor: theme.amber,
            transform: [
              { scale: pulse.interpolate({ inputRange: [0, 1], outputRange: [1, 1.6] }) },
            ],
            opacity: pulse.interpolate({ inputRange: [0, 1], outputRange: [1, 0.35] }),
          }}
        />
      )}
    </>
  );
}
```

Check `apps/omnideliv-app/src/theme.ts` for the real token names (`green`, `amber`, `cyan`, `muted`, `faint`, `border`, `surface`, `radius.md`) and use those — do not invent a token. If one is missing, add it to the theme rather than hardcoding a hex value inline.

- [ ] **Step 6b: Test the degrade states render**

`apps/omnideliv-app/src/components/map/__tests__/CanvasPlot.test.tsx`. The plot must be correct when things are *missing*, which is exactly when nobody checks it by hand.

```tsx
import { render } from "@testing-library/react-native";

import { CanvasPlot } from "../CanvasPlot";
import type { CourierFix, StopView } from "@/api/tracking";

const DEST = { lat: 14.5995, lng: 120.9842 };
const STOPS: StopView[] = [
  { vendor_name: "Kuya's", lat: 14.601, lng: 120.985, picked_up: true },
  { vendor_name: "Suki mart", lat: 14.603, lng: 120.987, picked_up: false },
];
const FIX: CourierFix = {
  lat: 14.602, lng: 120.986, heading_deg: null, smoothed_speed_kph: 20, age_seconds: 5,
};

/** Layout does not fire in the test renderer, so drive it directly. */
function measured(ui: ReturnType<typeof render>) {
  ui.getByLabelText("Delivery map").props.onLayout({
    nativeEvent: { layout: { width: 320, height: 220 } },
  });
  return ui;
}

describe("CanvasPlot", () => {
  it("draws the courier when there is a fresh fix", () => {
    const ui = measured(render(<CanvasPlot courier={FIX} stops={STOPS} destination={DEST} />));
    expect(ui.queryByLabelText("Courier location")).not.toBeNull();
  });

  /** The one that matters: no dot at all beats a stale dot read as live. */
  it("draws no courier when there is no fix", () => {
    const ui = measured(render(<CanvasPlot courier={null} stops={STOPS} destination={DEST} />));
    expect(ui.queryByLabelText("Courier location")).toBeNull();
    expect(ui.queryByText("Kuya's")).not.toBeNull();
  });

  it("renders nothing at all without a destination", () => {
    const ui = render(<CanvasPlot courier={FIX} stops={STOPS} destination={null} />);
    expect(ui.queryByLabelText("Delivery map")).toBeNull();
  });

  it("survives an order with no stops", () => {
    const ui = measured(render(<CanvasPlot courier={FIX} stops={[]} destination={DEST} />));
    expect(ui.queryByLabelText("Courier location")).not.toBeNull();
    expect(ui.queryByText("You")).not.toBeNull();
  });
});
```

**Before writing this, check whether `@testing-library/react-native` is installed:**

```bash
cd apps/omnideliv-app && npm ls @testing-library/react-native
```

It was **deliberately dropped** during the initial app build because it pulled an incompatible `react-test-renderer`. If it is absent, do not add it to satisfy this test — a dependency conflict that breaks the whole suite costs more than these four assertions. Instead, delete this step and extract the marker-selection logic into `project.ts` as a pure function:

```ts
/** Which markers to draw, given what is present. Pure, so it is testable. */
export function markers(stopCount: number, hasCourier: boolean) {
  return { stopSlice: [0, stopCount] as const, destIndex: stopCount, courierIndex: hasCourier ? stopCount + 1 : null };
}
```

and test that instead, in the existing `project.test.ts`. Record in the commit message which of the two routes you took.

- [ ] **Step 6c: Run the tests**

```bash
cd apps/omnideliv-app && npx jest src/components/map
```

Expected: PASS. Then mutation-check the one that matters — change `courier ? xy[stops.length + 1] : null` to `xy[stops.length + 1]` and re-run.

Expected: `draws no courier when there is no fix` FAILS (or its pure equivalent). Revert and confirm green.

- [ ] **Step 7: Typecheck**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both clean.

- [ ] **Step 8: Commit**

```bash
git add apps/omnideliv-app/src/components/map
git commit -m "feat(omnideliv-app): a pure-RN courier plot behind a swappable map seam"
```

---

## Task 10: Assemble the track screen

**Files:**
- Modify: `apps/omnideliv-app/app/track/[id].tsx`

- [ ] **Step 1: Render the ETA line**

Below the existing status headline, add:

```tsx
        {order.eta && (
          <Text style={{ color: theme.text, fontSize: 15, fontWeight: "600" }}>
            {order.eta.low_minutes === order.eta.high_minutes
              ? `Arriving in about ${order.eta.low_minutes} min`
              : `Arriving in ${order.eta.low_minutes}–${order.eta.high_minutes} min`}
          </Text>
        )}
```

- [ ] **Step 2: Render the map with its degrade states**

Add above the totals card:

```tsx
        {/* Four states, not one. A map that only works when everything is
            present is a map that is blank exactly when someone is worried. */}
        {order.destination && (
          <View style={{ gap: 6 }}>
            <MapSurface
              courier={order.courier}
              stops={order.stops}
              destination={order.destination}
            />
            {!order.courier && order.status !== "delivered" && order.status !== "cancelled" && (
              <Text style={{ color: theme.faint, fontSize: 12 }}>
                Waiting for the courier's location.
              </Text>
            )}
          </View>
        )}
```

Wrap it so the plot disappears on a terminal order — a finished delivery does not need a map, and the backend returns no position for one anyway:

```tsx
        {order.destination && order.status !== "delivered" && order.status !== "cancelled" && ( ... )}
```

Import it: `import { MapSurface } from "@/components/map/MapSurface";`

- [ ] **Step 3: Render the milestone strip**

Replace the existing bare timeline loop with one that also shows what has not happened yet. Add above the component:

```tsx
/** The steps every order passes through, so the list shows what is still to come. */
const FUTURE_STEPS: { key: TrackResponse["status"][]; label: string }[] = [
  { key: ["placed", "awaiting_courier"], label: "Courier accepted" },
  { key: ["placed", "awaiting_courier", "collecting"], label: "All items collected" },
  { key: ["placed", "awaiting_courier", "collecting", "delivering"], label: "Delivered" },
];
```

And after the existing timeline `.map(...)`, render the pending steps dimmed:

```tsx
          {order.status !== "cancelled" &&
            FUTURE_STEPS.filter((s) => s.key.includes(order.status)).map((s) => (
              <View key={s.label} style={{ flexDirection: "row", gap: 10 }}>
                <View style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: theme.faint, marginTop: 6, opacity: 0.4 }} />
                <Text style={{ color: theme.faint, fontSize: 13, opacity: 0.6 }}>{s.label}</Text>
              </View>
            ))}
```

- [ ] **Step 4: Typecheck and test**

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: both clean.

- [ ] **Step 5: Verify the bundle actually resolves the new imports**

`tsc --noEmit` has passed through three separate states that made this app unbuildable, and `expo export` cannot run on this Windows machine (`hermesc` rejects `#private` fields in a dependency, regardless of your code). The available local gate is the Metro module count.

```bash
cd apps/omnideliv-app && npx expo-doctor
```

Expected: no new failures versus before this branch.

Then start Metro and record the module count, comparing against a `git stash`ed baseline. A count that did not rise means an import was silently dropped rather than resolved.

- [ ] **Step 6: Audit responsiveness**

Required by CLAUDE.md. Run the app and check the track screen at a small viewport (≤360 dp wide) and a large one:

- the plot scales to its container and never collapses below its 160 dp floor
- vendor labels truncate rather than overlapping (`numberOfLines={1}` and a `maxWidth`)
- the milestone list scrolls with the page rather than pushing the plot off-screen
- the ETA line wraps rather than clipping

Fix anything that breaks before committing.

- [ ] **Step 7: Commit**

```bash
git add apps/omnideliv-app/app/track/\[id\].tsx
git commit -m "feat(omnideliv-app): live plot, ETA and forward milestones on the track screen"
```

---

## Task 11: End-to-end verification

No new code. This exists because a green build has repeatedly not meant working software in this repo.

- [ ] **Step 1: Full test run**

```bash
CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops -p logisticos-omnideliv
```

```bash
cd apps/omnideliv-app && npx tsc --noEmit && npx jest
```

Expected: all green. Record the actual counts — do not claim a pass you have not read.

- [ ] **Step 2: Clippy**

```bash
CARGO_INCREMENTAL=0 cargo clippy -p logisticos-field-ops -p logisticos-omnideliv -- -D warnings
```

Expected: clean.

- [ ] **Step 3: Confirm the position route is not reachable unauthenticated**

Against a running field-ops:

```bash
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:8090/v1/field-ops/assignments/00000000-0000-0000-0000-000000000000/position
```

Expected: `401`. **A `404` here means the route is mounted outside the auth layer** — that is a live location leak, not a routing detail. Stop and fix Task 4 Step 2.

- [ ] **Step 4: Confirm the disclosure gate against a real order**

Track an order that has been delivered:

```bash
curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:8000/v1/omnideliv/orders/$ORDER_ID/track | grep -o '"courier":[^,]*'
```

Expected: `"courier":null` for a delivered order, and a populated object for one in `collecting` or `delivering`.

Note: `scripts/seed-omnideliv.sh` must be re-run within 10 minutes of testing, because `find_available_near` only considers GPS fixes from the last 10 minutes. A stale seeded courier makes checkout fail with "no courier available", which looks like a dispatch bug and is not.

- [ ] **Step 5: Confirm the customer scoping still holds over the wider payload**

This read was already fixed once: it checked the tenant and never the customer, so any signed-in user could track any order in the tenant by id. That payload has just grown to carry a live courier position and a home address, so the scoping needs re-confirming against the wider response rather than assumed to have survived.

With `$TOKEN_A` owning `$ORDER_ID` and `$TOKEN_B` belonging to a different customer in the same tenant:

```bash
curl -s -o /dev/null -w 'owner:%{http_code}\n' -H "Authorization: Bearer $TOKEN_A" http://localhost:8000/v1/omnideliv/orders/$ORDER_ID/track
curl -s -o /dev/null -w 'other:%{http_code}\n' -H "Authorization: Bearer $TOKEN_B" http://localhost:8000/v1/omnideliv/orders/$ORDER_ID/track
```

Expected: `owner:200` and `other:404`. **It must be 404, not 403** — a 403 confirms the id names a real order to someone who guessed it.

Then confirm the non-owner response body carries no coordinates at all:

```bash
curl -s -H "Authorization: Bearer $TOKEN_B" http://localhost:8000/v1/omnideliv/orders/$ORDER_ID/track \
  | grep -cE '"lat"|"lng"|"courier"'
```

Expected: `0`. A non-zero count means the new fields are being serialized on a path that the customer check does not cover.

- [ ] **Step 6: Commit any fixes and stop**

Do not merge. Hand back for review with the recorded test counts and the two curl results.

---

## Notes for the implementer

- **If the money-surface plan has already run**, the track screen's totals card is now a `<Receipt />` component. Task 10 Step 2 says to add the map "above the totals card" — add it above the `<Receipt />` instead. Both plans extend the same `TrackResponse` interface with different fields; the additions do not collide in either order.
- **Do not add a courier id to any customer-facing payload.** `CourierFix` carries a position, not an identity, deliberately. field-ops returns `courier_id` on the internal route; omnideliv drops it.
- **Do not cache the position yet.** At 5-second polling each open track screen becomes an outbound field-ops call. This is recorded as a known risk in the spec; a short-TTL cache is the obvious relief and is deliberately out of scope so its behaviour is chosen deliberately rather than as a patch.
- **`speed_kph` may never be populated** by the current ingest route. That is why `DEFAULT_SPEED_KPH` exists and is tested. If you find the ingest route does accept a speed, do not change the fallback — the fallback is what makes the ETA work for couriers whose devices do not report one.
- **The two `FIX_STALE_AFTER_SECS` constants are intentionally duplicated** across the services. Do not "fix" this by creating a shared crate for one integer; the services do not otherwise depend on each other and coupling them for this would be the larger mistake.
