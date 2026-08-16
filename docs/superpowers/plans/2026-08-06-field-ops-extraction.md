# Field-Ops Tier Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `services/field-ops` as a platform-tier service owning courier identity, assignment/claim and GPS ingest, so OmniDeliv can dispatch couriers without violating ADR-0009's product-isolation rule.

**Architecture:** A new Rust/Axum service on the standard hexagonal layout, schema `field_ops`, migrating via `logisticos_common::migrations::run` per ADR-0012. It owns three tables: `couriers`, `courier_assignments`, `courier_locations`. Assignment claiming uses a Postgres compare-and-swap so two products racing for the same courier cannot both win. LogisticOS keeps POD/POP, hubs, carriers and parcel routing; its migration onto this service is deliberately **not** in this plan.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx, PostgreSQL + PostGIS, Kafka (`logisticos-events`), JWT auth (`logisticos-auth`).

---

## Scope

**In:** ADR-0015, the service skeleton, courier identity, assignment + atomic claim, GPS ingest.

**Out, deliberately:**

- **The earnings ledger.** The spec lists it under field-ops, but it is a money path whose shape depends on `order_vendor_legs` and the three-leg split — neither of which exists until Plan 5. Building it here means guessing at the settlement shape and rewriting it. It moves to Plan 5, where it can be built against the real order model and the settlement invariant test.
- **Migrating LogisticOS onto this service.** Per the spec's prerequisite table this is explicitly *not* a slice-one blocker — OmniDeliv can consume `field-ops` while LogisticOS keeps running its own `driver_ops.drivers`. ADR-0015 must carry the dated commitment; the migration is its own plan.

**Not a goal:** deduplicating `driver_ops.drivers` and `field_ops.couriers` in this plan. They coexist until the migration plan runs. That duplication is the cost ADR-0015 must state plainly rather than hide.

---

## Prerequisites

Read before starting:

- [docs/adr/0009-multi-product-platform-gateway-topology.md](../../adr/0009-multi-product-platform-gateway-topology.md) — the boundary rules this plan exists to satisfy, especially "watch the field-ops cluster"
- [services/pod/src/bootstrap.rs](../../../services/pod/src/bootstrap.rs) — the service bootstrap pattern to copy (`search_path` in `after_connect`, then `migrations::run`)
- [services/driver-ops/src/domain/entities/driver.rs](../../../services/driver-ops/src/domain/entities/driver.rs) — the entity being generalised
- [services/driver-ops/migrations/0005_disable_rls_drivers.sql](../../../services/driver-ops/migrations/0005_disable_rls_drivers.sql) — read this before writing any migration; see the tenancy note below

**Disk:** clear `C:\cargo-target-logisticos\debug\incremental` and export `CARGO_INCREMENTAL=0` before starting.

### Tenancy: do not write a decorative RLS policy

52 migrations in this repo run `ENABLE ROW LEVEL SECURITY` with `USING (tenant_id = current_setting('app.tenant_id', true)::UUID)`. **No service ever sets `app.tenant_id` on its DB session.** Services connect as the schema owner, and PostgreSQL bypasses RLS for table owners unless `FORCE ROW LEVEL SECURITY` is set — so the policies neither filter nor fail. Where `FORCE` *was* added it broke reads and was reverted (`order-intake/0007`, `driver-ops/0005`).

Actual tenant isolation across this platform is application-layer: an explicit `WHERE tenant_id = $n` in every repository query.

**This plan does not add a policy that does nothing.** It states the real mechanism in the migration, and Task 4 adds a test that enforces it. Making RLS genuinely work is a platform-wide change (per-request `SET LOCAL app.tenant_id`, connection-pool implications, 52 tables) and needs its own ADR — flagged at the end of this plan, not smuggled into it.

---

## File Structure

**New — `services/field-ops/`:**

| File | Responsibility |
|---|---|
| `Cargo.toml` | Manifest; workspace member |
| `migrations/0001_create_couriers.sql` | Schema + `couriers` |
| `migrations/0002_create_assignments.sql` | `products` registry + `courier_assignments` + claim index |
| `migrations/0003_create_locations.sql` | `courier_locations` + PostGIS GiST + `courier_latest_locations` view |
| `src/main.rs`, `src/lib.rs`, `src/bootstrap.rs`, `src/config.rs` | Wiring |
| `src/domain/entities/courier.rs` | `Courier`, `CourierStatus` |
| `src/domain/entities/assignment.rs` | `CourierAssignment`, `AssignmentStatus` |
| `src/domain/entities/location.rs` | `CourierLocation` |
| `src/domain/repositories/mod.rs` | Repository traits |
| `src/infrastructure/db/courier_repo.rs` | Postgres courier repo |
| `src/infrastructure/db/assignment_repo.rs` | Postgres assignment repo + CAS claim |
| `src/infrastructure/db/location_repo.rs` | Postgres location repo |
| `src/application/services/*.rs` | Courier, assignment, location services |
| `src/api/http/*.rs` | Routes + health/ready/metrics |

**Modified:** root `Cargo.toml` (workspace member), `.github/workflows/build-images.yml`, `docs/adr/0015-field-ops-platform-tier.md` (new).

---

## Task 1: ADR-0015 — DONE, accepted

**Status: complete.** [docs/adr/0015-field-ops-platform-tier.md](../../adr/0015-field-ops-platform-tier.md) is written and its status is **Accepted**. The gate this task guarded is open; Task 2 may proceed.

This task previously carried the full ADR text inline. That copy has been deleted rather than updated, because it had already drifted from the accepted decision in three places and a stale duplicate of a decision record is worse than no duplicate — the next reader cannot tell which one is authoritative. Read the file.

**What changed between the draft embedded here and the accepted version** — these are the amendments this plan now implements, so they are worth knowing before writing any of it:

1. **The two-quarter migration commitment is gone**, replaced by a prerequisite. The blocker was named instead of scheduled around: `driver_ops` carries a `drivers.id` / `user_id` split-brain, and collapsing it is now unblocked work in `driver_ops`, justified on its own merits as a latent correctness bug and decoupled from OmniDeliv's timeline. With one unambiguous courier identity, convergence becomes a repository swap rather than a data-model programme.

2. **`field_ops` inherits the stronger location model.** Not `last_lat`/`last_lng` with a btree — a `courier_locations` history table, a PostGIS GiST index, and a `courier_latest_locations` view, mirroring `driver_ops`. Tasks 3, 5 and 7 below reflect this.

3. **`product` is a foreign key to a registry table, not a `CHECK` enumeration**, and the Rust side is an opaque `ProductKey`, not an enum. Admitting a third consumer is an `INSERT`. Task 6 reflects this.

**One work item this plan does not contain.** The `drivers.id` / `user_id` collapse belongs to `driver_ops` and needs its own plan; it is a prerequisite for *convergence*, not for this extraction, so `field-ops` can be built while it proceeds in parallel. Do not fold it in here — that would recreate the coupling the amendment exists to remove.

---

## Task 2: Scaffold the service

**Files:**
- Create: `services/field-ops/Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/config.rs`
- Modify: root `Cargo.toml`

- [ ] **Step 1: Write the manifest**

