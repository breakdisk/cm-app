# OmniDeliv Catalog & Basket Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up `services/omnideliv` with the vendor catalog and the multi-vendor basket, so the agent mesh (Plan 4) has real data to reason over and a basket to write into.

**Architecture:** A new Rust/Axum service on the standard hexagonal layout, schema `omnideliv`, migrating via `logisticos_common::migrations::run` per ADR-0012. Modular monolith per the spec's decomposition decision — catalog, basket, mesh, consolidation and orders are separate workspace-internal modules with the split seam at `mesh`. This plan builds the first two. Two design decisions carry the weight: availability is a *freshness-stamped* state rather than a boolean, and the basket has exactly one writer.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLx, PostgreSQL + PostGIS, Kafka (`logisticos-events`), JWT auth (`logisticos-auth`).

---

## Scope

**In:** service skeleton, vendors, catalog items, availability with freshness, baskets, sub-intents, basket lines, the `BasketDelta` application path, HTTP API.

**Out:** the agent mesh (Plan 4), consolidation and orders (Plan 5), the vendor console UI (Plan 6). This plan produces a service the mesh can be built on top of — it does not call Claude and has no agent code.

**Dependencies:** none. This plan can run in parallel with Plans 1 and 2.

---

## Prerequisites

Read before starting:

- [docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md](../specs/2026-08-06-omnideliv-ai-design.md) §5 (domain model) and §D12 (the Vendor naming decision)
- [services/pod/src/bootstrap.rs](../../../services/pod/src/bootstrap.rs) — the bootstrap pattern to copy
- [services/pod/migrations/0001_create_pod_tables.sql](../../../services/pod/migrations/0001_create_pod_tables.sql) — migration house style

**Disk:** clear `C:\cargo-target-logisticos\debug\incremental` and export `CARGO_INCREMENTAL=0` before starting.

### Two naming rules that are load-bearing

**`Vendor` in code, "merchant" in UI copy.** LogisticOS already has a `Merchant`: a business that *pays the Partner* to ship parcels. An OmniDeliv restaurant *receives money from the Partner*. Opposite direction, different lifecycle, different settlement. ADR-0009 is explicit that conflating two actors "breaks multi-tenancy, billing, or RLS". Nothing in this service's Rust or SQL says `merchant`; user-facing strings still say "merchant" or "store" because that is what people call it.

**Tenancy is application-layer.** Do not add an RLS policy. See the extended note in [the field-ops plan](2026-08-06-field-ops-extraction.md) — 52 migrations in this repo enable a policy on `current_setting('app.tenant_id')` that no service ever sets, and services connect as the schema owner so PostgreSQL bypasses it. Every repository method here takes `tenant_id` explicitly; that signature is the enforcement point.

---

## File Structure

**New — `services/omnideliv/`:**

| File | Responsibility |
|---|---|
| `Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/bootstrap.rs`, `src/config.rs` | Wiring |
| `migrations/0001_create_vendors.sql` | Schema + `vendors` |
| `migrations/0002_create_catalog.sql` | `catalog_items` + `item_availability` |
| `migrations/0003_create_baskets.sql` | `baskets` + `sub_intents` + `basket_lines` |
| `src/domain/entities/vendor.rs` | `Vendor`, `Vertical` |
| `src/domain/entities/catalog.rs` | `CatalogItem`, `Availability`, `AvailabilityState` |
| `src/domain/entities/basket.rs` | `Basket`, `SubIntent`, `BasketLine`, `LineState`, `BasketDelta` |
| `src/domain/repositories/mod.rs` | Repository traits |
| `src/infrastructure/db/{vendor,catalog,basket}_repo.rs` | Postgres repositories |
| `src/application/services/{catalog,basket}_service.rs` | Use cases |
| `src/api/http/{mod,health,vendors,catalog,baskets}.rs` | Routes |

**Modified:** root `Cargo.toml`, `.github/workflows/build-images.yml`.

---

## Task 1: Scaffold the service

**Files:**
- Create: `services/omnideliv/Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/config.rs`
- Modify: root `Cargo.toml`

- [ ] **Step 1: Write the manifest**

```toml
# services/omnideliv/Cargo.toml
[package]
name        = "logisticos-omnideliv"
description = "OmniDeliv product tier — vendor catalog, multi-vendor basket, mesh, consolidation, orders"
version.workspace      = true
edition.workspace      = true
authors.workspace      = true
rust-version.workspace = true

[[bin]]
name = "omnideliv"
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

- [ ] **Step 2: Write the entrypoint, lib root and config**

```rust
// services/omnideliv/src/main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_omnideliv::bootstrap::run().await
}
```

```rust
// services/omnideliv/src/lib.rs
#![deny(clippy::all)]

pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;
```

```rust
// services/omnideliv/src/config.rs
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

    /// How old a vendor-declared availability flag may be before the catalog
    /// stops treating it as trustworthy. Drives defensive substitution — see
    /// `Availability::confidence`.
    #[serde(default = "default_stock_freshness_mins")]
    pub stock_freshness_mins: i64,
}

fn default_stock_freshness_mins() -> i64 { 30 }

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let c = config::Config::builder()
            .set_default("app.env", "development")?
            .set_default("app.port", 8091)?
            .add_source(config::Environment::default().separator("__"))
            .build()?;
        Ok(c.try_deserialize()?)
    }
}
```

- [ ] **Step 3: Register the workspace member**

In the root `Cargo.toml`, add `"services/omnideliv",` to `members` after `"services/order-intake",`.

- [ ] **Step 4: Verify it resolves**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: FAIL — `file not found for module 'api'` plus four similar.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml services/omnideliv/
git commit -m "feat(omnideliv): scaffold product-tier service crate and config"
```

---

## Task 2: Vendors

**Files:**
- Create: `services/omnideliv/migrations/0001_create_vendors.sql`, `src/domain/mod.rs`, `src/domain/entities/mod.rs`, `src/domain/entities/vendor.rs`

- [ ] **Step 1: Write the migration**

```sql
-- OmniDeliv product tier. See docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md
CREATE SCHEMA IF NOT EXISTS omnideliv;

-- NAMING: `vendor`, never `merchant`. A LogisticOS Merchant pays the Partner to
-- ship parcels; an OmniDeliv vendor receives money from the Partner for goods.
-- Opposite money flow, different lifecycle. UI copy still says "merchant".
--
-- TENANCY: application-layer, not RLS. Every repository query filters on
-- tenant_id explicitly. See the field-ops plan for why a policy here would
-- imply a database guarantee that does not exist on this platform.

CREATE TABLE IF NOT EXISTS omnideliv.vendors (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id         UUID        NOT NULL,
    vertical          TEXT        NOT NULL
                                  CHECK (vertical IN ('restaurant','grocery','pharmacy','florist','retail')),
    name              TEXT        NOT NULL,
    address           TEXT        NOT NULL,
    lat               DOUBLE PRECISION NOT NULL,
    lng               DOUBLE PRECISION NOT NULL,
    -- Kitchen/pick time. The Fleet agent sequences stops by this, so a grocery
    -- pick (5 min) is collected before a restaurant main (20 min) and nothing
    -- sits going cold.
    prep_time_minutes INT         NOT NULL DEFAULT 15 CHECK (prep_time_minutes >= 0),
    -- Commission in basis points (250 = 2.50%). Basis points, not a float —
    -- this multiplies money.
    commission_bps    INT         NOT NULL DEFAULT 1500
                                  CHECK (commission_bps BETWEEN 0 AND 10000),
    payout_account    TEXT,
    -- Opening hours as {"mon": [["09:00","21:00"]], ...}. JSONB because the
    -- shape varies per vertical (pharmacies have split shifts, groceries don't).
    hours             JSONB       NOT NULL DEFAULT '{}',
    status            TEXT        NOT NULL DEFAULT 'onboarding'
                                  CHECK (status IN ('onboarding','active','paused','offboarded')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_vendor_tenant_vertical
    ON omnideliv.vendors (tenant_id, vertical)
    WHERE status = 'active';

-- Supply lookup: active vendors of a vertical near the customer.
CREATE INDEX IF NOT EXISTS idx_vendor_geo
    ON omnideliv.vendors (tenant_id, lat, lng)
    WHERE status = 'active';
```

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/vendor.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn vendor() -> Vendor {
        Vendor::new(Uuid::new_v4(), Vertical::Restaurant, "Kuya's Silog House".into(),
                    "123 Mabini St".into(), 14.5995, 120.9842)
    }

    #[test]
    fn a_new_vendor_starts_onboarding_and_is_not_orderable() {
        let v = vendor();
        assert_eq!(v.status, VendorStatus::Onboarding);
        assert!(!v.is_orderable());
    }

    #[test]
    fn only_an_active_vendor_is_orderable() {
        let mut v = vendor();
        v.activate();
        assert!(v.is_orderable());
        v.pause();
        assert!(!v.is_orderable());
    }

    /// Commission is the Partner's revenue leg. Basis-point maths must round
    /// down so the platform never over-charges a vendor by a rounding cent.
    #[test]
    fn commission_rounds_down_in_the_vendors_favour() {
        let mut v = vendor();
        v.commission_bps = 1500; // 15%

        assert_eq!(v.commission_on(10_000), 1_500);
        // 999 * 0.15 = 149.85 → 149, not 150
        assert_eq!(v.commission_on(999), 149);
    }

    #[test]
    fn payout_is_the_subtotal_less_commission() {
        let mut v = vendor();
        v.commission_bps = 1500;
        assert_eq!(v.payout_on(10_000), 8_500);
        assert_eq!(v.commission_on(10_000) + v.payout_on(10_000), 10_000,
                   "commission and payout must always sum to the subtotal");
    }

    #[test]
    fn zero_commission_pays_out_the_whole_subtotal() {
        let mut v = vendor();
        v.commission_bps = 0;
        assert_eq!(v.payout_on(12_345), 12_345);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv vendor::`
Expected: FAIL to compile — `cannot find type 'Vendor' in this scope`.

- [ ] **Step 4: Write the entity**

```rust
//! A business that supplies goods. Named `Vendor`, never `merchant` — see the
//! naming note in migration 0001.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vertical {
    Restaurant,
    Grocery,
    Pharmacy,
    Florist,
    Retail,
}