```toml
# services/field-ops/Cargo.toml
[package]
name        = "logisticos-field-ops"
description = "Field-Ops platform tier — courier identity, assignment, GPS ingest"
version.workspace      = true
edition.workspace      = true
authors.workspace      = true
rust-version.workspace = true

[[bin]]
name = "field_ops"
path = "src/main.rs"

[dependencies]
logisticos-common.workspace  = true
logisticos-errors.workspace  = true
logisticos-auth.workspace    = true
logisticos-tracing.workspace = true
logisticos-types.workspace   = true
logisticos-events.workspace  = true
tokio.workspace       = true
axum.workspace        = true
tower-http.workspace  = true
sqlx.workspace        = true
serde.workspace       = true
serde_json.workspace  = true
thiserror.workspace   = true
anyhow.workspace      = true
uuid.workspace        = true
chrono.workspace      = true
config.workspace      = true
dotenvy.workspace     = true
validator.workspace   = true
rdkafka.workspace     = true
tracing.workspace     = true
async-trait.workspace = true

[dev-dependencies]
tokio      = { version = "1", features = ["macros", "rt-multi-thread"] }
uuid       = { version = "1", features = ["v4"] }
serde_json = "1"
```

- [ ] **Step 2: Write the entrypoint and lib root**

```rust
// services/field-ops/src/main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_field_ops::bootstrap::run().await
}
```

```rust
// services/field-ops/src/lib.rs
#![deny(clippy::all)]

pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;
```

- [ ] **Step 3: Write the config**

```rust
// services/field-ops/src/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub env:  String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url:             String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

fn default_max_connections() -> u32 { 10 }

#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    pub brokers: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,

    /// A courier's claim is released if no heartbeat arrives within this window,
    /// so a crashed client cannot hold a courier hostage forever.
    #[serde(default = "default_claim_ttl_secs")]
    pub claim_ttl_secs: i64,
}

fn default_claim_ttl_secs() -> i64 { 120 }

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8090)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
```

- [ ] **Step 4: Register the workspace member**

In the root `Cargo.toml`, add `"services/field-ops",` to `members`, directly after `"services/driver-ops",`.

- [ ] **Step 5: Verify it resolves**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`
Expected: FAIL — `file not found for module 'api'` plus four similar. Manifest and workspace wiring are correct; the modules are what's missing.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml services/field-ops/
git commit -m "feat(field-ops): scaffold service crate and config"
```

---

## Task 3: Courier table and schema

**Files:**
- Create: `services/field-ops/migrations/0001_create_couriers.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Field-Ops platform tier. Owns the human operating in the field, shared by
-- every product that dispatches one. See ADR-0015.
CREATE SCHEMA IF NOT EXISTS field_ops;

-- TENANCY NOTE — read before adding an RLS policy here.
-- Other schemas in this repo run ENABLE ROW LEVEL SECURITY with a policy on
-- current_setting('app.tenant_id'). No service sets that variable, and services
-- connect as the schema owner, so PostgreSQL bypasses the policy entirely — it
-- neither filters nor fails. Where FORCE was added it broke reads and was
-- reverted (order-intake/0007, driver-ops/0005).
-- Isolation here is application-layer: every repository query filters on
-- tenant_id explicitly, enforced by a test. A decorative policy would imply a
-- database-level guarantee that does not exist, so this migration omits one.
-- Making RLS genuinely enforce is a platform-wide change and needs its own ADR.

CREATE TABLE IF NOT EXISTS field_ops.couriers (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    -- Links to identity.users. The courier is a platform-tier worker identity,
    -- distinct from the customer profile in the CDP.
    user_id        UUID        NOT NULL,
    first_name     TEXT        NOT NULL,
    last_name      TEXT        NOT NULL,
    phone          TEXT        NOT NULL,
    status         TEXT        NOT NULL DEFAULT 'offline'
                               CHECK (status IN ('offline','available','assigned','on_break')),
    vehicle_type   TEXT,
    zone           TEXT,
    -- CACHE ONLY. The authoritative position is the latest row in
    -- field_ops.courier_locations (migration 0003); these columns exist so a
    -- courier list renders without touching the time-series table. Never
    -- proximity-search on them — see the GiST index in 0003.
    last_lat       DOUBLE PRECISION,
    last_lng       DOUBLE PRECISION,
    last_seen_at   TIMESTAMPTZ,
    is_active      BOOLEAN     NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- One courier record per user per tenant.
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_user
    ON field_ops.couriers (tenant_id, user_id);

-- Serves both "the tenant's active roster" and the status-narrowing half of a
-- supply lookup. One index, not two: an earlier draft added a second on
-- (tenant_id, status) WHERE status = 'available', which is a strict subset of
-- this one and buys nothing a planner cannot already get here — it would only
-- add write cost on a table that couriers update on every status change.
--
-- The geospatial half of a supply lookup runs against courier_latest_locations
-- (0003), which has the GiST index. There is deliberately NO btree on
-- (tenant_id, last_lat, last_lng): proximity search is ST_DWithin against a
-- geography, which a btree on two float columns cannot serve — it would sit
-- there looking useful while every search scanned.
CREATE INDEX IF NOT EXISTS idx_courier_tenant_status
    ON field_ops.couriers (tenant_id, status)
    WHERE is_active;
```

> **Why this is not the simpler denormalised model.** Per ADR-0015, `field_ops`
> inherits the location model `driver_ops` already has rather than a cheaper one:
> a history table with a PostGIS GiST index and a latest-fix view, built in
> **Task 7**. This is the single dimension where the existing product-tier table
> is ahead of the new platform tier, and shipping the weaker version would force
> a later choice between downgrading LogisticOS at convergence or rewriting
> `field-ops` a second time.

- [ ] **Step 2: Verify the SQL parses**

The service is not running yet, so check syntax against a scratch database:

```bash
docker exec -i logisticos-postgres psql -U logisticos -d postgres -v ON_ERROR_STOP=1 -c "CREATE DATABASE fieldops_syntax_check" && docker exec -i logisticos-postgres psql -U logisticos -d fieldops_syntax_check -v ON_ERROR_STOP=1 < services/field-ops/migrations/0001_create_couriers.sql && echo "SQL OK"
```

Expected: `SQL OK`. Then drop it: `docker exec logisticos-postgres psql -U logisticos -d postgres -c "DROP DATABASE fieldops_syntax_check"`

- [ ] **Step 3: Commit**

```bash
git add services/field-ops/migrations/0001_create_couriers.sql
git commit -m "feat(field-ops): couriers table, application-layer tenancy documented"
```

---

## Task 4: Courier entity + the tenant-filter invariant

**Files:**
- Create: `services/field-ops/src/domain/mod.rs`, `src/domain/entities/mod.rs`, `src/domain/entities/courier.rs`, `src/domain/repositories/mod.rs`

- [ ] **Step 1: Write the failing test**

```rust
// services/field-ops/src/domain/entities/courier.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn courier() -> Courier {
        Courier::new(Uuid::new_v4(), Uuid::new_v4(), "Rico".into(), "M".into(), "+639170000000".into())
    }

    #[test]
    fn a_new_courier_starts_offline_and_unavailable() {
        let c = courier();
        assert_eq!(c.status, CourierStatus::Offline);
        assert!(!c.is_dispatchable());
    }

    #[test]
    fn only_an_available_active_courier_is_dispatchable() {
        let mut c = courier();
        c.go_available();
        assert!(c.is_dispatchable());

        c.go_offline();
        assert!(!c.is_dispatchable());

        c.go_available();
        c.is_active = false;
        assert!(!c.is_dispatchable(), "a deactivated courier must never be dispatchable");
    }

    /// An assigned courier is not offerable to a second product — this is the
    /// entity-level half of ADR-0015's load-bearing invariant.
    #[test]
    fn an_assigned_courier_is_not_dispatchable() {
        let mut c = courier();
        c.go_available();
        c.mark_assigned();
        assert_eq!(c.status, CourierStatus::Assigned);
        assert!(!c.is_dispatchable());
    }

    #[test]
    fn recording_a_position_updates_last_seen() {
        let mut c = courier();
        assert!(c.last_seen_at.is_none());
        c.record_position(14.5995, 120.9842);
        assert_eq!(c.last_lat, Some(14.5995));
        assert!(c.last_seen_at.is_some());
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops courier::`
Expected: FAIL to compile — `cannot find type 'Courier' in this scope`.

- [ ] **Step 3: Write the entity**

```rust
//! The human operating in the field. Platform-tier: shared by every product
//! that dispatches one, distinct from the customer profile in the CDP.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CourierStatus {
    Offline,
    Available,
    Assigned,
    OnBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Courier {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub user_id:      Uuid,
    pub first_name:   String,
    pub last_name:    String,
    pub phone:        String,
    pub status:       CourierStatus,
    pub vehicle_type: Option<String>,
    pub zone:         Option<String>,
    pub last_lat:     Option<f64>,
    pub last_lng:     Option<f64>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub is_active:    bool,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

impl Courier {
    pub fn new(
        tenant_id: Uuid,
        user_id: Uuid,
        first_name: String,
        last_name: String,
        phone: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            user_id,
            first_name,
            last_name,
            phone,
            status: CourierStatus::Offline,
            vehicle_type: None,
            zone: None,
            last_lat: None,
            last_lng: None,
            last_seen_at: None,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Can this courier be offered work right now? Deliberately conservative:
    /// anything other than an active, available courier is a no.
    pub fn is_dispatchable(&self) -> bool {
        self.is_active && self.status == CourierStatus::Available
    }

    pub fn go_available(&mut self) { self.set_status(CourierStatus::Available); }
    pub fn go_offline(&mut self)   { self.set_status(CourierStatus::Offline); }
    pub fn mark_assigned(&mut self) { self.set_status(CourierStatus::Assigned); }

    fn set_status(&mut self, status: CourierStatus) {
        self.status = status;
        self.updated_at = Utc::now();
    }

    pub fn record_position(&mut self, lat: f64, lng: f64) {
        self.last_lat = Some(lat);
        self.last_lng = Some(lng);
        self.last_seen_at = Some(Utc::now());
        self.updated_at = Utc::now();
    }
}
```

Add the module wiring:

```rust
// services/field-ops/src/domain/mod.rs
pub mod entities;
pub mod repositories;
```

```rust
// services/field-ops/src/domain/entities/mod.rs
pub mod courier;
pub use courier::{Courier, CourierStatus};
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops courier::`
Expected: PASS — 4 passed.

- [ ] **Step 5: Write the repository trait**

```rust
// services/field-ops/src/domain/repositories/mod.rs
//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` as its first argument, by design.
//! There is no database-level policy enforcing isolation in this schema (see
//! migration 0001), so the signature is the enforcement point — a method that
//! can be called without a tenant is a method that can leak across tenants.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::Courier;

#[async_trait]
pub trait CourierRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>>;
    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>>;
    async fn save(&self, courier: &Courier) -> anyhow::Result<()>;

    /// Dispatchable couriers within `radius_km` of a point, nearest first.
    async fn find_available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Courier>>;
}
```

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/src/domain/
git commit -m "feat(field-ops): Courier entity and tenant-scoped repository contract"
```

---

## Task 5: Postgres courier repository

> **Ordering note.** `find_available_near` below queries `field_ops.courier_latest_locations`, which migration 0003 creates in Task 7. Nothing here breaks — these are runtime `sqlx::query` calls, not compile-checked `query!` macros, so this task's `cargo check` passes regardless. But do not point it at a database migrated only through 0001 and expect the proximity search to run; it needs the whole migration set applied, which `bootstrap` does in one pass.

**Files:**
- Create: `services/field-ops/src/infrastructure/mod.rs`, `src/infrastructure/db/mod.rs`, `src/infrastructure/db/courier_repo.rs`

- [ ] **Step 1: Write the implementation**

```rust
// services/field-ops/src/infrastructure/db/courier_repo.rs
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Courier, CourierStatus};
use crate::domain::repositories::CourierRepository;

pub struct PgCourierRepository {
    pool: PgPool,
}

impl PgCourierRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Courier> {
    let status_str: String = r.get("status");
    let status = match status_str.as_str() {
        "offline"   => CourierStatus::Offline,
        "available" => CourierStatus::Available,
        "assigned"  => CourierStatus::Assigned,
        "on_break"  => CourierStatus::OnBreak,
        other => anyhow::bail!("unknown courier status in database: {other}"),
    };

    Ok(Courier {
        id:           r.get("id"),
        tenant_id:    r.get("tenant_id"),
        user_id:      r.get("user_id"),
        first_name:   r.get("first_name"),
        last_name:    r.get("last_name"),
        phone:        r.get("phone"),
        status,
        vehicle_type: r.get("vehicle_type"),
        zone:         r.get("zone"),
        last_lat:     r.get("last_lat"),
        last_lng:     r.get("last_lng"),
        last_seen_at: r.get("last_seen_at"),
        is_active:    r.get("is_active"),
        created_at:   r.get("created_at"),
        updated_at:   r.get("updated_at"),
    })
}

fn status_str(s: CourierStatus) -> &'static str {
    match s {
        CourierStatus::Offline   => "offline",
        CourierStatus::Available => "available",
        CourierStatus::Assigned  => "assigned",
        CourierStatus::OnBreak   => "on_break",
    }
}

#[async_trait]
impl CourierRepository for PgCourierRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Courier>> {
        let row = sqlx::query(
            "SELECT * FROM field_ops.couriers WHERE tenant_id = $1 AND id = $2",
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(map_row).transpose()
    }

    async fn find_by_user(&self, tenant_id: Uuid, user_id: Uuid) -> anyhow::Result<Option<Courier>> {
        let row = sqlx::query(
            "SELECT * FROM field_ops.couriers WHERE tenant_id = $1 AND user_id = $2",
        )
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.as_ref().map(map_row).transpose()
    }

    async fn save(&self, c: &Courier) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.couriers (
                id, tenant_id, user_id, first_name, last_name, phone, status,
                vehicle_type, zone, last_lat, last_lng, last_seen_at, is_active,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)
            ON CONFLICT (id) DO UPDATE SET
                first_name   = EXCLUDED.first_name,
                last_name    = EXCLUDED.last_name,
                phone        = EXCLUDED.phone,
                status       = EXCLUDED.status,
                vehicle_type = EXCLUDED.vehicle_type,
                zone         = EXCLUDED.zone,
                last_lat     = EXCLUDED.last_lat,
                last_lng     = EXCLUDED.last_lng,
                last_seen_at = EXCLUDED.last_seen_at,
                is_active    = EXCLUDED.is_active,
                updated_at   = EXCLUDED.updated_at
            "#,
        )
        .bind(c.id).bind(c.tenant_id).bind(c.user_id)
        .bind(&c.first_name).bind(&c.last_name).bind(&c.phone)
        .bind(status_str(c.status))
        .bind(&c.vehicle_type).bind(&c.zone)
        .bind(c.last_lat).bind(c.last_lng).bind(c.last_seen_at)
        .bind(c.is_active).bind(c.created_at).bind(c.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_available_near(
        &self,
        tenant_id: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Courier>> {
        // PostGIS ST_DWithin against field_ops.courier_latest_locations, which
        // is what the GiST index in migration 0003 serves. This mirrors
        // dispatch's driver_avail_repo query against driver_latest_locations,
        // deliberately: convergence should be a repository swap, and two
        // different notions of "nearest available field worker" would make it
        // a reconciliation instead.
        //
        // Not Haversine over couriers.last_lat/last_lng — those are a render
        // cache. Arithmetic on them cannot use an index at all, so every supply
        // lookup would scan the courier table.
        //
        // INNER JOIN, not LEFT: a courier with no fix has no position to search
        // on. driver_ops LEFT JOINs and sorts no-fix drivers last, which is
        // right for "show me the fleet" and wrong for "who can take this job".
        let rows = sqlx::query(
            r#"
            SELECT c.*,
                   ST_Distance(
                       geography(ST_SetSRID(ST_MakePoint(cl.lng, cl.lat), 4326)),
                       ST_SetSRID(ST_MakePoint($4, $3), 4326)::geography
                   ) AS distance_m
              FROM field_ops.couriers c
              JOIN field_ops.courier_latest_locations cl
                ON cl.courier_id = c.id
               AND cl.recorded_at > NOW() - INTERVAL '10 minutes'
             WHERE c.tenant_id = $1
               AND c.is_active
               AND c.status = 'available'
               AND ST_DWithin(
                       geography(ST_SetSRID(ST_MakePoint(cl.lng, cl.lat), 4326)),
                       ST_SetSRID(ST_MakePoint($4, $3), 4326)::geography,
                       $2 * 1000.0
                   )
             ORDER BY distance_m ASC
             LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(radius_km)
        .bind(lat)
        .bind(lng)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.iter().map(map_row).collect()
    }
}
```