impl Vertical {
    pub fn as_str(&self) -> &'static str {
        match self {
            Vertical::Restaurant => "restaurant",
            Vertical::Grocery    => "grocery",
            Vertical::Pharmacy   => "pharmacy",
            Vertical::Florist    => "florist",
            Vertical::Retail     => "retail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VendorStatus {
    Onboarding,
    Active,
    Paused,
    Offboarded,
}

impl VendorStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VendorStatus::Onboarding => "onboarding",
            VendorStatus::Active     => "active",
            VendorStatus::Paused     => "paused",
            VendorStatus::Offboarded => "offboarded",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vendor {
    pub id:                Uuid,
    pub tenant_id:         Uuid,
    pub vertical:          Vertical,
    pub name:              String,
    pub address:           String,
    pub lat:               f64,
    pub lng:               f64,
    pub prep_time_minutes: i32,
    pub commission_bps:    i32,
    pub payout_account:    Option<String>,
    pub hours:             serde_json::Value,
    pub status:            VendorStatus,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

impl Vendor {
    pub fn new(
        tenant_id: Uuid,
        vertical: Vertical,
        name: String,
        address: String,
        lat: f64,
        lng: f64,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            vertical,
            name,
            address,
            lat,
            lng,
            prep_time_minutes: 15,
            commission_bps: 1500,
            payout_account: None,
            hours: serde_json::json!({}),
            status: VendorStatus::Onboarding,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn is_orderable(&self) -> bool {
        self.status == VendorStatus::Active
    }

    pub fn activate(&mut self) { self.status = VendorStatus::Active; self.updated_at = Utc::now(); }
    pub fn pause(&mut self)    { self.status = VendorStatus::Paused; self.updated_at = Utc::now(); }

    /// The Partner's commission on a goods subtotal, in cents.
    ///
    /// Integer maths throughout, truncating. Truncation rounds in the vendor's
    /// favour, which is the correct direction for a fee the platform charges.
    pub fn commission_on(&self, subtotal_cents: i64) -> i64 {
        subtotal_cents * self.commission_bps as i64 / 10_000
    }

    /// What the vendor is credited for a goods subtotal, in cents.
    /// Always exactly `subtotal - commission`, so the two can never drift.
    pub fn payout_on(&self, subtotal_cents: i64) -> i64 {
        subtotal_cents - self.commission_on(subtotal_cents)
    }
}
```

```rust
// services/omnideliv/src/domain/mod.rs
pub mod entities;
pub mod repositories;
```

```rust
// services/omnideliv/src/domain/entities/mod.rs
pub mod vendor;
pub use vendor::{Vendor, VendorStatus, Vertical};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv vendor::`
Expected: PASS — 5 passed.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0001_create_vendors.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): Vendor entity with basis-point commission maths"
```

---

## Task 3: Catalog items and availability freshness

The freshness stamp is the load-bearing decision in this plan. Because stock is vendor-declared rather than POS-synced, `updated_at` is what lets the Nutritionist reason honestly — a flag touched minutes ago is trustworthy; one from yesterday means propose a substitute defensively. Without it, the substitution loop in Screen C is guesswork.

**Files:**
- Create: `services/omnideliv/migrations/0002_create_catalog.sql`, `src/domain/entities/catalog.rs`

- [ ] **Step 1: Write the migration**

```sql
CREATE TABLE IF NOT EXISTS omnideliv.catalog_items (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL,
    vendor_id      UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    sku            TEXT        NOT NULL,
    name           TEXT        NOT NULL,
    description    TEXT,
    price_cents    BIGINT      NOT NULL CHECK (price_cents >= 0),
    -- Size/extras/options. Shape varies per vertical, so JSONB rather than a
    -- normalised modifier table we would have to reshape per vertical.
    modifiers      JSONB       NOT NULL DEFAULT '[]',
    -- Allergen and dietary tags drive the Nutritionist's filtering. Arrays, not
    -- JSONB, because they are queried with `&&` (overlap) on the hot path.
    allergens      TEXT[]      NOT NULL DEFAULT '{}',
    dietary_tags   TEXT[]      NOT NULL DEFAULT '{}',
    -- Per-vertical extras (Rx schedule, floral stem count, retail dimensions).
    vertical_attrs JSONB       NOT NULL DEFAULT '{}',
    is_listed      BOOLEAN     NOT NULL DEFAULT true,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_catalog_vendor_sku
    ON omnideliv.catalog_items (vendor_id, sku);

CREATE INDEX IF NOT EXISTS idx_catalog_vendor_listed
    ON omnideliv.catalog_items (tenant_id, vendor_id)
    WHERE is_listed;

-- Allergen exclusion is a filter on nearly every Nutritionist query.
CREATE INDEX IF NOT EXISTS idx_catalog_allergens
    ON omnideliv.catalog_items USING GIN (allergens);

-- Availability is a separate table, not a column on catalog_items, for one
-- reason: it is written far more often than the item it describes. A vendor
-- toggling stock all day must not churn the item row (and its GIN index).
--
-- updated_at is LOAD-BEARING, not bookkeeping. Stock here is vendor-declared,
-- so the age of the declaration is what tells the agent how much to trust it.
CREATE TABLE IF NOT EXISTS omnideliv.item_availability (
    item_id    UUID        PRIMARY KEY REFERENCES omnideliv.catalog_items(id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    state      TEXT        NOT NULL DEFAULT 'available'
                           CHECK (state IN ('available','limited','out_of_stock')),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by UUID
);

CREATE INDEX IF NOT EXISTS idx_availability_stale
    ON omnideliv.item_availability (tenant_id, updated_at);
```

- [ ] **Step 2: Write the failing test**

```rust
// services/omnideliv/src/domain/entities/catalog.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use uuid::Uuid;

    fn avail(state: AvailabilityState, age_mins: i64) -> Availability {
        Availability {
            item_id:    Uuid::new_v4(),
            tenant_id:  Uuid::new_v4(),
            state,
            updated_at: Utc::now() - Duration::minutes(age_mins),
            updated_by: None,
        }
    }

    const FRESH_WINDOW: i64 = 30;

    #[test]
    fn a_recently_confirmed_in_stock_item_is_trusted() {
        let a = avail(AvailabilityState::Available, 2);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
        assert!(!a.warrants_substitute(FRESH_WINDOW));
    }

    /// The whole point of the freshness stamp: a stale "in stock" flag is not
    /// a promise. The agent should line up a substitute rather than assume.
    #[test]
    fn a_stale_in_stock_flag_is_only_uncertain() {
        let a = avail(AvailabilityState::Available, 240);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Uncertain);
        assert!(a.warrants_substitute(FRESH_WINDOW),
                "a 4-hour-old in-stock flag should trigger defensive substitution");
    }

    /// Out-of-stock is believed regardless of age. Staleness can only ever make
    /// us *less* confident an item is present, never more.
    #[test]
    fn out_of_stock_is_trusted_even_when_stale() {
        let a = avail(AvailabilityState::OutOfStock, 5_000);
        assert_eq!(a.confidence(FRESH_WINDOW), Confidence::Trusted);
        assert!(a.warrants_substitute(FRESH_WINDOW),
                "out of stock always needs a substitute — that is the point");
    }

    #[test]
    fn limited_stock_always_warrants_a_backup() {
        let a = avail(AvailabilityState::Limited, 1);
        assert!(a.warrants_substitute(FRESH_WINDOW));
    }

    #[test]
    fn the_freshness_boundary_is_inclusive_of_the_window() {
        assert_eq!(avail(AvailabilityState::Available, 29).confidence(30), Confidence::Trusted);
        assert_eq!(avail(AvailabilityState::Available, 31).confidence(30), Confidence::Uncertain);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv catalog::`
Expected: FAIL to compile — `cannot find type 'Availability' in this scope`.

- [ ] **Step 4: Write the entities**

```rust
//! Catalog items and their availability.
//!
//! Availability is vendor-declared, not POS-synced. The age of a declaration is
//! therefore part of its meaning: `confidence` turns "what the vendor said" plus
//! "how long ago they said it" into "how much an agent should trust it".

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id:             Uuid,
    pub tenant_id:      Uuid,
    pub vendor_id:      Uuid,
    pub sku:            String,
    pub name:           String,
    pub description:    Option<String>,
    pub price_cents:    i64,
    pub modifiers:      serde_json::Value,
    pub allergens:      Vec<String>,
    pub dietary_tags:   Vec<String>,
    pub vertical_attrs: serde_json::Value,
    pub is_listed:      bool,
    pub created_at:     DateTime<Utc>,
    pub updated_at:     DateTime<Utc>,
}

impl CatalogItem {
    /// Does this item contain any allergen the customer must avoid?
    /// Case-insensitive — vendors type these by hand.
    pub fn conflicts_with_allergens(&self, avoid: &[String]) -> bool {
        self.allergens.iter().any(|a| {
            avoid.iter().any(|x| x.eq_ignore_ascii_case(a))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AvailabilityState {
    Available,
    Limited,
    OutOfStock,
}

impl AvailabilityState {
    pub fn as_str(&self) -> &'static str {
        match self {
            AvailabilityState::Available  => "available",
            AvailabilityState::Limited    => "limited",
            AvailabilityState::OutOfStock => "out_of_stock",
        }
    }
}

/// How much an agent should trust an availability declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Trusted,
    Uncertain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    pub item_id:    Uuid,
    pub tenant_id:  Uuid,
    pub state:      AvailabilityState,
    /// When the vendor last declared this. Load-bearing — see `confidence`.
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
}

impl Availability {
    /// Staleness only ever reduces confidence that an item is *present*.
    ///
    /// An "in stock" flag from four hours ago is a guess; the item may have sold
    /// out since. An "out of stock" flag from four hours ago is still believed —
    /// a vendor who marks something gone rarely has it back within the window,
    /// and being wrong in that direction merely offers a substitute the customer
    /// can decline. Being wrong the other way means a courier arrives to nothing.
    pub fn confidence(&self, fresh_window_mins: i64) -> Confidence {
        match self.state {
            AvailabilityState::OutOfStock | AvailabilityState::Limited => Confidence::Trusted,
            AvailabilityState::Available => {
                if Utc::now() - self.updated_at <= Duration::minutes(fresh_window_mins) {
                    Confidence::Trusted
                } else {
                    Confidence::Uncertain
                }
            }
        }
    }

    /// Should the agent line up a substitute before the courier sets off?
    ///
    /// True when the item is gone, nearly gone, or claimed present on evidence
    /// too old to rely on. This is what makes Screen C's substitution review
    /// meaningful rather than decorative.
    pub fn warrants_substitute(&self, fresh_window_mins: i64) -> bool {
        match self.state {
            AvailabilityState::OutOfStock | AvailabilityState::Limited => true,
            AvailabilityState::Available => self.confidence(fresh_window_mins) == Confidence::Uncertain,
        }
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod catalog;
pub use catalog::{Availability, AvailabilityState, CatalogItem, Confidence};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv catalog::`
Expected: PASS — 5 passed.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0002_create_catalog.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): catalog items and freshness-aware availability

Availability carries an updated_at that is load-bearing rather than
bookkeeping. Because stock is vendor-declared, the age of a declaration is
part of its meaning: a stale in-stock flag downgrades to Uncertain and
triggers defensive substitution, while out-of-stock is believed at any age.
Staleness can only ever reduce confidence that an item is present."
```

---

## Task 4: Basket, sub-intents and lines

**Files:**
- Create: `services/omnideliv/migrations/0003_create_baskets.sql`, `src/domain/entities/basket.rs`

- [ ] **Step 1: Write the migration**

```sql
-- The basket is the mesh's shared state. One row per customer session; one
-- sub_intent per fanned-out specialist; lines belong to a sub_intent so each
-- specialist's contribution stays attributable.

CREATE TABLE IF NOT EXISTS omnideliv.baskets (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID        NOT NULL,
    customer_id     UUID        NOT NULL,
    status          TEXT        NOT NULL DEFAULT 'draft'
                                CHECK (status IN ('draft','proposed','awaiting_review','confirmed','abandoned')),
    -- The mesh run that produced this basket. Links the basket to its agent
    -- audit trail (agent_sessions) so any line can be traced to the turn that
    -- proposed it.
    mesh_session_id UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_basket_customer
    ON omnideliv.baskets (tenant_id, customer_id, created_at DESC);

-- One row per vertical the Concierge split the utterance into. This is what
-- makes "agents are roles instantiated per sub-intent" concrete: two grocery +
-- restaurant sub-intents mean two Nutritionist workers.
CREATE TABLE IF NOT EXISTS omnideliv.sub_intents (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    basket_id   UUID        NOT NULL REFERENCES omnideliv.baskets(id) ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL,
    vertical    TEXT        NOT NULL
                            CHECK (vertical IN ('restaurant','grocery','pharmacy','florist','retail')),
    vendor_hint TEXT,
    -- The slice of the customer's utterance this sub-intent came from. Kept for
    -- audit and for showing the user what the agent thought they asked for.
    raw_text    TEXT        NOT NULL,
    -- Budget, dietary, timing constraints lifted from the CDP profile.
    constraints JSONB       NOT NULL DEFAULT '{}',
    status      TEXT        NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending','satisfied','degraded','failed')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_sub_intent_basket ON omnideliv.sub_intents (basket_id);

CREATE TABLE IF NOT EXISTS omnideliv.basket_lines (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    basket_id         UUID        NOT NULL REFERENCES omnideliv.baskets(id) ON DELETE CASCADE,
    sub_intent_id     UUID        NOT NULL REFERENCES omnideliv.sub_intents(id) ON DELETE CASCADE,
    tenant_id         UUID        NOT NULL,
    vendor_id         UUID        NOT NULL REFERENCES omnideliv.vendors(id),
    item_id           UUID        NOT NULL REFERENCES omnideliv.catalog_items(id),
    qty               INT         NOT NULL CHECK (qty > 0),
    -- Price captured when the line was proposed. The catalog price may move
    -- before checkout; the customer pays what they were shown.
    unit_price_cents  BIGINT      NOT NULL CHECK (unit_price_cents >= 0),
    state             TEXT        NOT NULL DEFAULT 'proposed'
                                  CHECK (state IN ('proposed','accepted','substituted','rejected')),
    -- Set on a replacement line, pointing at the line it replaces. Self-FK so a
    -- substitution chain is walkable for the review UI and for audit.
    substitution_for  UUID        REFERENCES omnideliv.basket_lines(id),
    proposed_by_agent TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_basket_line_basket ON omnideliv.basket_lines (basket_id);
CREATE INDEX IF NOT EXISTS idx_basket_line_sub_intent ON omnideliv.basket_lines (sub_intent_id);
```

- [ ] **Step 2: Write the failing test**

The single-writer property is what Plan 4's mesh depends on, so it gets the most explicit test.

```rust
// services/omnideliv/src/domain/entities/basket.rs — tests block
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn basket() -> Basket {
        Basket::new(Uuid::new_v4(), Uuid::new_v4())
    }

    fn line(basket_id: Uuid, sub_intent_id: Uuid, price: i64, qty: i32) -> BasketLine {
        BasketLine::propose(
            basket_id, sub_intent_id, Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(),
            qty, price, "nutritionist",
        )
    }

    #[test]
    fn a_new_basket_is_draft_and_empty() {
        let b = basket();
        assert_eq!(b.status, BasketStatus::Draft);
        assert!(b.lines.is_empty());
        assert_eq!(b.goods_total_cents(), 0);
    }

    /// The single-writer property. Specialists return deltas; only this method
    /// mutates the basket. Applying two deltas from two concurrent specialists
    /// must produce a deterministic union, not a lost update.
    #[test]
    fn applying_two_deltas_merges_both_without_loss() {
        let mut b = basket();
        let si_food = Uuid::new_v4();
        let si_grocery = Uuid::new_v4();

        b.apply(BasketDelta {
            sub_intent_id: si_food,
            lines: vec![line(b.id, si_food, 34_000, 1)],
            note: None,
        });
        b.apply(BasketDelta {
            sub_intent_id: si_grocery,
            lines: vec![line(b.id, si_grocery, 12_000, 2)],
            note: None,
        });

        assert_eq!(b.lines.len(), 2, "both specialists' lines must survive");
        assert_eq!(b.goods_total_cents(), 34_000 + 24_000);
    }

    /// Re-applying a delta for the same sub-intent replaces that sub-intent's
    /// lines rather than duplicating them — a specialist that retries must not
    /// double the basket.
    #[test]
    fn reapplying_a_delta_replaces_that_sub_intents_lines() {
        let mut b = basket();
        let si = Uuid::new_v4();

        b.apply(BasketDelta { sub_intent_id: si, lines: vec![line(b.id, si, 10_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si, lines: vec![line(b.id, si, 15_000, 1)], note: None });

        assert_eq!(b.lines.len(), 1, "a retry must replace, not append");
        assert_eq!(b.goods_total_cents(), 15_000);
    }

    /// A delta for one sub-intent must never disturb another's lines.
    #[test]
    fn reapplying_one_sub_intent_leaves_the_others_alone() {
        let mut b = basket();
        let si_a = Uuid::new_v4();
        let si_b = Uuid::new_v4();

        b.apply(BasketDelta { sub_intent_id: si_a, lines: vec![line(b.id, si_a, 10_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si_b, lines: vec![line(b.id, si_b, 20_000, 1)], note: None });
        b.apply(BasketDelta { sub_intent_id: si_a, lines: vec![line(b.id, si_a, 11_000, 1)], note: None });

        assert_eq!(b.lines.len(), 2);
        assert_eq!(b.goods_total_cents(), 11_000 + 20_000);
    }

    #[test]
    fn rejected_lines_do_not_count_toward_the_total() {
        let mut b = basket();
        let si = Uuid::new_v4();
        let mut l = line(b.id, si, 9_000, 1);
        l.state = LineState::Rejected;
        b.apply(BasketDelta { sub_intent_id: si, lines: vec![l], note: None });

        assert_eq!(b.goods_total_cents(), 0, "a rejected line is not charged for");
    }

    /// Screen C surfaces exactly the lines that block checkout.
    #[test]
    fn lines_awaiting_review_are_the_ones_needing_a_decision() {
        let mut b = basket();
        let si = Uuid::new_v4();

        let accepted = { let mut l = line(b.id, si, 1_000, 1); l.state = LineState::Accepted; l };
        let swapped  = { let mut l = line(b.id, si, 2_000, 1); l.state = LineState::Substituted; l };

        b.apply(BasketDelta { sub_intent_id: si, lines: vec![accepted, swapped], note: None });

        let pending = b.lines_awaiting_review();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, LineState::Substituted);
    }
}
```

- [ ] **Step 3: Run it to verify it fails**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::`
Expected: FAIL to compile — `cannot find type 'Basket' in this scope`.

- [ ] **Step 4: Write the entities**

```rust
//! The basket — the mesh's shared state.
//!
//! SINGLE WRITER: concurrent specialists never mutate a basket. Each returns a
//! `BasketDelta` scoped to its own sub-intent, and only `Basket::apply` writes.
//! That is what makes budget, timing and temperature conflicts surface
//! deterministically in the reconcile phase instead of as a race.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::Vertical;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BasketStatus {
    Draft,
    Proposed,
    AwaitingReview,
    Confirmed,
    Abandoned,
}

impl BasketStatus {
    /// The wire and database representation. One definition, so the API and the
    /// repository can never disagree — `format!("{:?}").to_lowercase()` would
    /// render `AwaitingReview` as `awaitingreview` and silently drift from the
    /// `awaiting_review` the CHECK constraint expects.
    pub fn as_str(&self) -> &'static str {
        match self {
            BasketStatus::Draft          => "draft",
            BasketStatus::Proposed       => "proposed",
            BasketStatus::AwaitingReview => "awaiting_review",
            BasketStatus::Confirmed      => "confirmed",
            BasketStatus::Abandoned      => "abandoned",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubIntentStatus {
    Pending,
    Satisfied,
    /// The specialist failed or timed out; this vertical falls back to manual
    /// browse. One degraded sub-intent must not fail the whole basket.
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineState {
    Proposed,
    Accepted,
    Substituted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubIntent {
    pub id:          Uuid,
    pub basket_id:   Uuid,
    pub tenant_id:   Uuid,
    pub vertical:    Vertical,
    pub vendor_hint: Option<String>,
    pub raw_text:    String,
    pub constraints: serde_json::Value,
    pub status:      SubIntentStatus,
    pub created_at:  DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasketLine {
    pub id:                Uuid,
    pub basket_id:         Uuid,
    pub sub_intent_id:     Uuid,
    pub tenant_id:         Uuid,
    pub vendor_id:         Uuid,
    pub item_id:           Uuid,
    pub qty:               i32,
    /// Captured at proposal time — the customer pays what they were shown, even
    /// if the catalog price moves before checkout.
    pub unit_price_cents:  i64,
    pub state:             LineState,
    pub substitution_for:  Option<Uuid>,
    pub proposed_by_agent: Option<String>,
    pub created_at:        DateTime<Utc>,
}

impl BasketLine {
    #[allow(clippy::too_many_arguments)]
    pub fn propose(
        basket_id: Uuid,
        sub_intent_id: Uuid,
        tenant_id: Uuid,
        vendor_id: Uuid,
        item_id: Uuid,
        qty: i32,
        unit_price_cents: i64,
        agent: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            basket_id,
            sub_intent_id,
            tenant_id,
            vendor_id,
            item_id,
            qty,
            unit_price_cents,
            state: LineState::Proposed,
            substitution_for: None,
            proposed_by_agent: Some(agent.to_string()),
            created_at: Utc::now(),
        }
    }

    pub fn subtotal_cents(&self) -> i64 {
        self.unit_price_cents * self.qty as i64
    }

    /// Does this line contribute to what the customer pays?
    pub fn is_chargeable(&self) -> bool {
        self.state != LineState::Rejected
    }
}

/// A specialist's contribution, scoped to one sub-intent.
///
/// Deltas are the only way lines enter a basket. A specialist that cannot
/// satisfy its sub-intent returns an empty delta with a `note` — it never
/// writes a partial basket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasketDelta {
    pub sub_intent_id: Uuid,
    pub lines:         Vec<BasketLine>,
    pub note:          Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Basket {
    pub id:              Uuid,
    pub tenant_id:       Uuid,
    pub customer_id:     Uuid,
    pub status:          BasketStatus,
    pub mesh_session_id: Option<Uuid>,
    pub sub_intents:     Vec<SubIntent>,
    pub lines:           Vec<BasketLine>,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Basket {
    pub fn new(tenant_id: Uuid, customer_id: Uuid) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            tenant_id,
            customer_id,
            status: BasketStatus::Draft,
            mesh_session_id: None,
            sub_intents: Vec::new(),
            lines: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// **The single writer.** Replaces this sub-intent's lines wholesale and
    /// leaves every other sub-intent untouched.
    ///
    /// Replace rather than append so a specialist that retries — or that the
    /// runner re-drives after a transient failure — cannot double the basket.
    /// Scoping by `sub_intent_id` is what lets concurrent specialists write
    /// without coordinating: their deltas are disjoint by construction.
    pub fn apply(&mut self, delta: BasketDelta) {
        self.lines.retain(|l| l.sub_intent_id != delta.sub_intent_id);
        self.lines.extend(delta.lines);
        self.updated_at = Utc::now();
    }

    /// What the customer pays for goods, before delivery fee and tip.
    pub fn goods_total_cents(&self) -> i64 {
        self.lines.iter().filter(|l| l.is_chargeable()).map(|l| l.subtotal_cents()).sum()
    }

    /// The lines Screen C must surface — the only ones blocking checkout.
    pub fn lines_awaiting_review(&self) -> Vec<&BasketLine> {
        self.lines.iter().filter(|l| l.state == LineState::Substituted).collect()
    }

    /// Per-vendor goods subtotals, for the vendor payout legs in Plan 5.
    pub fn subtotals_by_vendor(&self) -> std::collections::HashMap<Uuid, i64> {
        let mut out = std::collections::HashMap::new();
        for l in self.lines.iter().filter(|l| l.is_chargeable()) {
            *out.entry(l.vendor_id).or_insert(0) += l.subtotal_cents();
        }
        out
    }
}
```

Add to `src/domain/entities/mod.rs`:

```rust
pub mod basket;
pub use basket::{
    Basket, BasketDelta, BasketLine, BasketStatus, LineState, SubIntent, SubIntentStatus,
};
```

- [ ] **Step 5: Run the tests**

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv basket::`
Expected: PASS — 6 passed.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/migrations/0003_create_baskets.sql services/omnideliv/src/domain/
git commit -m "feat(omnideliv): basket with single-writer BasketDelta application

Concurrent specialists never mutate a basket. Each returns a delta scoped to
its own sub-intent, and Basket::apply is the only writer — it replaces that
sub-intent's lines wholesale so a retry cannot double the basket, and leaves
every other sub-intent untouched so disjoint specialists need no coordination."
```

---

## Task 5: Repository traits

**Files:**
- Create: `services/omnideliv/src/domain/repositories/mod.rs`

- [ ] **Step 1: Write the traits**

```rust
//! Repository contracts.
//!
//! TENANCY: every method takes `tenant_id` first. There is no database-level
//! policy in this schema (see migration 0001), so the signature is the
//! enforcement point.

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::entities::{Availability, Basket, CatalogItem, Vendor, Vertical};

#[async_trait]
pub trait VendorRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>>;
    async fn save(&self, vendor: &Vendor) -> anyhow::Result<()>;

    /// Orderable vendors of a vertical within `radius_km`, nearest first.
    async fn find_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>>;
}

/// An item paired with its current availability declaration. Returned together
/// because an agent needs both to decide anything — an item without its
/// freshness stamp cannot be reasoned about honestly.
#[derive(Debug, Clone)]
pub struct ItemWithAvailability {
    pub item:         CatalogItem,
    pub availability: Availability,
}

#[async_trait]
pub trait CatalogRepository: Send + Sync {
    async fn save_item(&self, item: &CatalogItem) -> anyhow::Result<()>;
    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()>;

    /// Listed items for a vendor, each with its availability.
    async fn list_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<ItemWithAvailability>>;

    /// Text search within a vendor, excluding items that clash with `avoid_allergens`.
    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ItemWithAvailability>>;
}

#[async_trait]
pub trait BasketRepository: Send + Sync {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>>;
    /// Persists the basket and its sub-intents and lines as one unit.
    async fn save(&self, basket: &Basket) -> anyhow::Result<()>;
}
```

- [ ] **Step 2: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: FAIL on missing `api`, `application`, `bootstrap`, `infrastructure` only.

- [ ] **Step 3: Commit**

```bash
git add services/omnideliv/src/domain/repositories/
git commit -m "feat(omnideliv): tenant-scoped repository contracts"
```

---

## Task 6: Postgres repositories

**Files:**
- Create: `src/infrastructure/mod.rs`, `src/infrastructure/db/mod.rs`, `src/infrastructure/db/vendor_repo.rs`, `catalog_repo.rs`, `basket_repo.rs`

- [ ] **Step 1: Write the vendor repository**

```rust
// services/omnideliv/src/infrastructure/db/vendor_repo.rs
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Vendor, VendorStatus, Vertical};
use crate::domain::repositories::VendorRepository;

pub struct PgVendorRepository { pool: PgPool }

impl PgVendorRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

pub(crate) fn parse_vertical(s: &str) -> anyhow::Result<Vertical> {
    Ok(match s {
        "restaurant" => Vertical::Restaurant,
        "grocery"    => Vertical::Grocery,
        "pharmacy"   => Vertical::Pharmacy,
        "florist"    => Vertical::Florist,
        "retail"     => Vertical::Retail,
        other => anyhow::bail!("unknown vertical in database: {other}"),
    })
}

fn map_row(r: &sqlx::postgres::PgRow) -> anyhow::Result<Vendor> {
    let status_str: String = r.get("status");
    let status = match status_str.as_str() {
        "onboarding" => VendorStatus::Onboarding,
        "active"     => VendorStatus::Active,
        "paused"     => VendorStatus::Paused,
        "offboarded" => VendorStatus::Offboarded,
        other => anyhow::bail!("unknown vendor status in database: {other}"),
    };
    let vertical_str: String = r.get("vertical");

    Ok(Vendor {
        id:                r.get("id"),
        tenant_id:         r.get("tenant_id"),
        vertical:          parse_vertical(&vertical_str)?,
        name:              r.get("name"),
        address:           r.get("address"),
        lat:               r.get("lat"),
        lng:               r.get("lng"),
        prep_time_minutes: r.get("prep_time_minutes"),
        commission_bps:    r.get("commission_bps"),
        payout_account:    r.get("payout_account"),
        hours:             r.get("hours"),
        status,
        created_at:        r.get("created_at"),
        updated_at:        r.get("updated_at"),
    })
}

#[async_trait]
impl VendorRepository for PgVendorRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Vendor>> {
        let row = sqlx::query("SELECT * FROM omnideliv.vendors WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?;
        row.as_ref().map(map_row).transpose()
    }

    async fn save(&self, v: &Vendor) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO omnideliv.vendors (
                id, tenant_id, vertical, name, address, lat, lng,
                prep_time_minutes, commission_bps, payout_account, hours, status,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                name              = EXCLUDED.name,
                address           = EXCLUDED.address,
                lat               = EXCLUDED.lat,
                lng               = EXCLUDED.lng,
                prep_time_minutes = EXCLUDED.prep_time_minutes,
                commission_bps    = EXCLUDED.commission_bps,
                payout_account    = EXCLUDED.payout_account,
                hours             = EXCLUDED.hours,
                status            = EXCLUDED.status,
                updated_at        = EXCLUDED.updated_at
            "#,
        )
        .bind(v.id).bind(v.tenant_id).bind(v.vertical.as_str())
        .bind(&v.name).bind(&v.address).bind(v.lat).bind(v.lng)
        .bind(v.prep_time_minutes).bind(v.commission_bps)
        .bind(&v.payout_account).bind(&v.hours).bind(v.status.as_str())
        .bind(v.created_at).bind(v.updated_at)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn find_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>> {
        // Haversine in a subquery so the computed distance is filterable — a
        // WHERE clause cannot reference a SELECT-list alias.
        let rows = sqlx::query(
            r#"
            SELECT * FROM (
                SELECT *,
                       6371 * 2 * ASIN(SQRT(
                           POWER(SIN(RADIANS($4 - lat) / 2), 2) +
                           COS(RADIANS(lat)) * COS(RADIANS($4)) *
                           POWER(SIN(RADIANS($5 - lng) / 2), 2)
                       )) AS distance_km
                FROM omnideliv.vendors
                WHERE tenant_id = $1 AND vertical = $2 AND status = 'active'
            ) AS scored
            WHERE distance_km <= $3
            ORDER BY distance_km ASC
            LIMIT $6
            "#,
        )
        .bind(tenant_id).bind(vertical.as_str()).bind(radius_km)
        .bind(lat).bind(lng).bind(limit)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_row).collect()
    }
}
```

- [ ] **Step 2: Write the catalog repository**

```rust
// services/omnideliv/src/infrastructure/db/catalog_repo.rs
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{Availability, AvailabilityState, CatalogItem};
use crate::domain::repositories::{CatalogRepository, ItemWithAvailability};

pub struct PgCatalogRepository { pool: PgPool }

impl PgCatalogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn map_pair(r: &sqlx::postgres::PgRow) -> anyhow::Result<ItemWithAvailability> {
    let state_str: String = r.get("state");
    let state = match state_str.as_str() {
        "available"    => AvailabilityState::Available,
        "limited"      => AvailabilityState::Limited,
        "out_of_stock" => AvailabilityState::OutOfStock,
        other => anyhow::bail!("unknown availability state in database: {other}"),
    };

    let item = CatalogItem {
        id:             r.get("id"),
        tenant_id:      r.get("tenant_id"),
        vendor_id:      r.get("vendor_id"),
        sku:            r.get("sku"),
        name:           r.get("name"),
        description:    r.get("description"),
        price_cents:    r.get("price_cents"),
        modifiers:      r.get("modifiers"),
        allergens:      r.get("allergens"),
        dietary_tags:   r.get("dietary_tags"),
        vertical_attrs: r.get("vertical_attrs"),
        is_listed:      r.get("is_listed"),
        created_at:     r.get("created_at"),
        updated_at:     r.get("updated_at"),
    };

    let availability = Availability {
        item_id:    item.id,
        tenant_id:  item.tenant_id,
        state,
        updated_at: r.get("availability_updated_at"),
        updated_by: r.get("updated_by"),
    };

    Ok(ItemWithAvailability { item, availability })
}

#[async_trait]
impl CatalogRepository for PgCatalogRepository {
    async fn save_item(&self, i: &CatalogItem) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.catalog_items (
                id, tenant_id, vendor_id, sku, name, description, price_cents,
                modifiers, allergens, dietary_tags, vertical_attrs, is_listed,
                created_at, updated_at
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)
            ON CONFLICT (id) DO UPDATE SET
                name           = EXCLUDED.name,
                description    = EXCLUDED.description,
                price_cents    = EXCLUDED.price_cents,
                modifiers      = EXCLUDED.modifiers,
                allergens      = EXCLUDED.allergens,
                dietary_tags   = EXCLUDED.dietary_tags,
                vertical_attrs = EXCLUDED.vertical_attrs,
                is_listed      = EXCLUDED.is_listed,
                updated_at     = EXCLUDED.updated_at
            "#,
        )
        .bind(i.id).bind(i.tenant_id).bind(i.vendor_id)
        .bind(&i.sku).bind(&i.name).bind(&i.description).bind(i.price_cents)
        .bind(&i.modifiers).bind(&i.allergens).bind(&i.dietary_tags)
        .bind(&i.vertical_attrs).bind(i.is_listed)
        .bind(i.created_at).bind(i.updated_at)
        .execute(&mut *tx).await?;

        // A new item is available by default — but the freshness stamp starts
        // now, so it is honestly "declared present just now" rather than
        // silently inheriting trust it has not earned.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at)
            VALUES ($1, $2, 'available', NOW())
            ON CONFLICT (item_id) DO NOTHING
            "#,
        )
        .bind(i.id).bind(i.tenant_id)
        .execute(&mut *tx).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        // updated_at is set to NOW() server-side rather than trusting the
        // caller's clock — the freshness stamp is only meaningful if it records
        // when the declaration actually reached us.
        sqlx::query(
            r#"
            INSERT INTO omnideliv.item_availability (item_id, tenant_id, state, updated_at, updated_by)
            VALUES ($1, $2, $3, NOW(), $4)
            ON CONFLICT (item_id) DO UPDATE SET
                state      = EXCLUDED.state,
                updated_at = NOW(),
                updated_by = EXCLUDED.updated_by
            "#,
        )
        .bind(a.item_id).bind(a.tenant_id).bind(a.state.as_str()).bind(a.updated_by)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn list_for_vendor(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> {
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1 AND i.vendor_id = $2 AND i.is_listed
             ORDER BY i.name
            "#,
        )
        .bind(tenant_id).bind(vendor_id)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_pair).collect()
    }

    async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ItemWithAvailability>> {
        // Allergen exclusion uses && (array overlap) against the GIN index.
        // Out-of-stock items are deliberately NOT filtered out here — the
        // Nutritionist needs to see them to propose a substitute, and hiding
        // them would make "we swapped X for Y" impossible to explain.
        let rows = sqlx::query(
            r#"
            SELECT i.*, a.state, a.updated_at AS availability_updated_at, a.updated_by
              FROM omnideliv.catalog_items i
              JOIN omnideliv.item_availability a ON a.item_id = i.id
             WHERE i.tenant_id = $1
               AND i.vendor_id = $2
               AND i.is_listed
               AND (i.name ILIKE '%' || $3 || '%' OR i.description ILIKE '%' || $3 || '%')
               AND NOT (i.allergens && $4::TEXT[])
             ORDER BY i.name
             LIMIT $5
            "#,
        )
        .bind(tenant_id).bind(vendor_id).bind(query).bind(avoid_allergens).bind(limit)
        .fetch_all(&self.pool).await?;

        rows.iter().map(map_pair).collect()
    }
}
```

- [ ] **Step 3: Write the basket repository**

```rust
// services/omnideliv/src/infrastructure/db/basket_repo.rs
use async_trait::async_trait;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domain::entities::{
    Basket, BasketLine, BasketStatus, LineState, SubIntent, SubIntentStatus,
};
use crate::domain::repositories::BasketRepository;
use crate::infrastructure::db::vendor_repo::parse_vertical;

pub struct PgBasketRepository { pool: PgPool }

impl PgBasketRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

fn basket_status(s: &str) -> anyhow::Result<BasketStatus> {
    Ok(match s {
        "draft"           => BasketStatus::Draft,
        "proposed"        => BasketStatus::Proposed,
        "awaiting_review" => BasketStatus::AwaitingReview,
        "confirmed"       => BasketStatus::Confirmed,
        "abandoned"       => BasketStatus::Abandoned,
        other => anyhow::bail!("unknown basket status in database: {other}"),
    })
}

fn line_state(s: &str) -> anyhow::Result<LineState> {
    Ok(match s {
        "proposed"    => LineState::Proposed,
        "accepted"    => LineState::Accepted,
        "substituted" => LineState::Substituted,
        "rejected"    => LineState::Rejected,
        other => anyhow::bail!("unknown line state in database: {other}"),
    })
}

fn line_state_str(s: LineState) -> &'static str {
    match s {
        LineState::Proposed    => "proposed",
        LineState::Accepted    => "accepted",
        LineState::Substituted => "substituted",
        LineState::Rejected    => "rejected",
    }
}

fn sub_intent_status(s: &str) -> anyhow::Result<SubIntentStatus> {
    Ok(match s {
        "pending"   => SubIntentStatus::Pending,
        "satisfied" => SubIntentStatus::Satisfied,
        "degraded"  => SubIntentStatus::Degraded,
        "failed"    => SubIntentStatus::Failed,
        other => anyhow::bail!("unknown sub-intent status in database: {other}"),
    })
}

fn sub_intent_status_str(s: SubIntentStatus) -> &'static str {
    match s {
        SubIntentStatus::Pending   => "pending",
        SubIntentStatus::Satisfied => "satisfied",
        SubIntentStatus::Degraded  => "degraded",
        SubIntentStatus::Failed    => "failed",
    }
}

#[async_trait]
impl BasketRepository for PgBasketRepository {
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>> {
        let Some(b) = sqlx::query("SELECT * FROM omnideliv.baskets WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id).bind(id)
            .fetch_optional(&self.pool).await?
        else {
            return Ok(None);
        };

        let status_str: String = b.get("status");

        let si_rows = sqlx::query("SELECT * FROM omnideliv.sub_intents WHERE basket_id = $1 ORDER BY created_at")
            .bind(id).fetch_all(&self.pool).await?;
        let mut sub_intents = Vec::with_capacity(si_rows.len());
        for r in &si_rows {
            let vertical_str: String = r.get("vertical");
            let st: String = r.get("status");
            sub_intents.push(SubIntent {
                id:          r.get("id"),
                basket_id:   r.get("basket_id"),
                tenant_id:   r.get("tenant_id"),
                vertical:    parse_vertical(&vertical_str)?,
                vendor_hint: r.get("vendor_hint"),
                raw_text:    r.get("raw_text"),
                constraints: r.get("constraints"),
                status:      sub_intent_status(&st)?,
                created_at:  r.get("created_at"),
            });
        }

        let line_rows = sqlx::query("SELECT * FROM omnideliv.basket_lines WHERE basket_id = $1 ORDER BY created_at")
            .bind(id).fetch_all(&self.pool).await?;
        let mut lines = Vec::with_capacity(line_rows.len());
        for r in &line_rows {
            let st: String = r.get("state");
            lines.push(BasketLine {
                id:                r.get("id"),
                basket_id:         r.get("basket_id"),
                sub_intent_id:     r.get("sub_intent_id"),
                tenant_id:         r.get("tenant_id"),
                vendor_id:         r.get("vendor_id"),
                item_id:           r.get("item_id"),
                qty:               r.get("qty"),
                unit_price_cents:  r.get("unit_price_cents"),
                state:             line_state(&st)?,
                substitution_for:  r.get("substitution_for"),
                proposed_by_agent: r.get("proposed_by_agent"),
                created_at:        r.get("created_at"),
            });
        }

        Ok(Some(Basket {
            id:              b.get("id"),
            tenant_id:       b.get("tenant_id"),
            customer_id:     b.get("customer_id"),
            status:          basket_status(&status_str)?,
            mesh_session_id: b.get("mesh_session_id"),
            sub_intents,
            lines,
            created_at:      b.get("created_at"),
            updated_at:      b.get("updated_at"),
        }))
    }

    async fn save(&self, basket: &Basket) -> anyhow::Result<()> {
        // One transaction for the whole aggregate. `apply` replaces a
        // sub-intent's lines in memory, so persistence must mirror that: delete
        // then re-insert, or a removed line survives in the database.
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            INSERT INTO omnideliv.baskets (id, tenant_id, customer_id, status, mesh_session_id, created_at, updated_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            ON CONFLICT (id) DO UPDATE SET
                status          = EXCLUDED.status,
                mesh_session_id = EXCLUDED.mesh_session_id,
                updated_at      = EXCLUDED.updated_at
            "#,
        )
        .bind(basket.id).bind(basket.tenant_id).bind(basket.customer_id)
        .bind(basket.status.as_str()).bind(basket.mesh_session_id)
        .bind(basket.created_at).bind(basket.updated_at)
        .execute(&mut *tx).await?;

        for si in &basket.sub_intents {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.sub_intents (
                    id, basket_id, tenant_id, vertical, vendor_hint, raw_text,
                    constraints, status, created_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                ON CONFLICT (id) DO UPDATE SET status = EXCLUDED.status
                "#,
            )
            .bind(si.id).bind(si.basket_id).bind(si.tenant_id)
            .bind(si.vertical.as_str()).bind(&si.vendor_hint).bind(&si.raw_text)
            .bind(&si.constraints).bind(sub_intent_status_str(si.status)).bind(si.created_at)
            .execute(&mut *tx).await?;
        }

        // Lines are replaced wholesale to mirror `Basket::apply`.
        sqlx::query("DELETE FROM omnideliv.basket_lines WHERE basket_id = $1")
            .bind(basket.id)
            .execute(&mut *tx).await?;

        for l in &basket.lines {
            sqlx::query(
                r#"
                INSERT INTO omnideliv.basket_lines (
                    id, basket_id, sub_intent_id, tenant_id, vendor_id, item_id,
                    qty, unit_price_cents, state, substitution_for,
                    proposed_by_agent, created_at
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
                "#,
            )
            .bind(l.id).bind(l.basket_id).bind(l.sub_intent_id).bind(l.tenant_id)
            .bind(l.vendor_id).bind(l.item_id).bind(l.qty).bind(l.unit_price_cents)
            .bind(line_state_str(l.state)).bind(l.substitution_for)
            .bind(&l.proposed_by_agent).bind(l.created_at)
            .execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
```

> **Substitution chains and the wholesale delete.** `basket_lines.substitution_for` is a self-referencing foreign key, and `save` deletes every line for the basket before re-inserting. Within one transaction that is safe *only* if the insert order satisfies the FK — a replacement line must be inserted after the line it points at. `Basket::apply` appends, so `basket.lines` is already in creation order and the ordering holds. If a future change reorders lines, this breaks with a foreign-key violation rather than silently — which is the failure mode we want. Task 8's round-trip test covers it.

- [ ] **Step 4: Wire the modules**

```rust
// services/omnideliv/src/infrastructure/mod.rs
pub mod db;
```

```rust
// services/omnideliv/src/infrastructure/db/mod.rs
pub mod basket_repo;
pub mod catalog_repo;
pub mod vendor_repo;

pub use basket_repo::PgBasketRepository;
pub use catalog_repo::PgCatalogRepository;
pub use vendor_repo::PgVendorRepository;
```

- [ ] **Step 5: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: FAIL on missing `api`, `application`, `bootstrap` only.

- [ ] **Step 6: Commit**

```bash
git add services/omnideliv/src/infrastructure/
git commit -m "feat(omnideliv): Postgres repositories for vendors, catalog and baskets"
```

---

## Task 7: Application services

**Files:**
- Create: `src/application/mod.rs`, `src/application/services/mod.rs`, `src/application/services/catalog_service.rs`, `basket_service.rs`

- [ ] **Step 1: Write the catalog service**

```rust
// services/omnideliv/src/application/services/catalog_service.rs
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{Availability, Vendor, Vertical};
use crate::domain::repositories::{CatalogRepository, ItemWithAvailability, VendorRepository};

/// An item plus the agent-facing judgement about it.
#[derive(Debug, Clone)]
pub struct ScoredItem {
    pub item_with_availability: ItemWithAvailability,
    /// True when the agent should line up a substitute before dispatch.
    pub warrants_substitute:    bool,
}

pub struct CatalogService {
    vendors:        Arc<dyn VendorRepository>,
    catalog:        Arc<dyn CatalogRepository>,
    fresh_window_mins: i64,
}

impl CatalogService {
    pub fn new(
        vendors: Arc<dyn VendorRepository>,
        catalog: Arc<dyn CatalogRepository>,
        fresh_window_mins: i64,
    ) -> Self {
        Self { vendors, catalog, fresh_window_mins }
    }

    pub async fn vendors_near(
        &self,
        tenant_id: Uuid,
        vertical: Vertical,
        lat: f64,
        lng: f64,
        radius_km: f64,
        limit: i64,
    ) -> anyhow::Result<Vec<Vendor>> {
        self.vendors.find_near(tenant_id, vertical, lat, lng, radius_km, limit).await
    }

    /// Search a vendor's catalog, annotating each hit with whether it warrants a
    /// substitute. The freshness judgement lives here rather than in the agent
    /// so every caller applies the same rule with the same configured window.
    pub async fn search(
        &self,
        tenant_id: Uuid,
        vendor_id: Uuid,
        query: &str,
        avoid_allergens: &[String],
        limit: i64,
    ) -> anyhow::Result<Vec<ScoredItem>> {
        let hits = self
            .catalog
            .search(tenant_id, vendor_id, query, avoid_allergens, limit)
            .await?;

        Ok(hits
            .into_iter()
            .map(|iwa| ScoredItem {
                warrants_substitute: iwa.availability.warrants_substitute(self.fresh_window_mins),
                item_with_availability: iwa,
            })
            .collect())
    }

    pub async fn set_availability(&self, a: &Availability) -> anyhow::Result<()> {
        self.catalog.set_availability(a).await
    }
}
```

- [ ] **Step 2: Write the basket service**

```rust
// services/omnideliv/src/application/services/basket_service.rs
use std::sync::Arc;

use uuid::Uuid;

use crate::domain::entities::{Basket, BasketDelta};
use crate::domain::repositories::BasketRepository;

pub struct BasketService {
    baskets: Arc<dyn BasketRepository>,
}

impl BasketService {
    pub fn new(baskets: Arc<dyn BasketRepository>) -> Self { Self { baskets } }

    pub async fn create(&self, tenant_id: Uuid, customer_id: Uuid) -> anyhow::Result<Basket> {
        let b = Basket::new(tenant_id, customer_id);
        self.baskets.save(&b).await?;
        Ok(b)
    }

    pub async fn get(&self, tenant_id: Uuid, id: Uuid) -> anyhow::Result<Option<Basket>> {
        self.baskets.find_by_id(tenant_id, id).await
    }

    /// Apply a specialist's delta.
    ///
    /// Read-modify-write is deliberate and safe here *because* the mesh has a
    /// single writer: only the Concierge calls this, serially, after joining its
    /// fan-out. If a second caller is ever added, this needs optimistic locking
    /// — a version column and a compare-and-swap — or deltas will be lost.
    pub async fn apply_delta(
        &self,
        tenant_id: Uuid,
        basket_id: Uuid,
        delta: BasketDelta,
    ) -> anyhow::Result<Basket> {
        let mut basket = self
            .baskets
            .find_by_id(tenant_id, basket_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("basket {basket_id} not found"))?;

        basket.apply(delta);
        self.baskets.save(&basket).await?;
        Ok(basket)
    }
}
```

```rust
// services/omnideliv/src/application/mod.rs
pub mod services;
```

```rust
// services/omnideliv/src/application/services/mod.rs
pub mod basket_service;
pub mod catalog_service;

pub use basket_service::BasketService;
pub use catalog_service::{CatalogService, ScoredItem};
```

- [ ] **Step 3: Verify it type-checks**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: FAIL on missing `api` and `bootstrap` only.

- [ ] **Step 4: Commit**

```bash
git add services/omnideliv/src/application/
git commit -m "feat(omnideliv): catalog and basket application services

The freshness judgement lives in CatalogService, not in the agent, so every
caller applies the same rule with the same configured window."
```

---

## Task 8: HTTP API, bootstrap, and the round-trip test

**Files:**
- Create: `src/api/mod.rs`, `src/api/http/mod.rs`, `src/api/http/health.rs`, `src/api/http/catalog.rs`, `src/api/http/baskets.rs`, `src/bootstrap.rs`, `tests/basket_roundtrip.rs`
- Modify: `.github/workflows/build-images.yml`

- [ ] **Step 1: Write the health routes**

Unauthenticated, for the same reason as field-ops — an open incident has services showing red for 11 days because `/health` returns 401 and the healthcheck's `curl -sf` fails.

```rust
// services/omnideliv/src/api/http/health.rs
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};

pub fn routes() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/metrics", get(metrics))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "service": "omnideliv" }))
}

async fn ready() -> Json<Value> {
    Json(json!({ "status": "ready" }))
}

async fn metrics() -> String {
    "# HELP omnideliv_up Service liveness\n# TYPE omnideliv_up gauge\nomnideliv_up 1\n".to_string()
}
```

- [ ] **Step 2: Write the router, state and bootstrap**

```rust
// services/omnideliv/src/api/http/mod.rs
pub mod baskets;
pub mod catalog;
pub mod health;

use std::sync::Arc;
use axum::Router;

use crate::application::services::{BasketService, CatalogService};

pub struct AppState {
    pub catalog: Arc<CatalogService>,
    pub baskets: Arc<BasketService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(health::routes())   // open — container healthchecks curl this
        .merge(
            catalog::routes()
                .merge(baskets::routes())
                .with_state(state)
                .layer(axum::middleware::from_fn(logisticos_auth::middleware::require_auth)),
        )
}
```

```rust
// services/omnideliv/src/api/mod.rs
pub mod http;
```

```rust
// services/omnideliv/src/bootstrap.rs
use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

use crate::api::http::{router, AppState};
use crate::application::services::{BasketService, CatalogService};
use crate::config::Config;
use crate::infrastructure::db::{PgBasketRepository, PgCatalogRepository, PgVendorRepository};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load omnideliv config")?;

    let otlp = std::env::var("OTLP_ENDPOINT").ok();
    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "omnideliv",
        env: &cfg.app.env,
        otlp_endpoint: otlp.as_deref(),
        log_level: None,
    })?;

    tracing::info!(env = %cfg.app.env, "omnideliv service starting");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO omnideliv, public")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await
        .context("omnideliv migration failed")?;

    let catalog = Arc::new(CatalogService::new(
        Arc::new(PgVendorRepository::new(pool.clone())),
        Arc::new(PgCatalogRepository::new(pool.clone())),
        cfg.stock_freshness_mins,
    ));
    let baskets = Arc::new(BasketService::new(Arc::new(PgBasketRepository::new(pool.clone()))));

    let state = Arc::new(AppState { catalog, baskets });

    let addr = format!("0.0.0.0:{}", cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr, "omnideliv listening");
    axum::serve(listener, router(state)).await?;

    Ok(())
}
```

- [ ] **Step 3: Write the catalog and basket routes**

```rust
// services/omnideliv/src/api/http/catalog.rs
use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub tenant_id: Uuid,
    pub vendor_id: Uuid,
    pub q:         String,
    /// Comma-separated allergens to exclude.
    #[serde(default)]
    pub avoid:     String,
    #[serde(default = "default_limit")]
    pub limit:     i64,
}

fn default_limit() -> i64 { 20 }

#[derive(Debug, Serialize)]
pub struct SearchHit {
    pub item_id:             Uuid,
    pub name:                String,
    pub price_cents:         i64,
    pub availability:        String,
    /// Surfaced so the caller can see *why* a substitute was proposed.
    pub warrants_substitute: bool,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/v1/omnideliv/catalog/search", get(search))
}

async fn search(
    State(st): State<Arc<AppState>>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<Vec<SearchHit>>, StatusCode> {
    let avoid: Vec<String> = q
        .avoid
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();

    let hits = st
        .catalog
        .search(q.tenant_id, q.vendor_id, &q.q, &avoid, q.limit)
        .await
        .map_err(|e| {
            tracing::error!(err = %e, "catalog search failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        hits.into_iter()
            .map(|h| SearchHit {
                item_id:             h.item_with_availability.item.id,
                name:                h.item_with_availability.item.name,
                price_cents:         h.item_with_availability.item.price_cents,
                availability:        h.item_with_availability.availability.state.as_str().to_string(),
                warrants_substitute: h.warrants_substitute,
            })
            .collect(),
    ))
}
```

```rust
// services/omnideliv/src/api/http/baskets.rs
use std::sync::Arc;

use axum::{extract::{Path, State}, http::StatusCode, routing::{get, post}, Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::http::AppState;

#[derive(Debug, Deserialize)]
pub struct CreateBasketRequest {
    pub tenant_id:   Uuid,
    pub customer_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct BasketResponse {
    pub id:                Uuid,
    pub status:            String,
    pub goods_total_cents: i64,
    pub lines_awaiting_review: usize,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/v1/omnideliv/baskets", post(create))
        .route("/v1/omnideliv/baskets/:id", get(fetch))
}

async fn create(
    State(st): State<Arc<AppState>>,
    Json(req): Json<CreateBasketRequest>,
) -> Result<Json<BasketResponse>, StatusCode> {
    let b = st.baskets.create(req.tenant_id, req.customer_id).await.map_err(|e| {
        tracing::error!(err = %e, "basket create failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(BasketResponse {
        id: b.id,
        status: b.status.as_str().to_string(),
        goods_total_cents: b.goods_total_cents(),
        lines_awaiting_review: b.lines_awaiting_review().len(),
    }))
}

async fn fetch(
    State(st): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<BasketResponse>, StatusCode> {
    // TODO(Plan 4): tenant comes from JWT claims once the mesh wires auth
    // context through; the query param form is a placeholder for local testing.
    let tenant_id = Uuid::nil();

    let b = st
        .baskets
        .get(tenant_id, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(BasketResponse {
        id: b.id,
        status: b.status.as_str().to_string(),
        goods_total_cents: b.goods_total_cents(),
        lines_awaiting_review: b.lines_awaiting_review().len(),
    }))
}
```

- [ ] **Step 4: Verify the crate compiles and unit tests pass**

Run: `CARGO_INCREMENTAL=0 cargo check -p logisticos-omnideliv`
Expected: PASS.

Run: `CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv`
Expected: PASS — 16 tests (5 vendor, 5 catalog, 6 basket).

- [ ] **Step 5: Write the persistence round-trip test**

This covers the substitution-chain FK ordering flagged in Task 6.

```rust
// services/omnideliv/tests/basket_roundtrip.rs
//! The basket aggregate must survive a save/load round trip with its
//! substitution chain intact. Requires a running Postgres; skipped otherwise.

use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

use logisticos_omnideliv::domain::entities::{
    Basket, BasketDelta, BasketLine, CatalogItem, LineState, SubIntent, SubIntentStatus,
    Vendor, Vertical,
};
use logisticos_omnideliv::domain::repositories::{BasketRepository, CatalogRepository, VendorRepository};
use logisticos_omnideliv::infrastructure::db::{
    PgBasketRepository, PgCatalogRepository, PgVendorRepository,
};

#[tokio::test]
async fn a_basket_with_a_substitution_chain_survives_a_round_trip() {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipping: DATABASE_URL not set");
        return;
    };

    let pool = PgPoolOptions::new()
        .after_connect(|c, _| Box::pin(async move {
            sqlx::query("SET search_path TO omnideliv, public").execute(&mut *c).await?;
            Ok(())
        }))
        .connect(&url).await.expect("connect");

    logisticos_common::migrations::run(&pool, "omnideliv", &sqlx::migrate!("./migrations"))
        .await.expect("migrate");

    let tenant = Uuid::new_v4();
    let vendors = PgVendorRepository::new(pool.clone());
    let catalog = PgCatalogRepository::new(pool.clone());
    let baskets = PgBasketRepository::new(pool.clone());

    // Seed a vendor and two items — the original and its replacement.
    let mut vendor = Vendor::new(tenant, Vertical::Grocery, "Test Grocery".into(),
                                 "1 Test St".into(), 14.6, 120.98);
    vendor.activate();
    vendors.save(&vendor).await.expect("save vendor");

    let mk_item = |sku: &str, price: i64| {
        let now = chrono::Utc::now();
        CatalogItem {
            id: Uuid::new_v4(), tenant_id: tenant, vendor_id: vendor.id,
            sku: sku.into(), name: sku.into(), description: None, price_cents: price,
            modifiers: serde_json::json!([]), allergens: vec![], dietary_tags: vec![],
            vertical_attrs: serde_json::json!({}), is_listed: true,
            created_at: now, updated_at: now,
        }
    };
    let original = mk_item("eggs-brand-a", 12_000);
    let replacement = mk_item("eggs-brand-b", 10_800);
    catalog.save_item(&original).await.expect("save original");
    catalog.save_item(&replacement).await.expect("save replacement");

    // Build a basket where the replacement points at the original.
    let mut basket = Basket::new(tenant, Uuid::new_v4());
    let si = SubIntent {
        id: Uuid::new_v4(), basket_id: basket.id, tenant_id: tenant,
        vertical: Vertical::Grocery, vendor_hint: None,
        raw_text: "we're out of milk and eggs".into(),
        constraints: serde_json::json!({}), status: SubIntentStatus::Pending,
        created_at: chrono::Utc::now(),
    };
    basket.sub_intents.push(si.clone());

    let mut out_of_stock = BasketLine::propose(
        basket.id, si.id, tenant, vendor.id, original.id, 1, 12_000, "nutritionist");
    out_of_stock.state = LineState::Rejected;

    let mut swap = BasketLine::propose(
        basket.id, si.id, tenant, vendor.id, replacement.id, 1, 10_800, "nutritionist");
    swap.state = LineState::Substituted;
    swap.substitution_for = Some(out_of_stock.id);

    // Order matters: the replacement's FK points at the original, so the
    // original must be inserted first. `apply` preserves this order.
    basket.apply(BasketDelta { sub_intent_id: si.id, lines: vec![out_of_stock, swap], note: None });

    baskets.save(&basket).await.expect("save basket");

    let loaded = baskets.find_by_id(tenant, basket.id).await
        .expect("load")
        .expect("basket should exist");

    assert_eq!(loaded.lines.len(), 2);
    assert_eq!(loaded.goods_total_cents(), 10_800,
               "the rejected original must not be charged for");
    assert_eq!(loaded.lines_awaiting_review().len(), 1,
               "the substitution is the one decision blocking checkout");

    let chained = loaded.lines.iter().find(|l| l.substitution_for.is_some())
        .expect("substitution chain must survive the round trip");
    assert_eq!(chained.state, LineState::Substituted);
}
```

- [ ] **Step 6: Run the round-trip test**

```bash
DATABASE_URL="postgres://logisticos:logisticos@localhost:5432/svc_omnideliv" CARGO_INCREMENTAL=0 cargo test -p logisticos-omnideliv --test basket_roundtrip
```

Expected: PASS. A foreign-key violation on `substitution_for` means the insert ordering in `PgBasketRepository::save` no longer matches creation order — fix the ordering, do not drop the constraint.

- [ ] **Step 7: Add the service to the image build**

In `.github/workflows/build-images.yml`, add `omnideliv` to the service matrix.

- [ ] **Step 8: Verify the workspace still checks**

Run: `CARGO_INCREMENTAL=0 cargo check --workspace`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add services/omnideliv/ .github/workflows/build-images.yml
git commit -m "feat(omnideliv): HTTP API, bootstrap, and basket round-trip test

The round-trip test covers the substitution-chain foreign key: save deletes
and re-inserts lines wholesale to mirror Basket::apply, which is only safe
while insert order matches creation order. If that ever changes it fails with
an FK violation rather than silently losing the chain."
```

---

## Definition of done

- [ ] `cargo test -p logisticos-omnideliv` — 16 unit tests pass
- [ ] `cargo test -p logisticos-omnideliv --test basket_roundtrip` with `DATABASE_URL` set — passes
- [ ] `cargo check --workspace` — clean
- [ ] `curl -sf localhost:8091/health` returns 200 without a token
- [ ] `rg -ni "merchant" services/omnideliv/src services/omnideliv/migrations` returns nothing (UI copy lives in the portal, not here)
- [ ] `rg -n "ENABLE ROW LEVEL SECURITY" services/omnideliv/migrations/` returns nothing

## Follow-on work this unblocks

1. **Plan 4** — the mesh. `BasketDelta` and `Basket::apply` are the contract it builds on; `CatalogService::search` returning `warrants_substitute` is what the Nutritionist reasons over.
2. **Plan 5** — consolidation, orders and settlement. `Vendor::commission_on`/`payout_on` and `Basket::subtotals_by_vendor` are the inputs to the three-leg split.
3. **Plan 6** — the vendor console, which writes the availability declarations whose freshness this service reasons about.

## Correction — the basket has no non-LLM writer

**This plan builds no way to add a line to a basket outside the mesh.** `Basket::apply` takes a `BasketDelta`, a delta requires a `sub_intent_id`, and only the Concierge creates sub-intents — so after this plan the sole producer of basket lines is the LLM. The two routes here are `POST /v1/omnideliv/baskets` (create empty) and `GET /v1/omnideliv/baskets/:id`.

That gap is closed by **[Plan 8 — Manual Order Path](2026-08-06-omnideliv-manual-order-path.md)**, which adds a browse sub-intent, `Basket::add_line` with append semantics, the line endpoints, and the optimistic lock this plan defers below. Read that plan before assuming the fallback works.

## Known follow-ups inside this service

- **Tenant from JWT.** `baskets::fetch` uses a placeholder `Uuid::nil()` tenant and `catalog::search` takes `tenant_id` as a query parameter. Both must read from validated `Claims` once Plan 4 wires the mesh's auth context. Tracked here rather than left silent — a tenant-from-the-client API is a cross-tenant read waiting to happen, and it must not reach production in this form.
- **Basket concurrency.** `BasketService::apply_delta` is read-modify-write, safe only because the mesh has a single writer. A second writer needs a version column and a compare-and-swap.