- [ ] **Step 2: Wire the modules**

```rust
// services/field-ops/src/infrastructure/mod.rs
pub mod db;
```

```rust
// services/field-ops/src/infrastructure/db/mod.rs
pub mod courier_repo;
pub use courier_repo::PgCourierRepository;
```

- [ ] **Step 3: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`
Expected: FAIL — `file not found for module 'api'`, `'application'`, `'bootstrap'` only. The domain and infrastructure layers compile.

- [ ] **Step 4: Commit**

```bash
git add services/field-ops/src/infrastructure/
git commit -m "feat(field-ops): Postgres courier repository with tenant-scoped queries"
```

---

## Task 6: Assignment with atomic claim

This is the ADR's load-bearing invariant. Two products racing for the same courier must produce one winner and one explicit loser.

**Files:**
- Create: `services/field-ops/migrations/0002_create_assignments.sql`, `src/domain/entities/assignment.rs`, `src/infrastructure/db/assignment_repo.rs`

- [ ] **Step 1: Write the migration**

```sql
-- Courier assignments. The claim is cross-product: LogisticOS and OmniDeliv
-- both dispatch from the same courier pool, so "one active claim per courier"
-- must be enforced by the database, not by application convention.

-- The consumer registry. Adding a product is a data change, not a schema
-- change. `completion_topic` is why this is a table rather than a free-text
-- column: field-ops has to route a completion event somewhere, and a bare
-- string gives a label with no destination. The FK also forecloses the typo
-- failure a free-text column invites, where 'omnideliv ' and 'omnideliv'
-- silently become two products that no query joins.
CREATE TABLE IF NOT EXISTS field_ops.products (
    key              TEXT        PRIMARY KEY,
    display_name     TEXT        NOT NULL,
    completion_topic TEXT        NOT NULL,
    is_active        BOOLEAN     NOT NULL DEFAULT true,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO field_ops.products (key, display_name, completion_topic) VALUES
    ('logistics', 'LogisticOS',  'logistics.assignment.completed'),
    ('omnideliv', 'OmniDeliv AI', 'omnideliv.assignment.completed')
ON CONFLICT (key) DO NOTHING;

CREATE TABLE IF NOT EXISTS field_ops.courier_assignments (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    courier_id      UUID        NOT NULL REFERENCES field_ops.couriers(id),
    -- Which product owns this assignment. field-ops does not interpret it
    -- beyond routing completion events home.
    --
    -- FK to a registry, NOT a CHECK enumeration: a platform tier that needs a
    -- migration to admit its third consumer is not a platform tier. Onboarding
    -- a product is an INSERT into field_ops.products.
    product         TEXT        NOT NULL REFERENCES field_ops.products(key),
    -- The product's own job id (shipment_id, order_id). field-ops does not
    -- interpret it — storing a typed FK here would couple the tier to a product.
    external_ref    UUID        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'offered'
                                CHECK (status IN ('offered','claimed','completed','released','expired')),
    offered_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    claimed_at      TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    -- Claim heartbeat. A claim older than the TTL is reclaimable, so a crashed
    -- client cannot hold a courier hostage.
    heartbeat_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- THE INVARIANT: at most one live claim per courier, enforced by the database.
-- A partial unique index is the cheapest correct expression of this — it costs
-- nothing on non-claimed rows and makes a double-claim a constraint violation
-- rather than a race the application has to notice.
CREATE UNIQUE INDEX IF NOT EXISTS uq_courier_single_live_claim
    ON field_ops.courier_assignments (courier_id)
    WHERE status = 'claimed';

CREATE INDEX IF NOT EXISTS idx_assignment_tenant_status
    ON field_ops.courier_assignments (tenant_id, status);

CREATE INDEX IF NOT EXISTS idx_assignment_external_ref
    ON field_ops.courier_assignments (product, external_ref);

-- Reclaim sweep support: find claims whose heartbeat has gone stale.
CREATE INDEX IF NOT EXISTS idx_assignment_stale_claims
    ON field_ops.courier_assignments (heartbeat_at)
    WHERE status = 'claimed';
```

- [ ] **Step 2: Write the failing claim test**

```rust
// services/field-ops/src/domain/entities/assignment.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn offered() -> CourierAssignment {
        CourierAssignment::offer(Uuid::new_v4(), Uuid::new_v4(), ProductKey::new("omnideliv"), Uuid::new_v4())
    }

    #[test]
    fn a_new_assignment_is_offered_not_claimed() {
        let a = offered();
        assert_eq!(a.status, AssignmentStatus::Offered);
        assert!(a.claimed_at.is_none());
        assert!(!a.holds_courier());
    }

    #[test]
    fn claiming_marks_it_claimed_and_starts_the_heartbeat() {
        let mut a = offered();
        a.claim();
        assert_eq!(a.status, AssignmentStatus::Claimed);
        assert!(a.claimed_at.is_some());
        assert!(a.heartbeat_at.is_some(), "a claim must start its heartbeat or it is immediately stale");
        assert!(a.holds_courier());
    }

    /// Only a claimed assignment ties up a courier. Completed, released and
    /// expired ones must not — otherwise a courier is never reusable.
    #[test]
    fn only_a_claimed_assignment_holds_the_courier() {
        for terminal in [AssignmentStatus::Completed, AssignmentStatus::Released, AssignmentStatus::Expired] {
            let mut a = offered();
            a.claim();
            a.status = terminal;
            assert!(!a.holds_courier(), "{terminal:?} must not hold the courier");
        }
    }

    #[test]
    fn a_claim_is_stale_once_the_heartbeat_window_passes() {
        let mut a = offered();
        a.claim();
        assert!(!a.is_stale(120), "a fresh claim is not stale");

        a.heartbeat_at = Some(chrono::Utc::now() - chrono::Duration::seconds(300));
        assert!(a.is_stale(120), "a claim with a 5-minute-old heartbeat is stale at a 120s TTL");
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops assignment::`
Expected: FAIL to compile — `cannot find type 'CourierAssignment' in this scope`.

- [ ] **Step 4: Write the entity**

```rust
//! Courier assignment and its claim lifecycle.
//!
//! `product` and `external_ref` are deliberately opaque: field-ops must not
//! interpret a product's job id, or the tier stops being product-agnostic.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which product owns an assignment.
///
/// An opaque key, not an enum. A Rust enum here would be the same closed set
/// the rejected `CHECK (product IN (...))` was — admitting a third consumer
/// would mean editing this tier's source and redeploying it, which is exactly
/// the property ADR-0015 says disqualifies something from being a platform
/// tier. The registry table `field_ops.products` is the authority; the FK on
/// `courier_assignments.product` is what rejects an unknown key, at the same
/// moment a CHECK would have, without naming the consumers in code.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProductKey(String);

impl ProductKey {
    pub fn new(key: impl Into<String>) -> Self { Self(key.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

impl std::fmt::Display for ProductKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Offered,
    Claimed,
    Completed,
    Released,
    Expired,
}

impl AssignmentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AssignmentStatus::Offered   => "offered",
            AssignmentStatus::Claimed   => "claimed",
            AssignmentStatus::Completed => "completed",
            AssignmentStatus::Released  => "released",
            AssignmentStatus::Expired   => "expired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierAssignment {
    pub id:           Uuid,
    pub tenant_id:    Uuid,
    pub courier_id:   Uuid,
    pub product:      ProductKey,
    pub external_ref: Uuid,
    pub status:       AssignmentStatus,
    pub offered_at:   DateTime<Utc>,
    pub claimed_at:   Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub created_at:   DateTime<Utc>,
}

impl CourierAssignment {
    pub fn offer(tenant_id: Uuid, courier_id: Uuid, product: ProductKey, external_ref: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            product,
            external_ref,
            status: AssignmentStatus::Offered,
            offered_at: now,
            claimed_at: None,
            completed_at: None,
            heartbeat_at: None,
            created_at: now,
        }
    }

    pub fn claim(&mut self) {
        let now = Utc::now();
        self.status = AssignmentStatus::Claimed;
        self.claimed_at = Some(now);
        self.heartbeat_at = Some(now);
    }

    pub fn complete(&mut self) {
        self.status = AssignmentStatus::Completed;
        self.completed_at = Some(Utc::now());
    }

    pub fn release(&mut self) {
        self.status = AssignmentStatus::Released;
    }

    pub fn heartbeat(&mut self) {
        self.heartbeat_at = Some(Utc::now());
    }

    /// Does this assignment currently tie up its courier? Only a live claim does.
    pub fn holds_courier(&self) -> bool {
        self.status == AssignmentStatus::Claimed
    }

    /// A claimed assignment whose heartbeat has gone quiet past the TTL. Such a
    /// claim is reclaimable — a crashed client must not hold a courier forever.
    pub fn is_stale(&self, ttl_secs: i64) -> bool {
        if self.status != AssignmentStatus::Claimed {
            return false;
        }
        match self.heartbeat_at {
            None => true,
            Some(hb) => Utc::now() - hb > Duration::seconds(ttl_secs),
        }
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod assignment;
pub use assignment::{AssignmentStatus, CourierAssignment, ProductKey};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops assignment::`
Expected: PASS — 4 passed.

- [ ] **Step 6: Write the CAS claim repository**

```rust
// services/field-ops/src/infrastructure/db/assignment_repo.rs
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entities::{AssignmentStatus, CourierAssignment};

/// Outcome of a claim attempt. `Lost` is an ordinary outcome, not an error —
/// two products racing is expected, and the loser needs to try another courier.
#[derive(Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    Won,
    Lost,
}

#[async_trait]
pub trait AssignmentRepository: Send + Sync {
    async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()>;

    /// Atomically claim an offered assignment. Returns `Lost` when another
    /// assignment already holds this courier.
    async fn try_claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<ClaimOutcome>;
}

pub struct PgAssignmentRepository {
    pool: PgPool,
}

impl PgAssignmentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl AssignmentRepository for PgAssignmentRepository {
    async fn save(&self, a: &CourierAssignment) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.courier_assignments (
                id, tenant_id, courier_id, product, external_ref, status,
                offered_at, claimed_at, completed_at, heartbeat_at, created_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
            ON CONFLICT (id) DO UPDATE SET
                status       = EXCLUDED.status,
                claimed_at   = EXCLUDED.claimed_at,
                completed_at = EXCLUDED.completed_at,
                heartbeat_at = EXCLUDED.heartbeat_at
            "#,
        )
        .bind(a.id).bind(a.tenant_id).bind(a.courier_id)
        .bind(a.product.as_str()).bind(a.external_ref)
        .bind(a.status.as_str())
        .bind(a.offered_at).bind(a.claimed_at).bind(a.completed_at)
        .bind(a.heartbeat_at).bind(a.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn try_claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<ClaimOutcome> {
        // Compare-and-swap: the UPDATE only fires while the row is still
        // `offered`, and the partial unique index on (courier_id) WHERE
        // status='claimed' rejects it if another assignment already holds this
        // courier. Two racing products therefore produce exactly one winner —
        // one gets a row back, the other gets either zero rows (lost the CAS)
        // or a unique violation (lost the index race).
        let result = sqlx::query(
            r#"
            UPDATE field_ops.courier_assignments
               SET status = 'claimed', claimed_at = NOW(), heartbeat_at = NOW()
             WHERE id = $1 AND tenant_id = $2 AND status = 'offered'
            RETURNING id
            "#,
        )
        .bind(assignment_id)
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await;

        match result {
            Ok(Some(_)) => Ok(ClaimOutcome::Won),
            Ok(None) => Ok(ClaimOutcome::Lost),
            // The unique index fired: another assignment claimed this courier
            // between our status check and our write. That is a lost race, not
            // a failure — surface it as such rather than a 500.
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Ok(ClaimOutcome::Lost),
            Err(e) => Err(e.into()),
        }
    }
}
```

Add to `src/infrastructure/db/mod.rs`:

```rust
pub mod assignment_repo;
pub use assignment_repo::{AssignmentRepository, ClaimOutcome, PgAssignmentRepository};
```

- [ ] **Step 7: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`
Expected: FAIL on the missing `api`/`application`/`bootstrap` modules only.

- [ ] **Step 8: Commit**

```bash
git add services/field-ops/migrations/0002_create_assignments.sql services/field-ops/src/
git commit -m "feat(field-ops): courier assignment with database-enforced single live claim

The partial unique index on (courier_id) WHERE status='claimed' makes a
double-claim a constraint violation rather than a race the application has to
notice. try_claim returns Lost rather than erroring — two products racing for
the same courier is expected traffic, not a fault."
```

---

## Task 7: GPS ingest

**Files:**
- Create: `services/field-ops/migrations/0003_create_locations.sql`, `src/domain/entities/location.rs`, `src/infrastructure/db/location_repo.rs`

- [ ] **Step 1: Write the migration**

```sql
-- GPS breadcrumbs. High write volume, read almost exclusively as "latest per
-- courier" plus occasional history windows for an audit or a dispute.
--
-- device_timestamp vs recorded_at: per the CLAUDE.md dual-timestamp contract,
-- device_timestamp is the hardware clock at the physical moment of capture and
-- is the primary basis for any SLA or transit-velocity maths. recorded_at is
-- backend receipt time and is only a fallback for server-generated points.

CREATE TABLE IF NOT EXISTS field_ops.courier_locations (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    courier_id       UUID        NOT NULL,
    lat              DOUBLE PRECISION NOT NULL,
    lng              DOUBLE PRECISION NOT NULL,
    accuracy_m       REAL,
    speed_kph        REAL,
    heading_deg      REAL,
    device_timestamp TIMESTAMPTZ,
    recorded_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, recorded_at)
);

-- The dominant read: most recent fix for one courier.
CREATE INDEX IF NOT EXISTS idx_courier_location_latest
    ON field_ops.courier_locations (courier_id, recorded_at DESC);

CREATE INDEX IF NOT EXISTS idx_courier_location_tenant
    ON field_ops.courier_locations (tenant_id, recorded_at DESC);

-- THE proximity index (ADR-0015). Supply lookup is ST_DWithin against a
-- geography; neither btree above can serve it, and without this every "who is
-- near this pickup" scans the table. driver_ops has the same index on
-- driver_locations — carrying it forward is what keeps convergence a
-- repository swap.
CREATE INDEX IF NOT EXISTS idx_courier_location_spatial
    ON field_ops.courier_locations
    USING GIST (geography(ST_SetSRID(ST_MakePoint(lng, lat), 4326)));

-- One definition of "where is this courier now". Dispatch reads the view, never
-- the raw table, so the latest-fix rule lives in exactly one place. driver_ops
-- arrived here the hard way: this view replaced an ad-hoc subquery.
CREATE OR REPLACE VIEW field_ops.courier_latest_locations AS
SELECT DISTINCT ON (courier_id)
    courier_id,
    tenant_id,
    lat,
    lng,
    speed_kph,
    heading_deg,
    accuracy_m,
    device_timestamp,
    recorded_at
FROM field_ops.courier_locations
ORDER BY courier_id, recorded_at DESC;

-- Hypertable conversion, guarded. The EXCEPTION handler is the whole point: on
-- a database without TimescaleDB this degrades to a plain table instead of
-- failing the migration. That matters more here than usual — a migration that
-- cannot apply pins the service to its last-good image, silently, which is how
-- engagement sat seven weeks behind master.
DO $$ BEGIN
    PERFORM create_hypertable(
        'field_ops.courier_locations',
        'recorded_at',
        chunk_time_interval => INTERVAL '1 day',
        if_not_exists => TRUE
    );
EXCEPTION WHEN undefined_function THEN
    NULL;
END $$;

DO $$ BEGIN
    PERFORM add_compression_policy('field_ops.courier_locations', INTERVAL '7 days', if_not_exists => TRUE);
EXCEPTION WHEN undefined_function THEN NULL;
END $$;

DO $$ BEGIN
    PERFORM add_retention_policy('field_ops.courier_locations', INTERVAL '90 days', if_not_exists => TRUE);
EXCEPTION WHEN undefined_function THEN NULL;
END $$;
```

> **On the composite primary key.** `(id, recorded_at)` is TimescaleDB's requirement — a hypertable's partitioning column must appear in every unique constraint. It is already correct here, so the guarded conversion above is additive rather than a rewrite.
>
> **PostGIS is a hard requirement, unlike TimescaleDB.** The GiST index cannot be guarded away: without it the service still starts but every supply lookup degrades to a sequential scan, which fails quietly under load rather than loudly at deploy. `postgis` is already required by `driver_ops` and `dispatch`, so this adds no new dependency — but Task 10 should assert the extension is present rather than assume it.

- [ ] **Step 2: Write the entity**

```rust
// services/field-ops/src/domain/entities/location.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CourierLocation {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub courier_id:       Uuid,
    pub lat:              f64,
    pub lng:              f64,
    pub accuracy_m:       Option<f32>,
    pub speed_kph:        Option<f32>,
    pub heading_deg:      Option<f32>,
    /// Hardware clock at the physical moment of capture. Primary time basis for
    /// SLA maths; `None` only for server-generated points.
    pub device_timestamp: Option<DateTime<Utc>>,
    pub recorded_at:      DateTime<Utc>,
}

impl CourierLocation {
    pub fn new(
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            courier_id,
            lat,
            lng,
            accuracy_m: None,
            speed_kph: None,
            heading_deg: None,
            device_timestamp,
            recorded_at: Utc::now(),
        }
    }

    /// The timestamp SLA and velocity calculations should use: the device clock
    /// where we have it, backend receipt time only as a fallback.
    pub fn sla_timestamp(&self) -> DateTime<Utc> {
        self.device_timestamp.unwrap_or(self.recorded_at)
    }
}
```

- [ ] **Step 3: Write the failing test**

Append to `location.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sla_timestamp_prefers_the_device_clock() {
        let device = Utc::now() - chrono::Duration::seconds(45);
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, Some(device));
        assert_eq!(l.sla_timestamp(), device, "device_timestamp must win when present");
        assert_ne!(l.sla_timestamp(), l.recorded_at);
    }

    #[test]
    fn sla_timestamp_falls_back_to_receipt_time_for_server_points() {
        let l = CourierLocation::new(Uuid::new_v4(), Uuid::new_v4(), 14.6, 120.98, None);
        assert_eq!(l.sla_timestamp(), l.recorded_at);
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod location;
pub use location::CourierLocation;
```

- [ ] **Step 4: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops location::`
Expected: PASS — 2 passed.

- [ ] **Step 5: Write the location repository**

```rust
// services/field-ops/src/infrastructure/db/location_repo.rs
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::CourierLocation;

#[async_trait]
pub trait LocationRepository: Send + Sync {
    async fn record(&self, l: &CourierLocation) -> anyhow::Result<()>;
    async fn latest(&self, tenant_id: Uuid, courier_id: Uuid) -> anyhow::Result<Option<CourierLocation>>;
}

pub struct PgLocationRepository {
    pool: PgPool,
}

impl PgLocationRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_row(r: &sqlx::postgres::PgRow) -> CourierLocation {
    CourierLocation {
        id:               r.get("id"),
        tenant_id:        r.get("tenant_id"),
        courier_id:       r.get("courier_id"),
        lat:              r.get("lat"),
        lng:              r.get("lng"),
        accuracy_m:       r.get("accuracy_m"),
        speed_kph:        r.get("speed_kph"),
        heading_deg:      r.get("heading_deg"),
        device_timestamp: r.get("device_timestamp"),
        recorded_at:      r.get("recorded_at"),
    }
}

#[async_trait]
impl LocationRepository for PgLocationRepository {
    async fn record(&self, l: &CourierLocation) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO field_ops.courier_locations (
                id, tenant_id, courier_id, lat, lng, accuracy_m, speed_kph,
                heading_deg, device_timestamp, recorded_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
            "#,
        )
        .bind(l.id).bind(l.tenant_id).bind(l.courier_id)
        .bind(l.lat).bind(l.lng)
        .bind(l.accuracy_m).bind(l.speed_kph).bind(l.heading_deg)
        .bind(l.device_timestamp).bind(l.recorded_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn latest(&self, tenant_id: Uuid, courier_id: Uuid) -> anyhow::Result<Option<CourierLocation>> {
        let row = sqlx::query(
            r#"
            SELECT * FROM field_ops.courier_locations
             WHERE tenant_id = $1 AND courier_id = $2
             ORDER BY recorded_at DESC
             LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(courier_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.as_ref().map(map_row))
    }
}
```

Add to `src/infrastructure/db/mod.rs`:

```rust
pub mod location_repo;
pub use location_repo::{LocationRepository, PgLocationRepository};
```

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/migrations/0003_create_locations.sql services/field-ops/src/
git commit -m "feat(field-ops): GPS breadcrumb ingest with dual-timestamp SLA basis"
```

---

## Task 8: Application services

**Files:**
- Create: `services/field-ops/src/application/mod.rs`, `src/application/services/mod.rs`, `src/application/services/dispatch_service.rs`

- [ ] **Step 1: Write the service**

```rust
// services/field-ops/src/application/services/dispatch_service.rs
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{CourierAssignment, CourierLocation, ProductKey};
use crate::domain::repositories::CourierRepository;
use crate::infrastructure::db::{AssignmentRepository, ClaimOutcome, LocationRepository};

pub struct DispatchService {
    couriers:    Arc<dyn CourierRepository>,
    assignments: Arc<dyn AssignmentRepository>,
    locations:   Arc<dyn LocationRepository>,
}

impl DispatchService {
    pub fn new(
        couriers: Arc<dyn CourierRepository>,
        assignments: Arc<dyn AssignmentRepository>,
        locations: Arc<dyn LocationRepository>,
    ) -> Self {
        Self { couriers, assignments, locations }
    }

    /// Offer a job to the nearest dispatchable couriers. Offering is not
    /// claiming — several couriers may hold an offer for the same job; exactly
    /// one will win the claim.
    pub async fn offer_to_nearest(
        &self,
        tenant_id: Uuid,
        product: ProductKey,
        external_ref: Uuid,
        lat: f64,
        lng: f64,
        radius_km: f64,
        fanout: i64,
    ) -> anyhow::Result<Vec<CourierAssignment>> {
        let candidates = self
            .couriers
            .find_available_near(tenant_id, lat, lng, radius_km, fanout)
            .await?;

        let mut offers = Vec::with_capacity(candidates.len());
        for c in candidates {
            let a = CourierAssignment::offer(tenant_id, c.id, product.clone(), external_ref);
            self.assignments.save(&a).await?;
            offers.push(a);
        }
        Ok(offers)
    }

    /// A courier accepts an offer. Returns `false` when another courier got
    /// there first — the caller should show "already taken", not an error.
    pub async fn claim(&self, tenant_id: Uuid, assignment_id: Uuid) -> anyhow::Result<bool> {
        match self.assignments.try_claim(tenant_id, assignment_id).await? {
            ClaimOutcome::Won  => Ok(true),
            ClaimOutcome::Lost => Ok(false),
        }
    }

    pub async fn record_position(
        &self,
        tenant_id: Uuid,
        courier_id: Uuid,
        lat: f64,
        lng: f64,
        device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> anyhow::Result<()> {
        let fix = CourierLocation::new(tenant_id, courier_id, lat, lng, device_timestamp);
        self.locations.record(&fix).await?;

        // Refresh the render cache on the courier row. This is NOT what supply
        // lookup reads — find_available_near joins courier_latest_locations,
        // because only the GiST index there can serve ST_DWithin.
        if let Some(mut c) = self.couriers.find_by_id(tenant_id, courier_id).await? {
            c.record_position(lat, lng);
            self.couriers.save(&c).await?;
        }
        Ok(())
    }
}
```

```rust
// services/field-ops/src/application/mod.rs
pub mod services;
```

```rust
// services/field-ops/src/application/services/mod.rs
pub mod dispatch_service;
pub use dispatch_service::DispatchService;
```

- [ ] **Step 2: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`
Expected: FAIL on missing `api` and `bootstrap` only.

- [ ] **Step 3: Commit**

```bash
git add services/field-ops/src/application/
git commit -m "feat(field-ops): dispatch service — offer fan-out and single-winner claim"
```

---

## Task 9: HTTP API and bootstrap

**Files:**
- Create: `src/api/mod.rs`, `src/api/http/mod.rs`, `src/api/http/health.rs`, `src/api/http/couriers.rs`, `src/bootstrap.rs`

- [ ] **Step 1: Write the health endpoints**

Every service exposes `/health`, `/ready`, `/metrics` per the engineering principles. **These must not sit behind auth** — an open incident in this repo has 8 services showing red for 11 days because `/health` returns 401 and the healthcheck's `curl -sf` fails.

```rust
// services/field-ops/src/api/http/health.rs
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "field-ops" }))
}

async fn ready() -> Json<Value> {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> String {
    // Prometheus text exposition. Expand as counters are added.
    "# HELP field_ops_up Service liveness\n# TYPE field_ops_up gauge\nfield_ops_up 1\n".to_string()
}
```

- [ ] **Step 2: Write the courier routes**

```rust
// services/field-ops/src/api/http/couriers.rs
use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;
use crate::domain::entities::ProductKey;

#[derive(Debug, Deserialize)]
pub struct OfferRequest {
    pub product:      ProductKey,
    pub external_ref: Uuid,
    pub lat:          f64,
    pub lng:          f64,
    #[serde(default = "default_radius_km")]
    pub radius_km:    f64,
    #[serde(default = "default_fanout")]
    pub fanout:       i64,
}

fn default_radius_km() -> f64 { 5.0 }
fn default_fanout() -> i64 { 5 }

#[derive(Debug, Serialize)]
pub struct OfferResponse {
    pub assignment_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct ClaimResponse {
    pub won: bool,
}

#[derive(Debug, Deserialize)]
pub struct PositionRequest {
    pub lat:              f64,
    pub lng:              f64,
    pub device_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

// Prefixed because this is a platform tier: `/v1/assignments` is already owned
// by dispatch and called in production by the driver app, so an unprefixed
// route resolves to dispatch at the gateway and never arrives here.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/field-ops/assignments/offer", post(offer))
        .route("/v1/field-ops/assignments/:id/claim", post(claim))
        .route("/v1/field-ops/couriers/:id/position", post(position))
}

async fn offer(
    State(st): State<Arc<AppState>>,
    Json(req): Json<OfferRequest>,
) -> Result<Json<OfferResponse>, StatusCode> {
    let offers = st
        .dispatch
        .offer_to_nearest(
            st.tenant_id, req.product, req.external_ref,
            req.lat, req.lng, req.radius_km, req.fanout,
        )
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "offer failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(OfferResponse { assignment_ids: offers.iter().map(|a| a.id).collect() }))
}

async fn claim(
    State(st): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<ClaimResponse>, StatusCode> {
    // A lost race is 200 { won: false }, not an error status. The client needs
    // to distinguish "someone else got it" from "the request failed".
    let won = st.dispatch.claim(st.tenant_id, id).await.map_err(|e| {
        tracing::error!(err = %e, "claim failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(ClaimResponse { won }))
}

async fn position(
    State(st): State<Arc<AppState>>,
    Path(courier_id): Path<Uuid>,
    Json(req): Json<PositionRequest>,
) -> Result<StatusCode, StatusCode> {
    st.dispatch
        .record_position(st.tenant_id, courier_id, req.lat, req.lng, req.device_timestamp)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "position ingest failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(StatusCode::ACCEPTED)
}
```

> **`AppState.tenant_id` is a placeholder.** It must come from the validated JWT via `require_auth`, not from app state. Task 10 wires the real middleware; this keeps the routes compiling in isolation.

- [ ] **Step 3: Write the router and state**

```rust
// services/field-ops/src/api/http/mod.rs
pub mod couriers;
pub mod health;

use std::sync::Arc;
use axum::Router;
use uuid::Uuid;

use crate::application::services::DispatchService;

pub struct AppState {
    pub dispatch: Arc<DispatchService>,
    /// TODO(Task 10): replaced by per-request tenant from JWT claims.
    pub tenant_id: Uuid,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health::routes())
        .merge(couriers::routes().with_state(state))
}
```

```rust
// services/field-ops/src/api/mod.rs
pub mod http;
```

- [ ] **Step 4: Write the bootstrap**

```rust
// services/field-ops/src/bootstrap.rs
use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::services::DispatchService;
use crate::config::Config;
use crate::infrastructure::db::{PgAssignmentRepository, PgCourierRepository, PgLocationRepository};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load field-ops config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "field-ops",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "field-ops service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO field_ops, public")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .context("field-ops migration failed")?;

    let dispatch = Arc::new(DispatchService::new(
        Arc::new(PgCourierRepository::new(pool.clone())),
        Arc::new(PgAssignmentRepository::new(pool.clone())),
        Arc::new(PgLocationRepository::new(pool.clone())),
    ));

    let state = Arc::new(AppState { dispatch, tenant_id: uuid::Uuid::nil() });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "field-ops listening");
    axum::serve(listener, router(state)).await?;

    Ok(())
}
```

- [ ] **Step 5: Verify the whole crate compiles**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-field-ops`
Expected: PASS, no errors.

- [ ] **Step 6: Run the full test suite**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops`
Expected: PASS — 10 tests (4 courier, 4 assignment, 2 location).

- [ ] **Step 7: Commit**

```bash
git add services/field-ops/src/
git commit -m "feat(field-ops): HTTP API, unauthenticated health probes, bootstrap"
```

---

## Task 10: Auth, deployment wiring, and the claim race test

**Files:**
- Modify: `src/api/http/mod.rs`, `src/api/http/couriers.rs`, `src/bootstrap.rs`
- Create: `services/field-ops/tests/claim_race.rs`
- Modify: `.github/workflows/build-images.yml`

- [ ] **Step 1: Replace the placeholder tenant with JWT claims**

Follow the pattern in `services/pod/src/api/middleware/mod.rs`. Delete `AppState.tenant_id`; extract `Claims` from request extensions in each handler and read `claims.tenant_id`. Apply `require_auth` to the courier routes only — the health router must stay open:

```rust
pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health::routes())                       // open — healthchecks curl this
        .merge(
            couriers::routes()
                .with_state(state)
                .layer(axum::middleware::from_fn(logisticos_auth::middleware::require_auth)),
        )
}
```

- [ ] **Step 2: Write the claim race integration test**

```rust
// services/field-ops/tests/claim_race.rs
//! Proves ADR-0015's load-bearing invariant against a real database:
//! two products racing for the same courier produce exactly one winner.
//!
//! Requires a running Postgres. Skipped when DATABASE_URL is unset.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_field_ops::domain::entities::{Courier, CourierAssignment, ProductKey};
use logisticos_field_ops::domain::repositories::CourierRepository;
use logisticos_field_ops::infrastructure::db::{
    AssignmentRepository, ClaimOutcome, PgAssignmentRepository, PgCourierRepository,
};

#[tokio::test]
async fn two_products_racing_for_one_courier_produce_exactly_one_winner() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO field_ops, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url)
        .await
        .expect("connect");

    logisticos_common::migrations::run(&pool, "field_ops", &sqlx::migrate!("./migrations"))
        .await
        .expect("migrate");

    let tenant = Uuid::new_v4();
    let couriers = PgCourierRepository::new(pool.clone());
    let assignments = PgAssignmentRepository::new(pool.clone());

    let mut courier = Courier::new(
        tenant, Uuid::new_v4(), "Race".into(), "Test".into(), "+639170000001".into(),
    );
    courier.go_available();
    couriers.save(&courier).await.expect("save courier");

    // Both products offer the same courier a job.
    let a_logistics = CourierAssignment::offer(tenant, courier.id, ProductKey::new("logistics"), Uuid::new_v4());
    let a_omnideliv = CourierAssignment::offer(tenant, courier.id, ProductKey::new("omnideliv"), Uuid::new_v4());
    assignments.save(&a_logistics).await.expect("save A");
    assignments.save(&a_omnideliv).await.expect("save B");

    // Race the claims concurrently.
    let (r1, r2) = tokio::join!(
        assignments.try_claim(tenant, a_logistics.id),
        assignments.try_claim(tenant, a_omnideliv.id),
    );

    let wins = [r1.expect("claim A"), r2.expect("claim B")]
        .iter()
        .filter(|o| **o == ClaimOutcome::Won)
        .count();

    assert_eq!(wins, 1, "exactly one product must win the courier, got {wins}");
}
```

- [ ] **Step 3: Run the race test**

```bash
DATABASE_URL="postgres://logisticos:logisticos@localhost:5432/svc_field_ops" CARGO_INCREMENTAL=0 cargo test -p logisticos-field-ops --test claim_race
```

Expected: PASS — `exactly one product must win the courier`. If it reports 2 wins, the partial unique index from Task 6 did not apply; check `\d field_ops.courier_assignments` for `uq_courier_single_live_claim`.

- [ ] **Step 4: Add the service to the image build**

In `.github/workflows/build-images.yml`, add `field-ops` to the service matrix alongside the existing 20 entries.

- [ ] **Step 5: Verify the workspace still checks**

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add services/field-ops/ .github/workflows/build-images.yml
git commit -m "feat(field-ops): JWT auth on operational routes, claim race test, image build

The claim race test exercises ADR-0015's load-bearing invariant against a real
database: two products claiming the same courier concurrently must produce
exactly one winner. Health probes stay unauthenticated so container
healthchecks can reach them."
```

---

## Definition of done

- [ ] ADR-0015 reviewed and status moved to Accepted
- [ ] `cargo test -p logisticos-field-ops` — 10 unit tests pass
- [ ] `cargo test -p logisticos-field-ops --test claim_race` with `DATABASE_URL` set — passes
- [ ] `cargo check --workspace` — clean
- [ ] `curl -sf localhost:8090/health` returns 200 without a token
- [ ] `rg -n "ENABLE ROW LEVEL SECURITY" services/field-ops/migrations/` returns nothing

## Follow-on work this surfaces

1. **Plan 5** — the earnings ledger, built against the real three-leg settlement model
2. **LogisticOS migration onto field-ops** — its own plan; ADR-0015 carries the dated commitment
3. **Platform-wide RLS decision, needs its own ADR.** 52 migrations enable a row-level-security policy keyed on `current_setting('app.tenant_id')` that no service ever sets; services connect as the schema owner so PostgreSQL bypasses it entirely. Every `FORCE` attempt was reverted. The platform's actual isolation is application-layer, which contradicts ADR-0003 and ADR-0008 as written. Either make RLS enforce (per-request `SET LOCAL`, with connection-pool implications) or amend those ADRs to describe what the system really does — the current state documents a control that is not in force.
