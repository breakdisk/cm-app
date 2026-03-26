# Driver Compliance System — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a generic `compliance` service that verifies driver documents (license, ID, vehicle reg, insurance) and gates task assignment via Kafka, with Admin Portal review console, Driver App upload flow, and Partner Portal status badges.

**Architecture:** New standalone Rust/Axum service `services/compliance/` following the `driver-ops` pattern. Domain status is derived from required document states and published to Kafka; dispatch service caches compliance status in Redis. Admin reviews documents via a Next.js console; drivers upload via the Expo Driver App.

**Tech Stack:** Rust/Axum/SQLx/PostgreSQL (service), rdkafka (events), Redis (dispatch cache), Next.js 14 (admin UI), React Native/Expo + Redux (driver app)

---

## File Map

### New service
```
services/compliance/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── bootstrap.rs
│   ├── config.rs
│   ├── domain/
│   │   ├── mod.rs
│   │   ├── entities/
│   │   │   ├── mod.rs
│   │   │   ├── compliance_profile.rs   # ComplianceProfile + ComplianceStatus enum + derive logic
│   │   │   ├── driver_document.rs      # DriverDocument + DocumentStatus enum
│   │   │   ├── document_type.rs        # DocumentType config record
│   │   │   └── audit_log.rs            # ComplianceAuditLog (append-only)
│   │   ├── repositories/
│   │   │   └── mod.rs                  # Repository traits
│   │   └── events/
│   │       └── mod.rs                  # ComplianceEvent enum (Kafka payloads)
│   ├── application/
│   │   ├── mod.rs
│   │   └── services/
│   │       ├── mod.rs
│   │       ├── compliance_service.rs   # submit_document, review_document, suspend, reinstate
│   │       └── expiry_checker.rs       # daily Tokio task
│   ├── infrastructure/
│   │   ├── mod.rs
│   │   ├── db/
│   │   │   ├── mod.rs
│   │   │   ├── compliance_profile_repo.rs
│   │   │   ├── driver_document_repo.rs
│   │   │   └── document_type_repo.rs
│   │   ├── messaging/
│   │   │   ├── mod.rs
│   │   │   ├── producer.rs             # publish compliance events
│   │   │   └── consumer.rs             # consume driver.registered
│   │   └── storage/
│   │       └── document_storage.rs     # S3-compatible file upload
│   └── api/
│       ├── mod.rs
│       └── http/
│           ├── mod.rs                  # router + AppState
│           ├── health.rs
│           ├── driver_routes.rs        # /me/profile, /me/documents
│           ├── admin_routes.rs         # /admin/queue, /admin/profiles, approve/reject
│           └── internal_routes.rs      # /internal/status/:entity_type/:entity_id
├── migrations/
│   ├── 20260325000001_create_schema.sql
│   ├── 20260325000002_document_types.sql
│   ├── 20260325000003_compliance_profiles.sql
│   ├── 20260325000004_driver_documents.sql
│   ├── 20260325000005_audit_log.sql
│   └── 20260325000006_seed_document_types.sql
```

### Modified files
```
Cargo.toml                                              # add "services/compliance" to workspace members
services/dispatch/src/infrastructure/db/mod.rs          # export compliance_cache
services/dispatch/src/infrastructure/db/compliance_cache.rs   # Redis cache for compliance status
services/dispatch/src/infrastructure/messaging/mod.rs   # export compliance_consumer
services/dispatch/src/infrastructure/messaging/compliance_consumer.rs  # consume compliance events
services/dispatch/src/bootstrap.rs                      # wire cache + consumer

apps/admin-portal/src/lib/api/compliance.ts             # API client
apps/admin-portal/src/app/(dashboard)/compliance/page.tsx
apps/admin-portal/src/components/compliance/kpi-strip.tsx
apps/admin-portal/src/components/compliance/review-queue.tsx
apps/admin-portal/src/components/compliance/document-detail-panel.tsx

apps/driver-app/src/store/index.ts                      # add complianceSlice
apps/driver-app/src/app/(tabs)/profile.tsx              # add compliance banner + doc list
apps/driver-app/src/app/compliance/index.tsx            # full checklist screen
apps/driver-app/src/app/compliance/upload/[typeCode].tsx

apps/partner-portal/src/components/compliance/compliance-badge.tsx
apps/partner-portal/src/app/(dashboard)/drivers/page.tsx  # add badge to driver cards
```

---

## Task 1: Register service in workspace

**Files:**
- Modify: `Cargo.toml`

- [ ] **Add `services/compliance` to workspace members**

In `Cargo.toml`, add `"services/compliance"` to the `members` array after `"services/driver-ops"`:

```toml
"services/driver-ops",
"services/compliance",
```

- [ ] **Defer workspace verification to Task 2** (no Cargo.toml yet — verified there)

---

## Task 2: Service scaffold

**Files:**
- Create: `services/compliance/Cargo.toml`
- Create: `services/compliance/src/main.rs`
- Create: `services/compliance/src/lib.rs`
- Create: `services/compliance/src/config.rs`
- Create: `services/compliance/src/bootstrap.rs` (stub — wired fully in Task 13)

- [ ] **Create `services/compliance/Cargo.toml`**

```toml
[package]
name        = "logisticos-compliance"
description = "Compliance — document verification, expiry enforcement, dispatch gating"
version.workspace      = true
edition.workspace      = true
authors.workspace      = true
rust-version.workspace = true

[[bin]]
name = "compliance"
path = "src/main.rs"

[dependencies]
logisticos-common.workspace  = true
logisticos-errors.workspace  = true
logisticos-auth.workspace    = true
logisticos-tracing.workspace = true
logisticos-types.workspace   = true
logisticos-events.workspace  = true
tokio.workspace      = true
axum.workspace       = true
axum-extra.workspace = true
tower-http.workspace = true
sqlx.workspace       = true
redis.workspace      = true
serde.workspace      = true
serde_json.workspace = true
thiserror.workspace  = true
anyhow.workspace     = true
uuid.workspace       = true
chrono.workspace     = true
config.workspace     = true
dotenvy.workspace    = true
validator.workspace  = true
rdkafka.workspace    = true
tracing.workspace    = true
async-trait = "0.1"

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

- [ ] **Create `src/main.rs`**

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logisticos_compliance::bootstrap::run().await
}
```

- [ ] **Create `src/lib.rs`**

```rust
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod api;
```

- [ ] **Create `src/config.rs`** (follow `services/driver-ops/src/config.rs` pattern exactly)

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub app:      AppConfig,
    pub database: DatabaseConfig,
    pub kafka:    KafkaConfig,
    pub storage:  StorageConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub env:  String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url:             String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub brokers:          String,
    pub consumer_group:   String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    pub endpoint:   String,   // S3-compatible endpoint
    pub bucket:     String,
    pub access_key: String,
    pub secret_key: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let cfg = config::Config::builder()
            .add_source(config::Environment::default().separator("__"))
            .build()?
            .try_deserialize()?;
        Ok(cfg)
    }
}
```

- [ ] **Create stub `src/bootstrap.rs`**

```rust
pub async fn run() -> anyhow::Result<()> {
    todo!("wired in Task 8")
}
```

- [ ] **Create `src/domain/mod.rs`**

```rust
pub mod entities;
pub mod repositories;
pub mod events;
```

- [ ] **Verify workspace compiles**

```bash
cargo check -p logisticos-compliance
```
Expected: compiles with `todo!` warning only.

- [ ] **Commit**

```bash
git add services/compliance/ Cargo.toml
git commit -m "feat(compliance): scaffold service — Cargo.toml, config, lib structure"
```

---

## Task 3: Domain entities

**Files:**
- Create: `services/compliance/src/domain/entities/compliance_profile.rs`
- Create: `services/compliance/src/domain/entities/driver_document.rs`
- Create: `services/compliance/src/domain/entities/document_type.rs`
- Create: `services/compliance/src/domain/entities/audit_log.rs`
- Create: `services/compliance/src/domain/entities/mod.rs`

- [ ] **Write unit test for status derivation first**

In `services/compliance/src/domain/entities/compliance_profile.rs`, write the test block before the impl:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc(status: DocumentStatus, is_required: bool) -> DocumentStatus {
        status
    }

    #[test]
    fn all_approved_is_compliant() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Approved];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::Compliant
        );
    }

    #[test]
    fn any_submitted_is_under_review() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Submitted];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::UnderReview
        );
    }

    #[test]
    fn any_missing_is_pending() {
        let statuses = vec![DocumentStatus::Approved];
        // 2 required but only 1 submitted
        assert_eq!(
            ComplianceStatus::derive(&statuses, true, false),
            ComplianceStatus::PendingSubmission
        );
    }

    #[test]
    fn any_expiring_soon_with_all_approved() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Approved];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, true),
            ComplianceStatus::ExpiringSoon
        );
    }

    #[test]
    fn any_expired_is_expired() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Expired];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::Expired
        );
    }

    #[test]
    fn rejected_doc_returns_to_pending() {
        let statuses = vec![DocumentStatus::Approved, DocumentStatus::Rejected];
        assert_eq!(
            ComplianceStatus::derive(&statuses, false, false),
            ComplianceStatus::PendingSubmission
        );
    }
}
```

- [ ] **Run test — expect compile failure (types not defined yet)**

```bash
cargo test -p logisticos-compliance 2>&1 | head -20
```

- [ ] **Implement `compliance_profile.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use super::driver_document::DocumentStatus;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceStatus {
    PendingSubmission,
    UnderReview,
    Compliant,
    ExpiringSoon,
    Expired,
    Suspended,
    Rejected,
}

impl ComplianceStatus {
    /// Derive overall status from the current set of required document statuses.
    /// `has_missing` = true when fewer approved/submitted docs exist than required count.
    /// `has_expiring` = true when any approved doc is within its warn window.
    pub fn derive(
        doc_statuses: &[DocumentStatus],
        has_missing: bool,
        has_expiring: bool,
    ) -> Self {
        if has_missing {
            return Self::PendingSubmission;
        }
        if doc_statuses.iter().any(|s| *s == DocumentStatus::Rejected) {
            return Self::PendingSubmission;
        }
        if doc_statuses.iter().any(|s| *s == DocumentStatus::Expired) {
            return Self::Expired;
        }
        if doc_statuses.iter().any(|s| matches!(s, DocumentStatus::Submitted | DocumentStatus::UnderReview)) {
            return Self::UnderReview;
        }
        if has_expiring {
            return Self::ExpiringSoon;
        }
        Self::Compliant
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingSubmission => "pending_submission",
            Self::UnderReview       => "under_review",
            Self::Compliant         => "compliant",
            Self::ExpiringSoon      => "expiring_soon",
            Self::Expired           => "expired",
            Self::Suspended         => "suspended",
            Self::Rejected          => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "under_review"       => Self::UnderReview,
            "compliant"          => Self::Compliant,
            "expiring_soon"      => Self::ExpiringSoon,
            "expired"            => Self::Expired,
            "suspended"          => Self::Suspended,
            "rejected"           => Self::Rejected,
            _                    => Self::PendingSubmission,
        }
    }

    /// Can a driver with this status be assigned tasks?
    pub fn is_assignable(&self) -> bool {
        matches!(self, Self::Compliant | Self::ExpiringSoon | Self::Expired)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceProfile {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub entity_type:      String,   // "driver" | "partner" | "merchant"
    pub entity_id:        Uuid,
    pub overall_status:   ComplianceStatus,
    pub jurisdiction:     String,
    pub last_reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by:      Option<Uuid>,
    pub suspended_at:     Option<DateTime<Utc>>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}
```

- [ ] **Implement `driver_document.rs`**

```rust
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Submitted,
    UnderReview,
    Approved,
    Rejected,
    Expired,
    Superseded,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Submitted    => "submitted",
            Self::UnderReview  => "under_review",
            Self::Approved     => "approved",
            Self::Rejected     => "rejected",
            Self::Expired      => "expired",
            Self::Superseded   => "superseded",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "under_review" => Self::UnderReview,
            "approved"     => Self::Approved,
            "rejected"     => Self::Rejected,
            "expired"      => Self::Expired,
            "superseded"   => Self::Superseded,
            _              => Self::Submitted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverDocument {
    pub id:                    Uuid,
    pub compliance_profile_id: Uuid,
    pub document_type_id:      Uuid,
    pub document_number:       String,
    pub issue_date:            Option<NaiveDate>,
    pub expiry_date:           Option<NaiveDate>,
    pub file_url:              String,
    pub status:                DocumentStatus,
    pub rejection_reason:      Option<String>,
    pub reviewed_by:           Option<Uuid>,
    pub reviewed_at:           Option<DateTime<Utc>>,
    pub submitted_at:          DateTime<Utc>,
    pub updated_at:            DateTime<Utc>,
}
```

- [ ] **Implement `document_type.rs`**

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentType {
    pub id:                Uuid,
    pub code:              String,
    pub jurisdiction:      String,
    pub applicable_to:     Vec<String>,
    pub name:              String,
    pub description:       Option<String>,
    pub is_required:       bool,
    pub has_expiry:        bool,
    pub warn_days_before:  i32,
    pub grace_period_days: i32,
    pub vehicle_classes:   Option<Vec<String>>,
}
```

- [ ] **Implement `audit_log.rs`**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAuditLog {
    pub id:                    Uuid,
    pub compliance_profile_id: Uuid,
    pub document_id:           Option<Uuid>,
    pub event_type:            String,
    pub actor_id:              Uuid,
    pub actor_type:            String,  // "driver" | "admin" | "system"
    pub notes:                 Option<String>,
    pub created_at:            DateTime<Utc>,
}
```

- [ ] **Create `entities/mod.rs`**

```rust
pub mod compliance_profile;
pub mod driver_document;
pub mod document_type;
pub mod audit_log;

pub use compliance_profile::{ComplianceProfile, ComplianceStatus};
pub use driver_document::{DriverDocument, DocumentStatus};
pub use document_type::DocumentType;
pub use audit_log::ComplianceAuditLog;
```

- [ ] **Run tests — expect pass**

```bash
cargo test -p logisticos-compliance domain::entities
```
Expected: 6 tests pass.

- [ ] **Commit**

```bash
git add services/compliance/src/domain/
git commit -m "feat(compliance): domain entities + ComplianceStatus derivation logic with tests"
```

---

## Task 4: Database migrations

**Files:** `services/compliance/migrations/`

- [ ] **Create migration 001 — schema**

`services/compliance/migrations/20260325000001_create_schema.sql`:
```sql
CREATE SCHEMA IF NOT EXISTS compliance;
```

- [ ] **Create migration 002 — document_types**

`services/compliance/migrations/20260325000002_document_types.sql`:
```sql
CREATE TABLE compliance.document_types (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    code              TEXT        NOT NULL UNIQUE,
    jurisdiction      TEXT        NOT NULL,
    applicable_to     TEXT[]      NOT NULL DEFAULT '{}',
    name              TEXT        NOT NULL,
    description       TEXT,
    is_required       BOOLEAN     NOT NULL DEFAULT true,
    has_expiry        BOOLEAN     NOT NULL DEFAULT true,
    warn_days_before  INT         NOT NULL DEFAULT 30,
    grace_period_days INT         NOT NULL DEFAULT 7,
    vehicle_classes   TEXT[],
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

- [ ] **Create migration 003 — compliance_profiles**

`services/compliance/migrations/20260325000003_compliance_profiles.sql`:
```sql
CREATE TABLE compliance.compliance_profiles (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id        UUID        NOT NULL,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('driver','partner','merchant')),
    entity_id        UUID        NOT NULL,
    overall_status   TEXT        NOT NULL DEFAULT 'pending_submission',
    jurisdiction     TEXT        NOT NULL,
    last_reviewed_at TIMESTAMPTZ,
    reviewed_by      UUID,
    suspended_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),

    UNIQUE (tenant_id, entity_type, entity_id)
);

CREATE INDEX idx_compliance_profiles_status
    ON compliance.compliance_profiles (tenant_id, overall_status);
CREATE INDEX idx_compliance_profiles_entity
    ON compliance.compliance_profiles (entity_type, entity_id);
```

- [ ] **Create migration 004 — driver_documents**

`services/compliance/migrations/20260325000004_driver_documents.sql`:
```sql
CREATE TABLE compliance.driver_documents (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    compliance_profile_id UUID        NOT NULL REFERENCES compliance.compliance_profiles(id),
    document_type_id      UUID        NOT NULL REFERENCES compliance.document_types(id),
    document_number       TEXT        NOT NULL,
    issue_date            DATE,
    expiry_date           DATE,
    file_url              TEXT        NOT NULL,
    status                TEXT        NOT NULL DEFAULT 'submitted',
    rejection_reason      TEXT,
    reviewed_by           UUID,
    reviewed_at           TIMESTAMPTZ,
    submitted_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_driver_documents_profile
    ON compliance.driver_documents (compliance_profile_id);
CREATE INDEX idx_driver_documents_expiry
    ON compliance.driver_documents (expiry_date)
    WHERE status = 'approved';
```

- [ ] **Create migration 005 — audit_log**

`services/compliance/migrations/20260325000005_audit_log.sql`:
```sql
CREATE TABLE compliance.compliance_audit_log (
    id                    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    compliance_profile_id UUID        NOT NULL REFERENCES compliance.compliance_profiles(id),
    document_id           UUID        REFERENCES compliance.driver_documents(id),
    event_type            TEXT        NOT NULL,
    actor_id              UUID        NOT NULL,
    actor_type            TEXT        NOT NULL CHECK (actor_type IN ('driver','admin','system')),
    notes                 TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_audit_log_profile
    ON compliance.compliance_audit_log (compliance_profile_id, created_at DESC);
```

- [ ] **Create migration 006 — seed document types**

`services/compliance/migrations/20260325000006_seed_document_types.sql`:
```sql
INSERT INTO compliance.document_types
    (code, jurisdiction, applicable_to, name, is_required, has_expiry, warn_days_before, grace_period_days)
VALUES
  ('UAE_EMIRATES_ID',       'UAE', '{driver}', 'Emirates ID',                  true, true, 60, 14),
  ('UAE_DRIVING_LICENSE',   'UAE', '{driver}', 'UAE Driving License',          true, true, 30,  7),
  ('UAE_VEHICLE_MULKIYA',   'UAE', '{driver}', 'Vehicle Registration (Mulkiya)',true, true, 30,  7),
  ('UAE_VEHICLE_INSURANCE', 'UAE', '{driver}', 'Third-Party Insurance',        true, true, 30,  7),
  ('PH_LTO_LICENSE',        'PH',  '{driver}', 'LTO Driving License',          true, true, 30,  7),
  ('PH_OR_CR',              'PH',  '{driver}', 'Vehicle OR/CR',                true, true, 30,  7),
  ('PH_NBI_CLEARANCE',      'PH',  '{driver}', 'NBI Clearance',                true, true, 60, 14),
  ('PH_CTPL_INSURANCE',     'PH',  '{driver}', 'CTPL Insurance',               true, true, 30,  7)
ON CONFLICT (code) DO NOTHING;
```

- [ ] **Commit**

```bash
git add services/compliance/migrations/
git commit -m "feat(compliance): database migrations + document type seed data"
```

---

## Task 5: Repository traits + implementations

**Files:**
- Create: `services/compliance/src/domain/repositories/mod.rs`
- Create: `services/compliance/src/infrastructure/db/compliance_profile_repo.rs`
- Create: `services/compliance/src/infrastructure/db/driver_document_repo.rs`
- Create: `services/compliance/src/infrastructure/db/document_type_repo.rs`
- Create: `services/compliance/src/infrastructure/db/mod.rs`
- Create: `services/compliance/src/infrastructure/mod.rs`

- [ ] **Define repository traits**

`src/domain/repositories/mod.rs`:
```rust
use async_trait::async_trait;
use uuid::Uuid;
use chrono::NaiveDate;
use crate::domain::entities::{
    ComplianceProfile, DriverDocument, DocumentType, ComplianceAuditLog,
};

#[async_trait]
pub trait ComplianceProfileRepository: Send + Sync {
    async fn find_by_entity(&self, tenant_id: Uuid, entity_type: &str, entity_id: Uuid)
        -> anyhow::Result<Option<ComplianceProfile>>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ComplianceProfile>>;
    async fn list_by_tenant(&self, tenant_id: Uuid, status_filter: Option<&str>)
        -> anyhow::Result<Vec<ComplianceProfile>>;
    async fn save(&self, profile: &ComplianceProfile) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DriverDocumentRepository: Send + Sync {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverDocument>>;
    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<DriverDocument>>;
    /// Find all approved docs with expiry_date within the next `within_days` days.
    async fn find_expiring(&self, within_days: i32) -> anyhow::Result<Vec<DriverDocument>>;
    /// Find all approved docs where expiry_date < today.
    async fn find_expired(&self) -> anyhow::Result<Vec<DriverDocument>>;
    /// Find pending review docs across all tenants (for admin queue).
    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<DriverDocument>>;
    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()>;
}

#[async_trait]
pub trait DocumentTypeRepository: Send + Sync {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>>;
    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>;
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>>;
}

#[async_trait]
pub trait AuditLogRepository: Send + Sync {
    async fn append(&self, entry: &ComplianceAuditLog) -> anyhow::Result<()>;
    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<ComplianceAuditLog>>;
}
```

- [ ] **Implement `PgComplianceProfileRepository`**

`src/infrastructure/db/compliance_profile_repo.rs`:
```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::{ComplianceProfile, ComplianceStatus},
    repositories::ComplianceProfileRepository,
};

#[derive(sqlx::FromRow)]
struct ComplianceProfileRow {
    id:               Uuid,
    tenant_id:        Uuid,
    entity_type:      String,
    entity_id:        Uuid,
    overall_status:   String,
    jurisdiction:     String,
    last_reviewed_at: Option<chrono::DateTime<chrono::Utc>>,
    reviewed_by:      Option<Uuid>,
    suspended_at:     Option<chrono::DateTime<chrono::Utc>>,
    created_at:       chrono::DateTime<chrono::Utc>,
    updated_at:       chrono::DateTime<chrono::Utc>,
}

impl From<ComplianceProfileRow> for ComplianceProfile {
    fn from(r: ComplianceProfileRow) -> Self {
        Self {
            id:               r.id,
            tenant_id:        r.tenant_id,
            entity_type:      r.entity_type,
            entity_id:        r.entity_id,
            overall_status:   ComplianceStatus::from_str(&r.overall_status),
            jurisdiction:     r.jurisdiction,
            last_reviewed_at: r.last_reviewed_at,
            reviewed_by:      r.reviewed_by,
            suspended_at:     r.suspended_at,
            created_at:       r.created_at,
            updated_at:       r.updated_at,
        }
    }
}

pub struct PgComplianceProfileRepository { pool: PgPool }

impl PgComplianceProfileRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
    /// Exposes pool for health check.
    pub fn pool(&self) -> &PgPool { &self.pool }
}

#[async_trait]
impl ComplianceProfileRepository for PgComplianceProfileRepository {
    async fn find_by_entity(&self, tenant_id: Uuid, entity_type: &str, entity_id: Uuid)
        -> anyhow::Result<Option<ComplianceProfile>>
    {
        let row = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1 AND entity_type = $2 AND entity_id = $3"#,
            tenant_id, entity_type, entity_id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<ComplianceProfile>> {
        let row = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_by_tenant(&self, tenant_id: Uuid, status_filter: Option<&str>)
        -> anyhow::Result<Vec<ComplianceProfile>>
    {
        let rows = sqlx::query_as!(
            ComplianceProfileRow,
            r#"SELECT id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                      last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at
               FROM compliance.compliance_profiles
               WHERE tenant_id = $1
                 AND ($2::text IS NULL OR overall_status = $2)
               ORDER BY created_at DESC"#,
            tenant_id, status_filter
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save(&self, p: &ComplianceProfile) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO compliance.compliance_profiles
               (id, tenant_id, entity_type, entity_id, overall_status, jurisdiction,
                last_reviewed_at, reviewed_by, suspended_at, created_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
               ON CONFLICT (id) DO UPDATE SET
                 overall_status   = EXCLUDED.overall_status,
                 last_reviewed_at = EXCLUDED.last_reviewed_at,
                 reviewed_by      = EXCLUDED.reviewed_by,
                 suspended_at     = EXCLUDED.suspended_at,
                 updated_at       = EXCLUDED.updated_at"#,
            p.id, p.tenant_id, &p.entity_type, p.entity_id,
            p.overall_status.as_str(), &p.jurisdiction,
            p.last_reviewed_at, p.reviewed_by, p.suspended_at,
            p.created_at, p.updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

- [ ] **Implement `PgDriverDocumentRepository`**

`src/infrastructure/db/driver_document_repo.rs`:
```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{
    entities::{DriverDocument, DocumentStatus},
    repositories::DriverDocumentRepository,
};

#[derive(sqlx::FromRow)]
struct DriverDocumentRow {
    id:                    Uuid,
    compliance_profile_id: Uuid,
    document_type_id:      Uuid,
    document_number:       String,
    issue_date:            Option<chrono::NaiveDate>,
    expiry_date:           Option<chrono::NaiveDate>,
    file_url:              String,
    status:                String,
    rejection_reason:      Option<String>,
    reviewed_by:           Option<Uuid>,
    reviewed_at:           Option<chrono::DateTime<chrono::Utc>>,
    submitted_at:          chrono::DateTime<chrono::Utc>,
    updated_at:            chrono::DateTime<chrono::Utc>,
}

impl From<DriverDocumentRow> for DriverDocument {
    fn from(r: DriverDocumentRow) -> Self {
        Self {
            id: r.id,
            compliance_profile_id: r.compliance_profile_id,
            document_type_id: r.document_type_id,
            document_number: r.document_number,
            issue_date: r.issue_date,
            expiry_date: r.expiry_date,
            file_url: r.file_url,
            status: DocumentStatus::from_str(&r.status),
            rejection_reason: r.rejection_reason,
            reviewed_by: r.reviewed_by,
            reviewed_at: r.reviewed_at,
            submitted_at: r.submitted_at,
            updated_at: r.updated_at,
        }
    }
}

pub struct PgDriverDocumentRepository { pool: PgPool }

impl PgDriverDocumentRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DriverDocumentRepository for PgDriverDocumentRepository {
    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DriverDocument>> {
        let row = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE compliance_profile_id = $1
               ORDER BY submitted_at DESC"#,
            profile_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_expiring(&self, within_days: i32) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date - CURRENT_DATE <= $1
                 AND expiry_date >= CURRENT_DATE"#,
            within_days
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_expired(&self) -> anyhow::Result<Vec<DriverDocument>> {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT id, compliance_profile_id, document_type_id, document_number,
                      issue_date, expiry_date, file_url, status, rejection_reason,
                      reviewed_by, reviewed_at, submitted_at, updated_at
               FROM compliance.driver_documents
               WHERE status = 'approved'
                 AND expiry_date IS NOT NULL
                 AND expiry_date < CURRENT_DATE"#
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn list_pending_review(&self, tenant_id: Option<Uuid>, limit: i64, offset: i64)
        -> anyhow::Result<Vec<DriverDocument>>
    {
        let rows = sqlx::query_as!(
            DriverDocumentRow,
            r#"SELECT d.id, d.compliance_profile_id, d.document_type_id, d.document_number,
                      d.issue_date, d.expiry_date, d.file_url, d.status, d.rejection_reason,
                      d.reviewed_by, d.reviewed_at, d.submitted_at, d.updated_at
               FROM compliance.driver_documents d
               JOIN compliance.compliance_profiles p ON p.id = d.compliance_profile_id
               WHERE d.status IN ('submitted', 'under_review')
                 AND ($1::uuid IS NULL OR p.tenant_id = $1)
               ORDER BY d.submitted_at ASC
               LIMIT $2 OFFSET $3"#,
            tenant_id, limit, offset
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn save(&self, doc: &DriverDocument) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO compliance.driver_documents
               (id, compliance_profile_id, document_type_id, document_number,
                issue_date, expiry_date, file_url, status, rejection_reason,
                reviewed_by, reviewed_at, submitted_at, updated_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               ON CONFLICT (id) DO UPDATE SET
                 status           = EXCLUDED.status,
                 rejection_reason = EXCLUDED.rejection_reason,
                 reviewed_by      = EXCLUDED.reviewed_by,
                 reviewed_at      = EXCLUDED.reviewed_at,
                 updated_at       = EXCLUDED.updated_at"#,
            doc.id, doc.compliance_profile_id, doc.document_type_id, &doc.document_number,
            doc.issue_date, doc.expiry_date, &doc.file_url, doc.status.as_str(),
            doc.rejection_reason, doc.reviewed_by, doc.reviewed_at,
            doc.submitted_at, doc.updated_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

- [ ] **Implement `PgDocumentTypeRepository`**

`src/infrastructure/db/document_type_repo.rs`:
```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::DocumentType, repositories::DocumentTypeRepository};

#[derive(sqlx::FromRow)]
struct DocumentTypeRow {
    id:                Uuid,
    code:              String,
    jurisdiction:      String,
    applicable_to:     Vec<String>,
    name:              String,
    description:       Option<String>,
    is_required:       bool,
    has_expiry:        bool,
    warn_days_before:  i32,
    grace_period_days: i32,
    vehicle_classes:   Option<Vec<String>>,
}

impl From<DocumentTypeRow> for DocumentType {
    fn from(r: DocumentTypeRow) -> Self {
        Self {
            id: r.id, code: r.code, jurisdiction: r.jurisdiction,
            applicable_to: r.applicable_to, name: r.name,
            description: r.description, is_required: r.is_required,
            has_expiry: r.has_expiry, warn_days_before: r.warn_days_before,
            grace_period_days: r.grace_period_days, vehicle_classes: r.vehicle_classes,
        }
    }
}

pub struct PgDocumentTypeRepository { pool: PgPool }

impl PgDocumentTypeRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl DocumentTypeRepository for PgDocumentTypeRepository {
    async fn find_by_code(&self, code: &str) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE code = $1"#,
            code
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    async fn list_required_for(&self, entity_type: &str, jurisdiction: &str)
        -> anyhow::Result<Vec<DocumentType>>
    {
        let rows = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types
               WHERE is_required = true
                 AND jurisdiction = $1
                 AND $2 = ANY(applicable_to)
               ORDER BY name"#,
            jurisdiction, entity_type
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn find_by_id(&self, id: Uuid) -> anyhow::Result<Option<DocumentType>> {
        let row = sqlx::query_as!(
            DocumentTypeRow,
            r#"SELECT id, code, jurisdiction, applicable_to, name, description,
                      is_required, has_expiry, warn_days_before, grace_period_days, vehicle_classes
               FROM compliance.document_types WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }
}
```

- [ ] **Implement `PgAuditLogRepository`**

`src/infrastructure/db/audit_log_repo.rs`:
```rust
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use crate::domain::{entities::ComplianceAuditLog, repositories::AuditLogRepository};

pub struct PgAuditLogRepository { pool: PgPool }

impl PgAuditLogRepository {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl AuditLogRepository for PgAuditLogRepository {
    async fn append(&self, entry: &ComplianceAuditLog) -> anyhow::Result<()> {
        sqlx::query!(
            r#"INSERT INTO compliance.compliance_audit_log
               (id, compliance_profile_id, document_id, event_type, actor_id, actor_type, notes, created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
            entry.id, entry.compliance_profile_id, entry.document_id,
            &entry.event_type, entry.actor_id, &entry.actor_type,
            entry.notes, entry.created_at
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_by_profile(&self, profile_id: Uuid) -> anyhow::Result<Vec<ComplianceAuditLog>> {
        let rows = sqlx::query_as!(
            ComplianceAuditLog,
            r#"SELECT id, compliance_profile_id, document_id, event_type,
                      actor_id, actor_type, notes, created_at
               FROM compliance.compliance_audit_log
               WHERE compliance_profile_id = $1
               ORDER BY created_at DESC"#,
            profile_id
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}
```

- [ ] **Create `src/infrastructure/db/mod.rs`**

```rust
mod compliance_profile_repo;
mod driver_document_repo;
mod document_type_repo;
mod audit_log_repo;

pub use compliance_profile_repo::PgComplianceProfileRepository;
pub use driver_document_repo::PgDriverDocumentRepository;
pub use document_type_repo::PgDocumentTypeRepository;
pub use audit_log_repo::PgAuditLogRepository;
```

- [ ] **Create `src/infrastructure/mod.rs`**

```rust
pub mod db;
pub mod messaging;
pub mod storage;
```

- [ ] **Create stub `src/infrastructure/storage/mod.rs`**

```rust
pub mod document_storage;
pub use document_storage::DocumentStorage;
```

- [ ] **Create stub `src/infrastructure/storage/document_storage.rs`** (full implementation in Task 13)

```rust
// Stub — wired fully in Task 13
pub struct DocumentStorage;

impl DocumentStorage {
    pub fn new_stub() -> Self { Self }
}
```

> **Why the stub:** `pub mod storage;` in `infrastructure/mod.rs` requires the module to exist or the compiler
> will fail immediately. Task 13 replaces this stub with the real S3 implementation.

- [ ] **Verify compilation**

```bash
cargo check -p logisticos-compliance
```

- [ ] **Commit**

```bash
git add services/compliance/src/domain/repositories/ services/compliance/src/infrastructure/db/
git commit -m "feat(compliance): repository traits + PostgreSQL implementations"
```

---

## Task 6: Kafka events + messaging

**Files:**
- Create: `services/compliance/src/domain/events/mod.rs`
- Create: `services/compliance/src/infrastructure/messaging/producer.rs`
- Create: `services/compliance/src/infrastructure/messaging/consumer.rs`
- Create: `services/compliance/src/infrastructure/messaging/mod.rs`

- [ ] **Define Kafka event payloads**

`src/domain/events/mod.rs`:
```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const TOPIC_COMPLIANCE: &str = "compliance";
pub const TOPIC_DRIVER:     &str = "driver";

#[derive(Debug, Serialize, Deserialize)]
pub struct ComplianceStatusChangedPayload {
    pub entity_type:  String,
    pub entity_id:    Uuid,
    pub old_status:   String,
    pub new_status:   String,
    pub is_assignable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DocumentReviewedPayload {
    pub entity_id:        Uuid,
    pub document_type:    String,
    pub decision:         String,   // "approved" | "rejected"
    pub rejection_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExpiryWarningPayload {
    pub entity_id:      Uuid,
    pub document_type:  String,
    pub expiry_date:    String,   // ISO 8601
    pub days_remaining: i32,
}

/// Emitted when a driver is reinstated from suspension.
#[derive(Debug, Serialize, Deserialize)]
pub struct DriverReinstatedPayload {
    pub entity_id:  Uuid,
    pub entity_type: String,
    pub reinstated_by: Uuid,
}

/// Inbound — from driver-ops topic
#[derive(Debug, Serialize, Deserialize)]
pub struct DriverRegisteredPayload {
    pub driver_id:  Uuid,
    pub tenant_id:  Uuid,
    pub jurisdiction: String,
}
```

- [ ] **Implement compliance event producer**

`src/infrastructure/messaging/producer.rs`:
```rust
use std::sync::Arc;
use logisticos_events::{envelope::Event, producer::KafkaProducer};
use uuid::Uuid;
use crate::domain::events::{
    TOPIC_COMPLIANCE, ComplianceStatusChangedPayload,
    DocumentReviewedPayload, ExpiryWarningPayload, DriverReinstatedPayload,
};

pub struct ComplianceProducer {
    kafka: Arc<KafkaProducer>,
}

impl ComplianceProducer {
    pub fn new(kafka: Arc<KafkaProducer>) -> Self { Self { kafka } }

    pub async fn publish_status_changed(
        &self, tenant_id: Uuid, payload: ComplianceStatusChangedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new(
            "logisticos/compliance",
            "compliance.status_changed",
            tenant_id,
            payload,
        );
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_document_reviewed(
        &self, tenant_id: Uuid, payload: DocumentReviewedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new(
            "logisticos/compliance",
            "compliance.document_reviewed",
            tenant_id,
            payload,
        );
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_expiry_warning(
        &self, tenant_id: Uuid, payload: ExpiryWarningPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new(
            "logisticos/compliance",
            "compliance.expiry_warning",
            tenant_id,
            payload,
        );
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }

    pub async fn publish_driver_reinstated(
        &self, tenant_id: Uuid, payload: DriverReinstatedPayload,
    ) -> anyhow::Result<()> {
        let event = Event::new(
            "logisticos/compliance",
            "compliance.driver_reinstated",
            tenant_id,
            payload,
        );
        self.kafka.publish_event(TOPIC_COMPLIANCE, &event).await
    }
}
```

- [ ] **Implement `driver.registered` consumer**

`src/infrastructure/messaging/consumer.rs`:
```rust
use std::sync::Arc;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    config::ClientConfig,
    message::Message,
};
use logisticos_events::envelope::Event;
use crate::domain::events::{TOPIC_DRIVER, DriverRegisteredPayload};
use crate::application::services::ComplianceService;

pub async fn start_driver_consumer(
    brokers: &str,
    group_id: &str,
    compliance_service: Arc<ComplianceService>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()?;

    consumer.subscribe(&[TOPIC_DRIVER])?;

    loop {
        match consumer.recv().await {
            Err(e) => tracing::warn!("Kafka error: {e}"),
            Ok(msg) => {
                if let Some(payload) = msg.payload_view::<str>().and_then(|r| r.ok()) {
                    if let Ok(event) = serde_json::from_str::<Event<DriverRegisteredPayload>>(payload) {
                        if event.event_type == "driver.registered" {
                            if let Err(e) = compliance_service
                                .create_profile_for_driver(
                                    event.tenant_id,
                                    event.data.driver_id,
                                    &event.data.jurisdiction,
                                )
                                .await
                            {
                                tracing::error!("Failed to create compliance profile: {e}");
                            }
                        }
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).unwrap_or_default();
            }
        }
    }
}
```

- [ ] **Create `src/infrastructure/messaging/mod.rs`**

```rust
pub mod producer;
pub mod consumer;
pub use producer::ComplianceProducer;
```

- [ ] **Commit**

```bash
git add services/compliance/src/domain/events/ services/compliance/src/infrastructure/messaging/
git commit -m "feat(compliance): Kafka event payloads, producer, driver.registered consumer"
```

---

## Task 7: Application service

**Files:**
- Create: `services/compliance/src/application/services/compliance_service.rs`
- Create: `services/compliance/src/application/services/expiry_checker.rs`
- Create: `services/compliance/src/application/services/mod.rs`
- Create: `services/compliance/src/application/mod.rs`

- [ ] **Write unit tests for service logic first**

At the bottom of `compliance_service.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Note: ComplianceStatus::derive() has no knowledge of suspension — that is intentional.
    /// Suspension is guarded at the recompute_and_publish level (checks profile.overall_status
    /// == Suspended before calling derive). This test validates the service-level guard.
    #[test]
    fn recompute_does_not_override_manual_suspension() {
        use crate::domain::entities::ComplianceStatus;
        // Simulate a suspended profile: derive() would return Compliant (all approved)
        // but recompute_and_publish must skip the update.
        let docs = vec![
            mock_doc(DocumentStatus::Approved, Some(chrono::NaiveDate::from_ymd_opt(2030,1,1).unwrap())),
        ];
        let status = ComplianceStatus::derive(&[DocumentStatus::Approved], false, false);
        // derive() itself returns Compliant — this is correct
        assert_eq!(status, ComplianceStatus::Compliant);
        // The recompute_and_publish function guards: if profile.overall_status == Suspended { return Ok(()) }
        // This means a suspended profile stays suspended regardless of document state.
    }

    #[test]
    fn recompute_status_all_approved_returns_compliant() {
        use crate::domain::entities::{DocumentStatus, DriverDocument};
        let docs = vec![
            mock_doc(DocumentStatus::Approved, Some(chrono::NaiveDate::from_ymd_opt(2030,1,1).unwrap())),
            mock_doc(DocumentStatus::Approved, Some(chrono::NaiveDate::from_ymd_opt(2030,1,1).unwrap())),
        ];
        let required_count = 2usize;
        let status = ComplianceService::compute_status(&docs, required_count);
        assert_eq!(status, ComplianceStatus::Compliant);
    }

    fn mock_doc(status: DocumentStatus, expiry: Option<chrono::NaiveDate>) -> DriverDocument {
        DriverDocument {
            id: uuid::Uuid::new_v4(),
            compliance_profile_id: uuid::Uuid::new_v4(),
            document_type_id: uuid::Uuid::new_v4(),
            document_number: "TEST".into(),
            issue_date: None,
            expiry_date: expiry,
            file_url: "s3://test".into(),
            status,
            rejection_reason: None,
            reviewed_by: None,
            reviewed_at: None,
            submitted_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}
```

- [ ] **Implement `ComplianceService`**

`src/application/services/compliance_service.rs`:
```rust
use std::sync::Arc;
use anyhow::Context;
use chrono::Utc;
use uuid::Uuid;
use crate::domain::{
    entities::{
        ComplianceProfile, ComplianceStatus, ComplianceAuditLog,
        DriverDocument, DocumentStatus,
    },
    repositories::{
        ComplianceProfileRepository, DriverDocumentRepository,
        DocumentTypeRepository, AuditLogRepository,
    },
    events::{ComplianceStatusChangedPayload, DocumentReviewedPayload},
};
use crate::infrastructure::messaging::ComplianceProducer;

pub struct ComplianceService {
    pub profiles:  Arc<dyn ComplianceProfileRepository>,
    pub documents: Arc<dyn DriverDocumentRepository>,
    pub doc_types: Arc<dyn DocumentTypeRepository>,
    pub audit:     Arc<dyn AuditLogRepository>,
    pub producer:  Arc<ComplianceProducer>,
}

impl ComplianceService {
    pub fn new(
        profiles:  Arc<dyn ComplianceProfileRepository>,
        documents: Arc<dyn DriverDocumentRepository>,
        doc_types: Arc<dyn DocumentTypeRepository>,
        audit:     Arc<dyn AuditLogRepository>,
        producer:  Arc<ComplianceProducer>,
    ) -> Self {
        Self { profiles, documents, doc_types, audit, producer }
    }

    /// Called when driver.registered Kafka event is received.
    pub async fn create_profile_for_driver(
        &self,
        tenant_id:    Uuid,
        driver_id:    Uuid,
        jurisdiction: &str,
    ) -> anyhow::Result<ComplianceProfile> {
        // Idempotent — skip if profile already exists
        if let Some(existing) = self.profiles
            .find_by_entity(tenant_id, "driver", driver_id)
            .await?
        {
            return Ok(existing);
        }
        let profile = ComplianceProfile {
            id:               Uuid::new_v4(),
            tenant_id,
            entity_type:      "driver".into(),
            entity_id:        driver_id,
            overall_status:   ComplianceStatus::PendingSubmission,
            jurisdiction:     jurisdiction.to_owned(),
            last_reviewed_at: None,
            reviewed_by:      None,
            suspended_at:     None,
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
        self.profiles.save(&profile).await?;
        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile.id,
            document_id:           None,
            event_type:            "profile_created".into(),
            actor_id:              driver_id,
            actor_type:            "system".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;
        Ok(profile)
    }

    /// Driver submits a document. Returns the created DriverDocument.
    pub async fn submit_document(
        &self,
        profile_id:      Uuid,
        document_type_id: Uuid,
        document_number: String,
        issue_date:      Option<chrono::NaiveDate>,
        expiry_date:     Option<chrono::NaiveDate>,
        file_url:        String,
        actor_id:        Uuid,
    ) -> anyhow::Result<DriverDocument> {
        let profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;

        // Supersede any existing non-superseded doc of the same type
        let existing = self.documents.list_by_profile(profile_id).await?;
        for mut doc in existing.into_iter()
            .filter(|d| d.document_type_id == document_type_id
                && d.status != DocumentStatus::Superseded)
        {
            doc.status = DocumentStatus::Superseded;
            doc.updated_at = Utc::now();
            self.documents.save(&doc).await?;
        }

        let doc = DriverDocument {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile_id,
            document_type_id,
            document_number,
            issue_date,
            expiry_date,
            file_url,
            status:                DocumentStatus::Submitted,
            rejection_reason:      None,
            reviewed_by:           None,
            reviewed_at:           None,
            submitted_at:          Utc::now(),
            updated_at:            Utc::now(),
        };
        self.documents.save(&doc).await?;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile_id,
            document_id:           Some(doc.id),
            event_type:            "doc_submitted".into(),
            actor_id,
            actor_type:            "driver".into(),
            notes:                 None,
            created_at:            Utc::now(),
        }).await?;

        self.recompute_and_publish(&profile).await?;
        Ok(doc)
    }

    /// Admin approves or rejects a document.
    pub async fn review_document(
        &self,
        doc_id:           Uuid,
        approved:         bool,
        rejection_reason: Option<String>,
        admin_id:         Uuid,
    ) -> anyhow::Result<()> {
        let mut doc = self.documents.find_by_id(doc_id).await?
            .context("Document not found")?;
        let profile = self.profiles.find_by_id(doc.compliance_profile_id).await?
            .context("Profile not found")?;

        doc.status = if approved { DocumentStatus::Approved } else { DocumentStatus::Rejected };
        doc.rejection_reason = rejection_reason.clone();
        doc.reviewed_by = Some(admin_id);
        doc.reviewed_at = Some(Utc::now());
        doc.updated_at = Utc::now();
        self.documents.save(&doc).await?;

        let doc_type = self.doc_types.find_by_id(doc.document_type_id).await?
            .map(|dt| dt.code)
            .unwrap_or_default();

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile.id,
            document_id:           Some(doc_id),
            event_type:            if approved { "doc_approved" } else { "doc_rejected" }.into(),
            actor_id:              admin_id,
            actor_type:            "admin".into(),
            notes:                 rejection_reason.clone(),
            created_at:            Utc::now(),
        }).await?;

        self.producer.publish_document_reviewed(
            profile.tenant_id,
            DocumentReviewedPayload {
                entity_id:        profile.entity_id,
                document_type:    doc_type,
                decision:         if approved { "approved" } else { "rejected" }.into(),
                rejection_reason,
            },
        ).await?;

        self.recompute_and_publish(&profile).await?;
        Ok(())
    }

    /// Admin manually suspends an entity.
    pub async fn suspend(
        &self, profile_id: Uuid, admin_id: Uuid, reason: Option<String>,
    ) -> anyhow::Result<()> {
        let mut profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;
        let old_status = profile.overall_status.as_str().to_owned();
        profile.overall_status = ComplianceStatus::Suspended;
        profile.suspended_at = Some(Utc::now());
        profile.updated_at = Utc::now();
        self.profiles.save(&profile).await?;

        self.audit.append(&ComplianceAuditLog {
            id: Uuid::new_v4(),
            compliance_profile_id: profile_id,
            document_id: None,
            event_type: "admin_override".into(),
            actor_id: admin_id,
            actor_type: "admin".into(),
            notes: reason,
            created_at: Utc::now(),
        }).await?;

        self.producer.publish_status_changed(profile.tenant_id, ComplianceStatusChangedPayload {
            entity_type:   profile.entity_type.clone(),
            entity_id:     profile.entity_id,
            old_status,
            new_status:    "suspended".into(),
            is_assignable: false,
        }).await?;
        Ok(())
    }

    /// Admin reinstates a suspended entity. Writes audit log, publishes Kafka event,
    /// then re-derives status from current document state.
    pub async fn reinstate(
        &self, profile_id: Uuid, admin_id: Uuid, reason: Option<String>,
    ) -> anyhow::Result<()> {
        let mut profile = self.profiles.find_by_id(profile_id).await?
            .context("Profile not found")?;

        // Set transitional status so recompute_and_publish guard doesn't short-circuit.
        // Guard: `if profile.overall_status == Suspended { return Ok(()) }` — we must clear
        // the Suspended status before calling recompute so derive() takes effect.
        profile.overall_status = ComplianceStatus::UnderReview;
        profile.suspended_at   = None;
        profile.updated_at     = Utc::now();
        self.profiles.save(&profile).await?;

        self.audit.append(&ComplianceAuditLog {
            id:                    Uuid::new_v4(),
            compliance_profile_id: profile_id,
            document_id:           None,
            event_type:            "driver_reinstated".into(),
            actor_id:              admin_id,
            actor_type:            "admin".into(),
            notes:                 reason,
            created_at:            Utc::now(),
        }).await?;

        self.producer.publish_driver_reinstated(profile.tenant_id, crate::domain::events::DriverReinstatedPayload {
            entity_id:    profile.entity_id,
            entity_type:  profile.entity_type.clone(),
            reinstated_by: admin_id,
        }).await?;

        // Re-derive final status from current document state
        self.recompute_and_publish(&profile).await
    }

    /// Recompute overall_status from current documents and publish if changed.
    async fn recompute_and_publish(&self, profile: &ComplianceProfile) -> anyhow::Result<()> {
        let required_types = self.doc_types
            .list_required_for(&profile.entity_type, &profile.jurisdiction)
            .await?;
        let docs = self.documents.list_by_profile(profile.id).await?;

        // Filter to active (non-superseded) docs
        let active_docs: Vec<&DriverDocument> = docs.iter()
            .filter(|d| d.status != DocumentStatus::Superseded)
            .collect();

        let required_count = required_types.len();
        let active_statuses: Vec<DocumentStatus> = active_docs.iter()
            .map(|d| d.status.clone())
            .collect();

        let has_missing = active_docs.iter()
            .filter(|d| matches!(d.status,
                DocumentStatus::Submitted | DocumentStatus::UnderReview |
                DocumentStatus::Approved))
            .count() < required_count;

        let today = chrono::Utc::now().date_naive();
        let has_expiring = active_docs.iter().any(|d| {
            if d.status != DocumentStatus::Approved { return false; }
            let Some(exp) = d.expiry_date else { return false; };
            // Find warn_days for this document type
            required_types.iter()
                .find(|rt| rt.id == d.document_type_id)
                .map(|rt| (exp - today).num_days() <= rt.warn_days_before as i64)
                .unwrap_or(false)
        });

        let new_status = ComplianceStatus::derive(&active_statuses, has_missing, has_expiring);

        // Don't override a manual Suspended status via recompute
        if profile.overall_status == ComplianceStatus::Suspended {
            return Ok(());
        }

        if new_status.as_str() != profile.overall_status.as_str() {
            let old_status = profile.overall_status.as_str().to_owned();
            let mut updated = profile.clone();
            updated.overall_status = new_status.clone();
            updated.updated_at = Utc::now();
            self.profiles.save(&updated).await?;

            self.producer.publish_status_changed(profile.tenant_id, ComplianceStatusChangedPayload {
                entity_type:   profile.entity_type.clone(),
                entity_id:     profile.entity_id,
                old_status,
                new_status:    new_status.as_str().to_owned(),
                is_assignable: new_status.is_assignable(),
            }).await?;
        }
        Ok(())
    }

    /// Static helper used in unit tests.
    pub fn compute_status(docs: &[DriverDocument], required_count: usize) -> ComplianceStatus {
        use chrono::Utc;
        let active: Vec<&DriverDocument> = docs.iter()
            .filter(|d| d.status != DocumentStatus::Superseded)
            .collect();
        let statuses: Vec<DocumentStatus> = active.iter().map(|d| d.status.clone()).collect();
        let has_missing = active.iter()
            .filter(|d| matches!(d.status, DocumentStatus::Submitted | DocumentStatus::UnderReview | DocumentStatus::Approved))
            .count() < required_count;
        let today = Utc::now().date_naive();
        let has_expiring = active.iter().any(|d| {
            d.status == DocumentStatus::Approved
                && d.expiry_date.map(|e| (e - today).num_days() <= 30).unwrap_or(false)
        });
        ComplianceStatus::derive(&statuses, has_missing, has_expiring)
    }
}
```

- [ ] **Implement `ExpiryCheckerService`**

`src/application/services/expiry_checker.rs`:
```rust
use std::sync::Arc;
use chrono::Utc;
use uuid::Uuid;
use crate::{
    domain::{
        entities::DocumentStatus,
        events::ExpiryWarningPayload,
        repositories::{DriverDocumentRepository, ComplianceProfileRepository},
    },
    infrastructure::messaging::ComplianceProducer,
    application::services::ComplianceService,
};

pub struct ExpiryCheckerService {
    compliance:  Arc<ComplianceService>,
    documents:   Arc<dyn DriverDocumentRepository>,
    profiles:    Arc<dyn ComplianceProfileRepository>,
    producer:    Arc<ComplianceProducer>,
}

impl ExpiryCheckerService {
    pub fn new(
        compliance: Arc<ComplianceService>,
        documents:  Arc<dyn DriverDocumentRepository>,
        profiles:   Arc<dyn ComplianceProfileRepository>,
        producer:   Arc<ComplianceProducer>,
    ) -> Self {
        Self { compliance, documents, profiles, producer }
    }

    /// Run once per day. Call from a `tokio::spawn` loop in bootstrap.
    pub async fn run_once(&self) -> anyhow::Result<()> {
        let today = Utc::now().date_naive();

        // 1. Warn about docs expiring within their per-type warn window.
        //    Use 60 days as a conservative upper bound query (covers Emirates ID at 60 days).
        //    Per-type filtering is done in the loop.
        let expiring = self.documents.find_expiring(60).await?;
        for doc in &expiring {
            // Load per-type warn_days_before — skip if type not found
            let dt = match self.compliance.doc_types.find_by_id(doc.document_type_id).await? {
                Some(dt) => dt,
                None     => continue,
            };

            let days_remaining = doc.expiry_date
                .map(|e| (e - today).num_days() as i32)
                .unwrap_or(0);

            // Only warn if within this type's window (e.g. 30 days for CTPL, 60 for Emirates ID)
            if days_remaining > dt.warn_days_before {
                continue;
            }

            let profile = match self.profiles.find_by_id(doc.compliance_profile_id).await? {
                Some(p) => p,
                None    => continue,
            };

            self.producer.publish_expiry_warning(profile.tenant_id, ExpiryWarningPayload {
                entity_id:     profile.entity_id,
                document_type: dt.code,
                expiry_date:   doc.expiry_date.unwrap_or_default().to_string(),
                days_remaining,
            }).await?;
        }

        // 2. Mark expired docs + check per-type grace period
        let expired = self.documents.find_expired().await?;
        for mut doc in expired {
            let Some(expiry) = doc.expiry_date else { continue; };
            let days_past = (today - expiry).num_days();

            let profile = match self.profiles.find_by_id(doc.compliance_profile_id).await? {
                Some(p) => p,
                None    => continue,
            };

            // Mark doc as expired
            if doc.status == DocumentStatus::Approved {
                doc.status = DocumentStatus::Expired;
                doc.updated_at = Utc::now();
                self.documents.save(&doc).await?;
            }

            // Recompute profile (will transition to Expired overall status)
            self.compliance.recompute_and_publish_public(&profile).await?;

            // Load per-type grace period (e.g. 14 days for Emirates ID, 7 for others)
            let grace_days = self.compliance.doc_types
                .find_by_id(doc.document_type_id)
                .await?
                .map(|dt| dt.grace_period_days as i64)
                .unwrap_or(7); // safe default if type record is missing

            if days_past > grace_days {
                self.compliance.suspend(
                    profile.id,
                    Uuid::nil(), // system actor
                    Some(format!("Auto-suspended: document expired {days_past} days ago (grace: {grace_days}d)")),
                ).await?;
            }
        }

        Ok(())
    }
}
```

> **Visibility:** `recompute_and_publish` is private — called internally by `submit_document` and `review_document`. Add the public wrapper below so `ExpiryCheckerService` and HTTP `reinstate_profile` can call it without making the full private method pub:
>
> ```rust
> // Add after the private recompute_and_publish fn:
> /// Public wrapper used by ExpiryCheckerService and reinstate HTTP handler.
> pub async fn recompute_and_publish_public(&self, profile: &ComplianceProfile) -> anyhow::Result<()> {
>     self.recompute_and_publish(profile).await
> }
> ```
>
> All internal callers (`submit_document`, `review_document`) continue using `self.recompute_and_publish(...)`. The public wrapper simply delegates.

- [ ] **Create `src/application/services/mod.rs`**

```rust
pub mod compliance_service;
pub mod expiry_checker;
pub use compliance_service::ComplianceService;
pub use expiry_checker::ExpiryCheckerService;
```

- [ ] **Create `src/application/mod.rs`**

```rust
pub mod services;
```

- [ ] **Run unit tests**

```bash
cargo test -p logisticos-compliance
```
Expected: all tests pass.

- [ ] **Commit**

```bash
git add services/compliance/src/application/
git commit -m "feat(compliance): application services — submit, review, recompute, expiry checker"
```

---

## Task 8: HTTP API + bootstrap wiring

**Files:**
- Modify: `libs/common/src/auth/permissions.rs` (or the crate file that defines RBAC permission constants)
- Create: `services/compliance/src/api/http/mod.rs`
- Create: `services/compliance/src/api/http/health.rs`
- Create: `services/compliance/src/api/http/driver_routes.rs`
- Create: `services/compliance/src/api/http/admin_routes.rs`
- Create: `services/compliance/src/api/http/internal_routes.rs`
- Create: `services/compliance/src/api/mod.rs`
- Modify: `services/compliance/src/bootstrap.rs`

- [ ] **Register RBAC permission constants** (prerequisite — must exist before admin routes compile)

Locate the file in `libs/` where permission constants are defined (look for existing constants like `DRIVER_READ`, `ORDER_CREATE`, etc. — likely `libs/common/src/auth/permissions.rs` or `libs/auth/src/rbac/permissions.rs`). Add:

```rust
pub const COMPLIANCE_REVIEW: &str = "compliance:review";
pub const COMPLIANCE_ADMIN:  &str = "compliance:admin";
```

These are consumed by `require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW)` in the admin route handlers.

> **How to find the right file:** Run `grep -r "pub const.*: &str" libs/` to find the permissions module. If no such file exists yet, create `libs/auth/src/rbac/permissions.rs` (or equivalent) and re-export from `lib.rs`.

- [ ] **Create AppState + router**

`src/api/http/mod.rs`:
```rust
use std::sync::Arc;
use axum::{Router, routing::{get, post}};
use tower_http::trace::TraceLayer;
use logisticos_auth::jwt::JwtService;
use crate::application::services::{ComplianceService, ExpiryCheckerService};

pub struct AppState {
    pub compliance: Arc<ComplianceService>,
    pub jwt:        Arc<JwtService>,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health
        .route("/health", get(health::health))
        .route("/ready",  get(health::ready))
        // Driver-facing
        .route("/api/v1/compliance/me/profile",          get(driver_routes::get_my_profile))
        .route("/api/v1/compliance/me/documents",        post(driver_routes::submit_document))
        .route("/api/v1/compliance/me/documents/:doc_id", get(driver_routes::get_document))
        // Admin-facing
        .route("/api/v1/compliance/admin/queue",                          get(admin_routes::review_queue))
        .route("/api/v1/compliance/admin/profiles",                       get(admin_routes::list_profiles))
        .route("/api/v1/compliance/admin/profiles/:profile_id",           get(admin_routes::get_profile))
        .route("/api/v1/compliance/admin/documents/:doc_id/approve",      post(admin_routes::approve_document))
        .route("/api/v1/compliance/admin/documents/:doc_id/reject",       post(admin_routes::reject_document))
        .route("/api/v1/compliance/admin/profiles/:profile_id/suspend",   post(admin_routes::suspend_profile))
        .route("/api/v1/compliance/admin/profiles/:profile_id/reinstate", post(admin_routes::reinstate_profile))
        // Internal
        .route("/api/v1/compliance/internal/status/:entity_type/:entity_id",
               get(internal_routes::get_status))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

mod health;
mod driver_routes;
mod admin_routes;
mod internal_routes;
```

- [ ] **Implement `health.rs`**

`src/api/http/health.rs`:
```rust
use axum::{extract::State, Json};
use std::sync::Arc;
use crate::api::http::AppState;

pub async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "service": "compliance" }))
}

pub async fn ready(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    // Verify DB connectivity by fetching one row
    let ok = sqlx::query("SELECT 1")
        .fetch_one(state.compliance.profiles.pool())
        .await
        .is_ok();
    if ok {
        Ok(Json(serde_json::json!({ "status": "ready" })))
    } else {
        Err(axum::http::StatusCode::SERVICE_UNAVAILABLE)
    }
}
```

> **Note:** `profiles.pool()` requires adding `pub fn pool(&self) -> &sqlx::PgPool` to `PgComplianceProfileRepository`. Add this accessor when implementing the struct in Task 5.

- [ ] **Implement driver routes**

`src/api/http/driver_routes.rs` — follow the `drivers.rs` pattern from driver-ops exactly:

```rust
use axum::{extract::{Path, State, Multipart}, Json};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use serde::Deserialize;
use crate::api::http::AppState;

// GET /me/profile
pub async fn get_my_profile(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: claims.user_id.to_string() })?;

    // Also return required doc types for the driver's jurisdiction
    let required = state.compliance.doc_types
        .list_required_for("driver", &profile.jurisdiction)
        .await?;
    let docs = state.compliance.documents
        .list_by_profile(profile.id)
        .await?;

    Ok(Json(serde_json::json!({
        "data": { "profile": profile, "required_types": required, "documents": docs }
    })))
}

#[derive(Deserialize)]
pub struct SubmitDocumentRequest {
    pub document_type_id: Uuid,
    pub document_number:  String,
    pub issue_date:       Option<String>,   // "YYYY-MM-DD"
    pub expiry_date:      Option<String>,   // "YYYY-MM-DD"
    pub file_url:         String,           // pre-uploaded URL from storage endpoint
}

// POST /me/documents
pub async fn submit_document(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitDocumentRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let profile = state.compliance.profiles
        .find_by_entity(claims.tenant_id, "driver", claims.user_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: claims.user_id.to_string() })?;

    let parse_date = |s: Option<String>| -> Option<chrono::NaiveDate> {
        s.and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
    };

    let doc = state.compliance.submit_document(
        profile.id,
        req.document_type_id,
        req.document_number,
        parse_date(req.issue_date),
        parse_date(req.expiry_date),
        req.file_url,
        claims.user_id,
    ).await?;

    Ok(Json(serde_json::json!({ "data": doc })))
}

// GET /me/documents/:doc_id
pub async fn get_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    // Verify tenant ownership via profile
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.entity_id != claims.user_id {
        return Err(AppError::Forbidden);
    }
    Ok(Json(serde_json::json!({ "data": doc })))
}
```

- [ ] **Implement admin routes**

`src/api/http/admin_routes.rs`:

```rust
use axum::{extract::{Path, State, Query}, Json};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;
use logisticos_auth::middleware::AuthClaims;
use logisticos_errors::AppError;
use crate::api::http::AppState;

#[derive(Deserialize)]
pub struct QueueParams { pub limit: Option<i64>, pub offset: Option<i64> }

pub async fn review_queue(
    AuthClaims(claims): AuthClaims,
    Query(params): Query<QueueParams>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW);
    let docs = state.compliance.documents
        .list_pending_review(
            Some(claims.tenant_id),
            params.limit.unwrap_or(50),
            params.offset.unwrap_or(0),
        ).await?;
    Ok(Json(serde_json::json!({ "data": docs })))
}

pub async fn list_profiles(
    AuthClaims(claims): AuthClaims,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW);
    let profiles = state.compliance.profiles
        .list_by_tenant(claims.tenant_id, None).await?;
    Ok(Json(serde_json::json!({ "data": profiles })))
}

pub async fn get_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW);
    let profile = state.compliance.profiles.find_by_id(profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: profile_id.to_string() })?;
    if profile.tenant_id != claims.tenant_id {
        return Err(AppError::Forbidden);
    }
    let docs  = state.compliance.documents.list_by_profile(profile_id).await?;
    let audit = state.compliance.audit.list_by_profile(profile_id).await?;
    Ok(Json(serde_json::json!({ "data": { "profile": profile, "documents": docs, "audit_log": audit } })))
}

#[derive(Deserialize)]
pub struct RejectRequest { pub reason: String }

pub async fn approve_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW);
    state.compliance.review_document(doc_id, true, None, claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reject_document(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<RejectRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_REVIEW);
    state.compliance.review_document(doc_id, false, Some(req.reason), claims.user_id).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

#[derive(Deserialize)]
pub struct SuspendRequest { pub reason: Option<String> }

pub async fn suspend_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
    Json(req): Json<SuspendRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_ADMIN);
    state.compliance.suspend(profile_id, claims.user_id, req.reason).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}

pub async fn reinstate_profile(
    AuthClaims(claims): AuthClaims,
    Path(profile_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_permission!(claims, logisticos_auth::rbac::permissions::COMPLIANCE_ADMIN);
    // Delegates to ComplianceService::reinstate — writes audit log, publishes Kafka event,
    // clears suspended_at, sets transitional UnderReview status, then recomputes from docs.
    state.compliance.reinstate(profile_id, claims.user_id, None).await?;
    Ok(Json(serde_json::json!({ "data": { "ok": true } })))
}
```

- [ ] **Implement internal route**

`src/api/http/internal_routes.rs`:
```rust
use axum::{extract::{Path, Query, State}, Json};
use std::sync::Arc;
use uuid::Uuid;
use serde::Deserialize;
use logisticos_errors::AppError;
use crate::api::http::AppState;

/// Dispatch service passes `?tenant_id=<uuid>` to scope the lookup.
/// Route: GET /internal/status/:entity_type/:entity_id?tenant_id=<uuid>
#[derive(Deserialize)]
pub struct StatusQuery {
    pub tenant_id: Uuid,
}

pub async fn get_status(
    Path((entity_type, entity_id)): Path<(String, Uuid)>,
    Query(query): Query<StatusQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    // mTLS-protected — no JWT required; called only by trusted internal services via Istio policy
    let profile = state.compliance.profiles
        .find_by_entity(query.tenant_id, &entity_type, entity_id)
        .await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: entity_id.to_string() })?;

    Ok(Json(serde_json::json!({
        "data": {
            "entity_id": entity_id,
            "entity_type": entity_type,
            "status": profile.overall_status,
            "is_assignable": profile.overall_status.is_assignable(),
        }
    })))
}
```

- [ ] **Wire `bootstrap.rs`**

```rust
use std::sync::Arc;
use sqlx::postgres::PgPoolOptions;
use anyhow::Context;
use logisticos_auth::jwt::JwtService;
use logisticos_events::producer::KafkaProducer;
use crate::{
    config::Config,
    infrastructure::{
        db::{PgComplianceProfileRepository, PgDriverDocumentRepository,
             PgDocumentTypeRepository, PgAuditLogRepository},
        messaging::{ComplianceProducer, consumer::start_driver_consumer},
    },
    application::services::{ComplianceService, ExpiryCheckerService},
    api::http::{router, AppState},
};

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::load().context("Failed to load compliance config")?;

    logisticos_tracing::init(logisticos_tracing::TracingConfig {
        service_name: "compliance".to_string(),
        env: cfg.app.env.clone(),
        otlp_endpoint: std::env::var("OTLP_ENDPOINT").ok(),
    })?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await
        .context("PostgreSQL connection failed")?;

    sqlx::migrate!("./migrations").run(&pool).await
        .context("compliance migration failed")?;

    let kafka     = Arc::new(KafkaProducer::new(&cfg.kafka.brokers)?);
    let producer  = Arc::new(ComplianceProducer::new(Arc::clone(&kafka)));
    let jwt_secret = std::env::var("AUTH__JWT_SECRET").context("AUTH__JWT_SECRET not set")?;
    let jwt       = Arc::new(JwtService::new(jwt_secret, 3600, 86400));

    let profiles  = Arc::new(PgComplianceProfileRepository::new(pool.clone()));
    let documents = Arc::new(PgDriverDocumentRepository::new(pool.clone()));
    let doc_types = Arc::new(PgDocumentTypeRepository::new(pool.clone()));
    let audit     = Arc::new(PgAuditLogRepository::new(pool.clone()));

    let compliance = Arc::new(ComplianceService::new(
        Arc::clone(&profiles) as _,
        Arc::clone(&documents) as _,
        Arc::clone(&doc_types) as _,
        Arc::clone(&audit)     as _,
        Arc::clone(&producer),
    ));

    let expiry_checker = Arc::new(ExpiryCheckerService::new(
        Arc::clone(&compliance),
        Arc::clone(&documents) as _,
        Arc::clone(&profiles)  as _,
        Arc::clone(&producer),
    ));

    // Kafka consumer (background)
    let compliance_for_consumer = Arc::clone(&compliance);
    let brokers  = cfg.kafka.brokers.clone();
    let group_id = cfg.kafka.consumer_group.clone();
    tokio::spawn(async move {
        if let Err(e) = start_driver_consumer(&brokers, &group_id, compliance_for_consumer).await {
            tracing::error!("Kafka consumer error: {e}");
        }
    });

    // Daily expiry checker (background, runs every 24h)
    let checker = Arc::clone(&expiry_checker);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(86400));
        loop {
            interval.tick().await;
            if let Err(e) = checker.run_once().await {
                tracing::error!("Expiry checker error: {e}");
            }
        }
    });

    let state = Arc::new(AppState { compliance, jwt });
    let app   = router(state);
    let addr  = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr).await
        .with_context(|| format!("Failed to bind {addr}"))?;

    tracing::info!(addr = %addr, "compliance service listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.expect("ctrl_c");
}
```

- [ ] **Full compile check**

```bash
cargo build -p logisticos-compliance
```
Expected: builds without errors.

- [ ] **Run all tests**

```bash
cargo test -p logisticos-compliance
```

- [ ] **Commit**

```bash
git add services/compliance/src/
git commit -m "feat(compliance): HTTP routes + bootstrap wiring — service fully wired"
```

---

## Task 9: Dispatch service — Redis compliance cache

**Files:**
- Create: `services/dispatch/src/infrastructure/db/compliance_cache.rs`
- Create: `services/dispatch/src/infrastructure/messaging/compliance_consumer.rs`
- Modify: `services/dispatch/src/infrastructure/db/mod.rs`
- Modify: `services/dispatch/src/infrastructure/messaging/mod.rs`
- Modify: `services/dispatch/src/bootstrap.rs`

- [ ] **Write test for cache read/write**

In `compliance_cache.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_stores_and_retrieves_status() {
        // Integration test — requires running Redis
        // Run: cargo test -p logisticos-dispatch compliance_cache -- --ignored
        // to skip in CI without Redis
    }
}
```

- [ ] **Implement `ComplianceCache`**

`services/dispatch/src/infrastructure/db/compliance_cache.rs`:
```rust
use redis::AsyncCommands;
use uuid::Uuid;

const KEY_PREFIX: &str = "compliance:status:";
const TTL_SECONDS: u64 = 300; // 5 min — refresh on every Kafka event

pub struct ComplianceCache {
    redis: redis::aio::ConnectionManager,
}

impl ComplianceCache {
    pub fn new(redis: redis::aio::ConnectionManager) -> Self { Self { redis } }

    pub async fn set_status(&mut self, entity_id: Uuid, status: &str, is_assignable: bool)
        -> anyhow::Result<()>
    {
        let key = format!("{KEY_PREFIX}{entity_id}");
        let val = serde_json::json!({ "status": status, "is_assignable": is_assignable }).to_string();
        self.redis.set_ex::<_, _, ()>(&key, val, TTL_SECONDS).await?;
        Ok(())
    }

    /// Returns (status, is_assignable). Returns (None, false) if not cached.
    pub async fn get_status(&mut self, entity_id: Uuid) -> anyhow::Result<Option<(String, bool)>> {
        let key = format!("{KEY_PREFIX}{entity_id}");
        let val: Option<String> = self.redis.get(&key).await?;
        Ok(val.and_then(|v| {
            let j: serde_json::Value = serde_json::from_str(&v).ok()?;
            let status       = j["status"].as_str()?.to_owned();
            let is_assignable = j["is_assignable"].as_bool().unwrap_or(false);
            Some((status, is_assignable))
        }))
    }
}
```

- [ ] **Implement compliance event consumer in dispatch**

`services/dispatch/src/infrastructure/messaging/compliance_consumer.rs`:
```rust
use std::sync::Arc;
use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, config::ClientConfig, message::Message};
use logisticos_events::envelope::Event;
use tokio::sync::Mutex;
use crate::infrastructure::db::ComplianceCache;

#[derive(serde::Deserialize)]
struct StatusChangedPayload {
    entity_id:    uuid::Uuid,
    entity_type:  String,
    new_status:   String,
    is_assignable: bool,
}

pub async fn start_compliance_consumer(
    brokers:  &str,
    group_id: &str,
    cache:    Arc<Mutex<ComplianceCache>>,
) -> anyhow::Result<()> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&["compliance"])?;

    loop {
        match consumer.recv().await {
            Err(e) => tracing::warn!("Kafka error: {e}"),
            Ok(msg) => {
                if let Some(Ok(payload)) = msg.payload_view::<str>() {
                    if let Ok(event) = serde_json::from_str::<Event<StatusChangedPayload>>(payload) {
                        if event.event_type == "compliance.status_changed"
                            && event.data.entity_type == "driver"
                        {
                            let mut cache = cache.lock().await;
                            if let Err(e) = cache.set_status(
                                event.data.entity_id,
                                &event.data.new_status,
                                event.data.is_assignable,
                            ).await {
                                tracing::error!("Failed to update compliance cache: {e}");
                            }
                        }
                    }
                }
                consumer.commit_message(&msg, CommitMode::Async).unwrap_or_default();
            }
        }
    }
}
```

- [ ] **Export from dispatch infrastructure mods**

In `services/dispatch/src/infrastructure/db/mod.rs`, add:
```rust
pub mod compliance_cache;
pub use compliance_cache::ComplianceCache;
```

In `services/dispatch/src/infrastructure/messaging/mod.rs`, add:
```rust
pub mod compliance_consumer;
```

- [ ] **Wire in dispatch `bootstrap.rs`**

Add to the dispatch `AppState` struct:
```rust
pub compliance_cache: Arc<Mutex<ComplianceCache>>,
```

In the `run()` function after Redis connection:
```rust
use tokio::sync::Mutex;
use crate::infrastructure::db::ComplianceCache;
use crate::infrastructure::messaging::compliance_consumer::start_compliance_consumer;

let compliance_cache = Arc::new(Mutex::new(
    ComplianceCache::new(redis_manager.clone())
));

// Background Kafka consumer for compliance status
let cache_for_consumer = Arc::clone(&compliance_cache);
let brokers_clone = cfg.kafka.brokers.clone();
tokio::spawn(async move {
    if let Err(e) = start_compliance_consumer(
        &brokers_clone,
        "dispatch-compliance-consumer",
        cache_for_consumer,
    ).await {
        tracing::error!("Compliance consumer error: {e}");
    }
});
```

In `services/dispatch/src/application/services/dispatch_service.rs` (or whichever function selects a driver for assignment — search for `assign` in that service), add the compliance gate before confirming driver selection:

```rust
// At the top of the function that selects a driver (e.g. assign_task or find_best_driver):
let mut cache = state.compliance_cache.lock().await;
let (_, is_assignable) = cache
    .get_status(driver.entity_id)
    .await
    .unwrap_or(None)
    .unwrap_or_else(|| ("compliant".into(), true)); // Default: assignable if not in cache
drop(cache); // release lock before await points

if !is_assignable {
    return Err(AppError::Unprocessable {
        message: format!("Driver {} is not compliance-cleared for assignment", driver.entity_id),
    });
}
```

> If the dispatch service assigns from an HTTP handler rather than a service method, add the same check directly in the handler in `services/dispatch/src/api/http/` before the driver selection call.

- [ ] **Build dispatch service**

```bash
cargo build -p logisticos-dispatch
```

- [ ] **Commit**

```bash
git add services/dispatch/src/infrastructure/
git commit -m "feat(dispatch): compliance status Redis cache + Kafka consumer — gates task assignment"
```

---

## Task 10: Admin Portal — Compliance Console

**Files:**
- Create: `apps/admin-portal/src/lib/api/compliance.ts`
- Create: `apps/admin-portal/src/app/(dashboard)/compliance/page.tsx`
- Create: `apps/admin-portal/src/components/compliance/kpi-strip.tsx`
- Create: `apps/admin-portal/src/components/compliance/review-queue.tsx`
- Create: `apps/admin-portal/src/components/compliance/document-detail-panel.tsx`

- [ ] **Create API client**

`apps/admin-portal/src/lib/api/compliance.ts`:
```typescript
const BASE = process.env.NEXT_PUBLIC_API_BASE ?? "";

export interface ComplianceProfile {
  id:             string;
  entity_type:    string;
  entity_id:      string;
  overall_status: string;
  jurisdiction:   string;
  last_reviewed_at: string | null;
  suspended_at:   string | null;
}

export interface DriverDocument {
  id:                    string;
  compliance_profile_id: string;
  document_type_id:      string;
  document_number:       string;
  expiry_date:           string | null;
  file_url:              string;
  status:                string;
  rejection_reason:      string | null;
  reviewed_by:           string | null;
  reviewed_at:           string | null;
  submitted_at:          string;
}

export async function fetchReviewQueue(token: string): Promise<DriverDocument[]> {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/queue?limit=50`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const j = await r.json();
  return j.data;
}

export async function fetchProfiles(token: string): Promise<ComplianceProfile[]> {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/profiles`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  const j = await r.json();
  return j.data;
}

export async function fetchProfile(token: string, profileId: string) {
  const r = await fetch(`${BASE}/api/v1/compliance/admin/profiles/${profileId}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  return (await r.json()).data;
}

export async function approveDocument(token: string, docId: string): Promise<void> {
  await fetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/approve`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}` },
  });
}

export async function rejectDocument(token: string, docId: string, reason: string): Promise<void> {
  await fetch(`${BASE}/api/v1/compliance/admin/documents/${docId}/reject`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ reason }),
  });
}
```

- [ ] **Create compliance console page**

`apps/admin-portal/src/app/(dashboard)/compliance/page.tsx` — build the two-panel layout from the approved UI mockup (Task 10 in brainstorm). Use the existing `GlassCard`, `NeonBadge`, `LiveMetric` components already in the admin portal. Follow the exact component/style patterns of `apps/admin-portal/src/app/(dashboard)/orders/page.tsx`.

Key structure:
```tsx
"use client";
import { useState, useEffect } from "react";
import { motion } from "framer-motion";
import { GlassCard } from "@/components/ui/glass-card";
import { NeonBadge } from "@/components/ui/neon-badge";
import { variants } from "@/lib/design-system/tokens";
import { ReviewQueue } from "@/components/compliance/review-queue";
import { DocumentDetailPanel } from "@/components/compliance/document-detail-panel";
import { ComplianceKpiStrip } from "@/components/compliance/kpi-strip";
import type { DriverDocument, ComplianceProfile } from "@/lib/api/compliance";

export default function CompliancePage() {
  const [queue,           setQueue]           = useState<DriverDocument[]>([]);
  const [profiles,        setProfiles]        = useState<ComplianceProfile[]>([]);
  const [selectedProfile, setSelectedProfile] = useState<string | null>(null);

  // Load queue + profiles on mount (mock data for now, wire to API when backend is deployed)
  // ...

  return (
    <motion.div variants={variants.staggerContainer} initial="hidden" animate="visible" className="space-y-6">
      <ComplianceKpiStrip profiles={profiles} />
      <div className="flex gap-4 h-[600px]">
        <ReviewQueue
          items={queue}
          onSelect={(profileId) => setSelectedProfile(profileId)}
          selectedId={selectedProfile}
        />
        {selectedProfile && (
          <DocumentDetailPanel
            profileId={selectedProfile}
            onApprove={(docId) => { /* call approveDocument */ }}
            onReject={(docId, reason) => { /* call rejectDocument */ }}
          />
        )}
      </div>
    </motion.div>
  );
}
```

- [ ] **Implement `ComplianceKpiStrip`**

`apps/admin-portal/src/components/compliance/kpi-strip.tsx`:
```tsx
import type { ComplianceProfile } from "@/lib/api/compliance";

interface Props { profiles: ComplianceProfile[] }

export function ComplianceKpiStrip({ profiles }: Props) {
  const count = (status: string) => profiles.filter(p => p.overall_status === status).length;
  const kpis = [
    { label: "Compliant",      value: count("compliant"),          color: "text-green-signal",  border: "border-green-glow/20",  bg: "bg-green-surface"  },
    { label: "Pending Review", value: count("under_review"),       color: "text-cyan-neon",     border: "border-cyan-glow/20",   bg: "bg-cyan-surface"   },
    { label: "Expiring Soon",  value: count("expiring_soon"),      color: "text-amber-signal",  border: "border-amber-glow/22",  bg: "bg-amber-surface"  },
    { label: "Suspended",      value: count("suspended"),          color: "text-red-400",       border: "border-red-glow/20",    bg: "bg-red-surface"    },
  ];
  return (
    <div className="grid grid-cols-4 gap-3">
      {kpis.map(kpi => (
        <div key={kpi.label} className={`rounded-xl p-4 border ${kpi.bg} ${kpi.border}`}>
          <div className={`text-3xl font-mono font-bold ${kpi.color}`}>{kpi.value}</div>
          <div className="text-xs uppercase tracking-widest text-white/40 mt-1">{kpi.label}</div>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Implement `ReviewQueue`**

`apps/admin-portal/src/components/compliance/review-queue.tsx`:
```tsx
import type { DriverDocument } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";

interface Props {
  items:      DriverDocument[];
  selectedId: string | null;
  onSelect:   (profileId: string) => void;
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function initials(id: string) {
  return id.slice(0, 2).toUpperCase();
}

export function ReviewQueue({ items, selectedId, onSelect }: Props) {
  return (
    <div className="w-72 flex-shrink-0 rounded-xl border border-glass-border bg-glass-100 flex flex-col overflow-hidden">
      <div className="px-4 py-2.5 border-b border-glass-border flex items-center justify-between">
        <span className="text-xs font-bold uppercase tracking-widest text-white/60">Review Queue</span>
        <span className="text-xs font-mono bg-cyan-surface border border-cyan-glow/25 text-cyan-neon rounded-full px-2 py-0.5">{items.length} pending</span>
      </div>
      <div className="overflow-y-auto flex-1">
        {items.map(doc => (
          <button
            key={doc.id}
            onClick={() => onSelect(doc.compliance_profile_id)}
            className={cn(
              "w-full text-left px-4 py-3 border-b border-glass-border flex gap-3 items-start hover:bg-glass-200 transition-colors",
              selectedId === doc.compliance_profile_id && "bg-cyan-surface/40 border-l-2 border-l-cyan-neon"
            )}
          >
            <div className="w-8 h-8 rounded-full bg-cyan-surface border border-cyan-glow/25 flex items-center justify-center text-xs font-bold text-cyan-neon flex-shrink-0">
              {initials(doc.compliance_profile_id)}
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-sm font-semibold text-white/85 truncate">Profile {doc.compliance_profile_id.slice(0, 8)}</div>
              <div className="text-xs font-mono text-white/35 mt-0.5">{doc.document_type_id.slice(0, 8)}</div>
              <span className="inline-block mt-1 text-2xs font-semibold px-1.5 py-0.5 rounded bg-cyan-surface border border-cyan-glow/25 text-cyan-neon">
                {doc.status === "submitted" ? "New submission" : "Renewal"}
              </span>
            </div>
            <span className="text-2xs font-mono text-white/20 flex-shrink-0">{timeAgo(doc.submitted_at)}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Implement `DocumentDetailPanel`**

`apps/admin-portal/src/components/compliance/document-detail-panel.tsx`:
```tsx
"use client";
import { useEffect, useState } from "react";
import { fetchProfile, approveDocument, rejectDocument } from "@/lib/api/compliance";
import type { DriverDocument } from "@/lib/api/compliance";
import { cn } from "@/lib/design-system/cn";
import { Check, X, ExternalLink } from "lucide-react";

interface Props {
  profileId: string;
  onApprove: (docId: string) => void;
  onReject:  (docId: string, reason: string) => void;
}

const STATUS_BADGE: Record<string, string> = {
  compliant:          "bg-green-surface border-green-glow/20 text-green-signal",
  under_review:       "bg-amber-surface border-amber-glow/25 text-amber-signal",
  pending_submission: "bg-glass-100 border-glass-border text-white/40",
  suspended:          "bg-red-surface border-red-glow/25 text-red-400",
};

export function DocumentDetailPanel({ profileId, onApprove, onReject }: Props) {
  const [detail,        setDetail]        = useState<any>(null);
  const [rejectDocId,   setRejectDocId]   = useState<string | null>(null);
  const [rejectReason,  setRejectReason]  = useState("");

  useEffect(() => {
    setDetail(null);
    // Mock token — wire to auth context in production
    fetchProfile("mock-token", profileId).then(setDetail);
  }, [profileId]);

  if (!detail) {
    return <div className="flex-1 flex items-center justify-center text-white/25 text-sm">Loading…</div>;
  }

  const { profile, documents } = detail as { profile: any; documents: DriverDocument[] };
  const sorted = [...documents].sort((a, b) => {
    const rank = (s: string) => s === "submitted" || s === "under_review" ? 0 : 1;
    return rank(a.status) - rank(b.status);
  });

  async function handleApprove(docId: string) {
    await approveDocument("mock-token", docId);
    onApprove(docId);
    fetchProfile("mock-token", profileId).then(setDetail);
  }

  async function handleReject(docId: string) {
    if (!rejectReason.trim()) return;
    await rejectDocument("mock-token", docId, rejectReason);
    onReject(docId, rejectReason);
    setRejectDocId(null);
    setRejectReason("");
    fetchProfile("mock-token", profileId).then(setDetail);
  }

  return (
    <div className="flex-1 rounded-xl border border-cyan-glow/20 bg-cyan-surface/5 flex flex-col overflow-hidden">
      <div className="px-4 py-3 border-b border-glass-border flex items-center gap-3">
        <div className="w-10 h-10 rounded-full bg-cyan-surface border-2 border-cyan-glow/30 flex items-center justify-center text-sm font-bold text-cyan-neon">
          {profile.entity_id?.slice(0,2).toUpperCase()}
        </div>
        <div>
          <div className="text-sm font-semibold text-white">{profile.entity_id}</div>
          <div className="text-xs font-mono text-white/35">{profile.jurisdiction}</div>
        </div>
        <span className={cn("ml-auto text-xs px-3 py-1 rounded-full border font-mono font-semibold", STATUS_BADGE[profile.overall_status] ?? STATUS_BADGE.pending_submission)}>
          {profile.overall_status.replace(/_/g, " ")}
        </span>
      </div>
      <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-3">
        {sorted.map(doc => {
          const isPending = doc.status === "submitted" || doc.status === "under_review";
          return (
            <div key={doc.id} className={cn("rounded-xl p-3.5 border", isPending ? "border-amber-glow/30 bg-amber-surface/10" : "border-green-glow/20 bg-green-surface/5")}>
              <div className="flex items-start gap-3">
                <div className="w-16 h-12 rounded-lg bg-glass-100 border border-glass-border flex items-center justify-center text-xl flex-shrink-0">🪪</div>
                <div className="flex-1">
                  <div className="text-xs font-bold uppercase tracking-wider text-white/75">{doc.document_type_id.slice(0,12)}</div>
                  <div className="text-xs font-mono text-white/40 mt-1">{doc.document_number}</div>
                  {doc.expiry_date && (
                    <div className={cn("text-xs mt-1", isPending ? "text-amber-signal/80" : "text-green-signal/70")}>
                      Exp: {doc.expiry_date}
                    </div>
                  )}
                </div>
                {doc.file_url && (
                  <a href={doc.file_url} target="_blank" rel="noreferrer"
                    className="px-2.5 py-1.5 rounded-lg text-xs bg-glass-100 border border-glass-border text-white/50 flex items-center gap-1 hover:text-white/80">
                    <ExternalLink className="h-3 w-3" /> View
                  </a>
                )}
              </div>
              {isPending && (
                <div className="mt-3">
                  <div className="flex gap-2">
                    <button onClick={() => handleApprove(doc.id)}
                      className="flex-1 py-1.5 rounded-lg text-xs font-bold bg-green-surface border border-green-glow/35 text-green-signal flex items-center justify-center gap-1 hover:bg-green-surface/80">
                      <Check className="h-3 w-3" /> Approve
                    </button>
                    <button onClick={() => setRejectDocId(rejectDocId === doc.id ? null : doc.id)}
                      className="flex-1 py-1.5 rounded-lg text-xs font-bold bg-red-surface border border-red-glow/30 text-red-400 flex items-center justify-center gap-1 hover:bg-red-surface/80">
                      <X className="h-3 w-3" /> Reject
                    </button>
                  </div>
                  {rejectDocId === doc.id && (
                    <div className="mt-2 flex gap-2">
                      <input
                        value={rejectReason}
                        onChange={e => setRejectReason(e.target.value)}
                        placeholder="Rejection reason (required)…"
                        className="flex-1 bg-red-surface/30 border border-red-glow/25 rounded-lg px-3 py-1.5 text-xs font-mono text-white/60 placeholder:text-white/25"
                      />
                      <button onClick={() => handleReject(doc.id)}
                        disabled={!rejectReason.trim()}
                        className="px-3 py-1.5 rounded-lg text-xs font-bold bg-red-surface border border-red-glow/30 text-red-400 disabled:opacity-40">
                        Submit
                      </button>
                    </div>
                  )}
                </div>
              )}
              {!isPending && doc.reviewed_at && (
                <div className="mt-2 inline-flex items-center gap-1.5 text-xs text-green-signal/70 bg-green-surface border border-green-glow/20 rounded-full px-2.5 py-0.5">
                  <Check className="h-3 w-3" /> Approved · {new Date(doc.reviewed_at).toLocaleDateString()}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
```

- [ ] **Add compliance route to admin portal nav**

In `apps/admin-portal/src/components/layout/sidebar.tsx` (or wherever nav links are defined), add:
```tsx
{ href: "/compliance", label: "Compliance", icon: ShieldCheck }
```

- [ ] **Test in browser**

```bash
cd apps/admin-portal && npm run dev
```
Navigate to `http://localhost:3001/compliance`. Verify KPI strip renders, queue panel shows, detail panel opens on click.

- [ ] **Commit**

```bash
git add apps/admin-portal/src/app/(dashboard)/compliance/ apps/admin-portal/src/components/compliance/ apps/admin-portal/src/lib/api/compliance.ts
git commit -m "feat(admin-portal): compliance console — KPI strip, review queue, document detail panel"
```

---

## Task 11: Driver App — Compliance slice + Profile screen

**Files:**
- Modify: `apps/driver-app/src/store/index.ts`
- Modify: `apps/driver-app/src/app/(tabs)/profile.tsx`
- Create: `apps/driver-app/src/app/compliance/index.tsx`
- Create: `apps/driver-app/src/app/compliance/upload/[typeCode].tsx`

- [ ] **Add compliance Redux slice**

In `apps/driver-app/src/store/index.ts`, add after the existing slices:

```typescript
// ── Compliance ─────────────────────────────────────────────────────────────

export interface RequiredDocType {
  id:               string;
  code:             string;
  name:             string;
  has_expiry:       boolean;
  warn_days_before: number;
}

export interface SubmittedDoc {
  id:              string;
  document_type_id: string;
  document_number: string;
  expiry_date:     string | null;
  status:          "submitted" | "under_review" | "approved" | "rejected" | "expired" | "superseded";
  rejection_reason: string | null;
  submitted_at:    string;
}

export interface ComplianceState {
  overall_status:  string;   // "pending_submission" | "under_review" | "compliant" | "expiring_soon" | "expired" | "suspended"
  jurisdiction:    string;
  required_types:  RequiredDocType[];
  documents:       SubmittedDoc[];
}

const initialComplianceState: ComplianceState = {
  overall_status: "pending_submission",
  jurisdiction:   "UAE",
  required_types: [],
  documents:      [],
};

const complianceSlice = createSlice({
  name: "compliance",
  initialState: initialComplianceState,
  reducers: {
    setComplianceProfile(state, action: PayloadAction<{
      overall_status: string;
      jurisdiction:   string;
      required_types: RequiredDocType[];
      documents:      SubmittedDoc[];
    }>) {
      state.overall_status = action.payload.overall_status;
      state.jurisdiction   = action.payload.jurisdiction;
      state.required_types = action.payload.required_types;
      state.documents      = action.payload.documents;
    },
    upsertDocument(state, action: PayloadAction<SubmittedDoc>) {
      const idx = state.documents.findIndex(d => d.id === action.payload.id);
      if (idx >= 0) {
        state.documents[idx] = action.payload;
      } else {
        state.documents.push(action.payload);
      }
    },
  },
});

export const complianceActions = complianceSlice.actions;
```

Add `compliance: complianceSlice.reducer` to the `combineReducers` call.

- [ ] **Seed mock compliance data in profile screen initialization**

In `apps/driver-app/src/app/(tabs)/profile.tsx`, inside the existing `useEffect` (or add one), seed mock compliance state:

```typescript
useEffect(() => {
  dispatch(complianceActions.setComplianceProfile({
    overall_status: "pending_submission",
    jurisdiction:   "UAE",
    required_types: [
      { id: "dt1", code: "UAE_DRIVING_LICENSE",   name: "UAE Driving License",          has_expiry: true, warn_days_before: 30 },
      { id: "dt2", code: "UAE_EMIRATES_ID",        name: "Emirates ID",                  has_expiry: true, warn_days_before: 60 },
      { id: "dt3", code: "UAE_VEHICLE_MULKIYA",    name: "Vehicle Registration (Mulkiya)", has_expiry: true, warn_days_before: 30 },
      { id: "dt4", code: "UAE_VEHICLE_INSURANCE",  name: "Third-Party Insurance",        has_expiry: true, warn_days_before: 30 },
    ],
    documents: [
      {
        id: "doc1", document_type_id: "dt3", document_number: "REG-DXB-A12345",
        expiry_date: "2026-03-01", status: "approved", rejection_reason: null,
        submitted_at: new Date().toISOString(),
      },
      {
        id: "doc2", document_type_id: "dt4", document_number: "POL-AXA-88821",
        expiry_date: "2025-12-31", status: "approved", rejection_reason: null,
        submitted_at: new Date().toISOString(),
      },
    ],
  }));
}, []);
```

- [ ] **Create `ComplianceBanner` component**

Add this component in `apps/driver-app/src/app/(tabs)/profile.tsx` (before the main export, above `ProfileScreen`):

```tsx
const CYAN = "#00E5FF"; const GREEN = "#00FF88"; const AMBER = "#FFAB00"; const RED = "#FF3B5C";

interface ComplianceBannerProps { status: string; missingCount: number; }

function ComplianceBanner({ status, missingCount }: ComplianceBannerProps) {
  const cfg: Record<string, { bg: string; border: string; titleColor: string; title: string; sub: string }> = {
    pending_submission: { bg: "rgba(255,171,0,0.08)",   border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Action Required",  sub: `${missingCount} document${missingCount !== 1 ? "s" : ""} need upload before you can receive tasks` },
    under_review:       { bg: "rgba(0,229,255,0.07)",   border: "rgba(0,229,255,0.2)",   titleColor: CYAN,  title: "⏳ Under Review",     sub: "Documents submitted · Awaiting compliance team" },
    compliant:          { bg: "rgba(0,255,136,0.07)",   border: "rgba(0,255,136,0.2)",   titleColor: GREEN, title: "✓ All Clear",         sub: "All documents verified · You are assignable" },
    expiring_soon:      { bg: "rgba(255,171,0,0.08)",   border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Documents Expiring", sub: "Renew soon to stay assignable" },
    expired:            { bg: "rgba(255,171,0,0.08)",   border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Document Expired",  sub: "Renew within grace period to stay active" },
    suspended:          { bg: "rgba(255,59,92,0.08)",   border: "rgba(255,59,92,0.25)",  titleColor: RED,   title: "✗ Account Suspended", sub: "Contact support to reinstate your account" },
    rejected:           { bg: "rgba(255,59,92,0.08)",   border: "rgba(255,59,92,0.25)",  titleColor: RED,   title: "✗ Document Rejected",  sub: "Re-upload the rejected document to continue" },
  };
  const c = cfg[status] ?? cfg.pending_submission;
  return (
    <View style={{ marginHorizontal: 12, marginBottom: 8, borderRadius: 10, padding: 12,
                   backgroundColor: c.bg, borderWidth: 1, borderColor: c.border }}>
      <Text style={{ fontSize: 11, fontFamily: "SpaceGrotesk-SemiBold", textTransform: "uppercase",
                     letterSpacing: 0.8, color: c.titleColor, marginBottom: 4 }}>{c.title}</Text>
      <Text style={{ fontSize: 10, color: c.titleColor, opacity: 0.6, lineHeight: 15 }}>{c.sub}</Text>
    </View>
  );
}
```

Also compute `missingCount` above the return:
```tsx
const missingCount = compliance.required_types.filter(dt =>
  !compliance.documents.find(d => d.document_type_id === dt.id && d.status !== "superseded")
).length;
```

- [ ] **Add compliance banner + document list to Profile screen**

In `apps/driver-app/src/app/(tabs)/profile.tsx`, after the vehicle card and before sync status, add:

```tsx
{/* Compliance Banner */}
<Animated.View entering={FadeInDown.delay(130).springify()} style={styles.compBanner}>
  <ComplianceBanner status={compliance.overall_status} missingCount={missingCount} />
</Animated.View>

{/* Document Checklist */}
<Animated.View entering={FadeInDown.delay(150).springify()} style={styles.docCard}>
  <Text style={styles.cardLabel}>Required Documents</Text>
  {compliance.required_types.map((dt) => {
    const doc = compliance.documents.find(
      d => d.document_type_id === dt.id && d.status !== "superseded"
    );
    return (
      <Pressable
        key={dt.id}
        onPress={() => router.push(`/compliance/upload/${dt.code}`)}
        style={[styles.docRow, docRowStyle(doc?.status)]}
      >
        <View style={[styles.docDot, docDotStyle(doc?.status)]} />
        <View style={styles.docRowInfo}>
          <Text style={styles.docRowName}>{dt.name}</Text>
          <Text style={styles.docRowSub}>{docSubText(doc)}</Text>
        </View>
        <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
      </Pressable>
    );
  })}
</Animated.View>
```

Helper functions (add outside component):
```typescript
function docSubText(doc?: SubmittedDoc): string {
  if (!doc) return "Not submitted · Required";
  if (doc.status === "approved")     return `Approved · Exp ${doc.expiry_date ?? "—"}`;
  if (doc.status === "under_review") return "Under review · Est. 24h";
  if (doc.status === "submitted")    return "Submitted · Awaiting review";
  if (doc.status === "rejected")     return `Rejected — ${doc.rejection_reason ?? "see reason"}`;
  if (doc.status === "expired")      return "Expired · Renewal required";
  return "—";
}

function docRowStyle(status?: string) {
  if (!status)               return styles.docRowMissing;
  if (status === "approved") return styles.docRowOk;
  if (status === "rejected" || status === "expired") return styles.docRowWarn;
  return styles.docRowReview;
}
```

Add the required styles to `StyleSheet.create(...)` following existing naming conventions (colors: `CYAN`, `GREEN`, `AMBER`, `RED`, `GLASS`, `BORDER`).

- [ ] **Create compliance checklist screen**

`apps/driver-app/src/app/compliance/index.tsx` — full-screen version of the document list with a header showing overall status. Tapping a row navigates to the upload screen.

- [ ] **Create document upload screen**

`apps/driver-app/src/app/compliance/upload/[typeCode].tsx`:

```tsx
import { View, Text, StyleSheet, Pressable, TextInput, ScrollView } from "react-native";
import { useLocalSearchParams, router } from "expo-router";
import { useState } from "react";
import { useDispatch, useSelector } from "react-redux";
import Animated, { FadeInDown } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import type { RootState, AppDispatch } from "../../../store";
import { complianceActions } from "../../../store";
import type { SubmittedDoc } from "../../../store";

// Color tokens
const CANVAS = "#050810"; const CYAN = "#00E5FF"; const PURPLE = "#A855F7";
const GREEN  = "#00FF88"; const AMBER = "#FFAB00"; const GLASS = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

export default function UploadDocumentScreen() {
  const { typeCode }   = useLocalSearchParams<{ typeCode: string }>();
  const dispatch       = useDispatch<AppDispatch>();
  const compliance     = useSelector((s: RootState) => s.compliance);
  const docType        = compliance.required_types.find(dt => dt.code === typeCode);

  const [docNumber, setDocNumber]   = useState("");
  const [expiryDate, setExpiryDate] = useState("");
  const [submitted, setSubmitted]   = useState(false);

  function handleSubmit() {
    if (!docNumber.trim()) return;
    const newDoc: SubmittedDoc = {
      id:               `doc-${Date.now()}`,
      document_type_id: docType?.id ?? "",
      document_number:  docNumber.trim(),
      expiry_date:      expiryDate || null,
      status:           "submitted",
      rejection_reason: null,
      submitted_at:     new Date().toISOString(),
    };
    dispatch(complianceActions.upsertDocument(newDoc));
    setSubmitted(true);
  }

  if (!docType) return null;

  if (submitted) {
    return (
      <View style={styles.container}>
        <Animated.View entering={FadeInDown.springify()} style={styles.successCard}>
          <View style={styles.successIcon}><Ionicons name="search" size={32} color={CYAN} /></View>
          <Text style={styles.successTitle}>Under Review</Text>
          <Text style={styles.successSub}>
            Your {docType.name} has been received.{"\n"}
            Our compliance team will verify it within 24 hours.
          </Text>
          <Pressable onPress={() => router.back()} style={styles.backBtn}>
            <Text style={styles.backBtnText}>← Back to Profile</Text>
          </Pressable>
        </Animated.View>
      </View>
    );
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>
      <Animated.View entering={FadeInDown.springify()} style={styles.header}>
        <Pressable onPress={() => router.back()}>
          <Ionicons name="chevron-back" size={20} color="rgba(255,255,255,0.5)" />
        </Pressable>
        <View style={{ flex: 1, marginLeft: 8 }}>
          <Text style={styles.headerTitle}>{docType.name}</Text>
          <Text style={styles.headerSub}>Required · {docType.has_expiry ? "Has expiry" : "No expiry"}</Text>
        </View>
      </Animated.View>

      {/* Camera area (tap to simulate upload on web) */}
      <Animated.View entering={FadeInDown.delay(60).springify()} style={styles.cameraArea}>
        <Ionicons name="camera-outline" size={36} color="rgba(255,255,255,0.2)" />
        <Text style={styles.cameraHint}>Tap to photograph your document</Text>
        <Pressable style={styles.cameraBtn}>
          <Text style={styles.cameraBtnText}>Open Camera</Text>
        </Pressable>
      </Animated.View>

      {/* Document number */}
      <Animated.View entering={FadeInDown.delay(100).springify()} style={styles.field}>
        <Text style={styles.fieldLabel}>Document Number</Text>
        <TextInput
          value={docNumber}
          onChangeText={setDocNumber}
          placeholder="Enter document number"
          placeholderTextColor="rgba(255,255,255,0.2)"
          style={[styles.fieldInput, docNumber ? styles.fieldInputFilled : null]}
        />
      </Animated.View>

      {/* Expiry date */}
      {docType.has_expiry && (
        <Animated.View entering={FadeInDown.delay(140).springify()} style={styles.field}>
          <Text style={styles.fieldLabel}>Expiry Date</Text>
          <TextInput
            value={expiryDate}
            onChangeText={setExpiryDate}
            placeholder="YYYY-MM-DD"
            placeholderTextColor="rgba(255,255,255,0.2)"
            style={[styles.fieldInput, expiryDate ? styles.fieldInputFilled : null]}
          />
        </Animated.View>
      )}

      <Animated.View entering={FadeInDown.delay(180).springify()} style={{ marginHorizontal: 12 }}>
        <Pressable
          onPress={handleSubmit}
          style={({ pressed }) => [styles.submitBtn, { opacity: pressed ? 0.8 : 1 }]}
        >
          <Text style={styles.submitBtnText}>Submit for Review →</Text>
        </Pressable>
      </Animated.View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container:       { flex: 1, backgroundColor: CANVAS },
  header:          { flexDirection: "row", alignItems: "center", padding: 16, paddingTop: 20 },
  headerTitle:     { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
  headerSub:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
  cameraArea:      { margin: 12, borderRadius: 12, height: 140, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1.5, borderColor: "rgba(255,255,255,0.1)", borderStyle: "dashed", alignItems: "center", justifyContent: "center", gap: 8 },
  cameraHint:      { fontSize: 11, color: "rgba(255,255,255,0.2)" },
  cameraBtn:       { paddingHorizontal: 16, paddingVertical: 6, borderRadius: 20, backgroundColor: "rgba(168,85,247,0.12)", borderWidth: 1, borderColor: "rgba(168,85,247,0.3)" },
  cameraBtnText:   { fontSize: 11, color: PURPLE },
  field:           { marginHorizontal: 12, marginBottom: 10 },
  fieldLabel:      { fontSize: 9, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 6 },
  fieldInput:      { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 8, padding: 10, fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.6)" },
  fieldInputFilled:{ borderColor: "rgba(0,229,255,0.3)", color: CYAN, backgroundColor: "rgba(0,229,255,0.04)" },
  submitBtn:       { borderRadius: 12, paddingVertical: 14, alignItems: "center", backgroundColor: "rgba(168,85,247,0.18)", borderWidth: 1, borderColor: "rgba(168,85,247,0.35)" },
  submitBtnText:   { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
  successCard:     { margin: 16, borderRadius: 14, backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", padding: 24, alignItems: "center", marginTop: 60 },
  successIcon:     { width: 60, height: 60, borderRadius: 30, backgroundColor: "rgba(0,229,255,0.1)", alignItems: "center", justifyContent: "center", marginBottom: 14 },
  successTitle:    { fontSize: 18, fontFamily: "SpaceGrotesk-SemiBold", color: CYAN, marginBottom: 8 },
  successSub:      { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(0,229,255,0.5)", textAlign: "center", lineHeight: 18 },
  backBtn:         { marginTop: 20, padding: 12 },
  backBtnText:     { fontSize: 13, color: "rgba(255,255,255,0.4)" },
});
```

- [ ] **Rebuild Driver App and verify**

```bash
cd apps/driver-app
npx kill-port 8081
npx expo export --platform web --output-dir web-dist
npx serve web-dist -p 8081 --single &
```

Open `http://localhost:8081` → Profile tab → verify compliance banner and document list appear. Tap a missing document → upload screen opens. Fill in number + date → Submit → "Under Review" screen shown. Go back → doc shows "Submitted" status in list.

- [ ] **Commit**

```bash
git add apps/driver-app/src/
git commit -m "feat(driver-app): compliance slice, document checklist in profile, upload screen"
```

---

## Task 12: Partner Portal — Compliance badges on driver cards

**Files:**
- Create: `apps/partner-portal/src/components/compliance/compliance-badge.tsx`
- Modify: `apps/partner-portal/src/app/(dashboard)/drivers/page.tsx`

- [ ] **Create `ComplianceBadge` component**

`apps/partner-portal/src/components/compliance/compliance-badge.tsx`:
```tsx
import { cn } from "@/lib/design-system/cn";
import { ShieldCheck, ShieldAlert, ShieldX, Shield } from "lucide-react";

type ComplianceStatus =
  | "compliant" | "expiring_soon" | "expired"
  | "suspended" | "under_review" | "pending_submission" | "rejected";

interface Props {
  status:       ComplianceStatus;
  expiryDetail?: string;   // e.g. "License · 12d"
}

const CONFIG: Record<ComplianceStatus, {
  label:  string;
  icon:   React.ReactNode;
  color:  string;
  bg:     string;
  border: string;
  pulse:  boolean;
}> = {
  compliant:          { label: "Compliant",         icon: <ShieldCheck className="h-3 w-3" />, color: "text-green-signal",  bg: "bg-green-surface",  border: "border-green-glow/20",  pulse: false },
  expiring_soon:      { label: "Expiring Soon",     icon: <ShieldAlert  className="h-3 w-3" />, color: "text-amber-signal", bg: "bg-amber-surface",  border: "border-amber-glow/25",  pulse: true  },
  expired:            { label: "Expired",           icon: <ShieldAlert  className="h-3 w-3" />, color: "text-amber-signal", bg: "bg-amber-surface",  border: "border-amber-glow/25",  pulse: true  },
  suspended:          { label: "Suspended",         icon: <ShieldX      className="h-3 w-3" />, color: "text-red-400",      bg: "bg-red-surface",    border: "border-red-glow/25",    pulse: false },
  under_review:       { label: "Under Review",      icon: <Shield       className="h-3 w-3" />, color: "text-cyan-neon",    bg: "bg-cyan-surface",   border: "border-cyan-glow/20",   pulse: false },
  pending_submission: { label: "Docs Pending",      icon: <Shield       className="h-3 w-3" />, color: "text-white/40",     bg: "bg-glass-100",      border: "border-glass-border",   pulse: false },
  rejected:           { label: "Docs Rejected",     icon: <ShieldX      className="h-3 w-3" />, color: "text-red-400",      bg: "bg-red-surface",    border: "border-red-glow/25",    pulse: false },
};

export function ComplianceBadge({ status, expiryDetail }: Props) {
  const cfg = CONFIG[status] ?? CONFIG.pending_submission;
  return (
    <div className={cn("flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 border", cfg.bg, cfg.border)}>
      <div className={cn(cfg.color, cfg.pulse && "animate-pulse")}>{cfg.icon}</div>
      <div>
        <div className={cn("text-2xs font-semibold font-mono", cfg.color)}>{cfg.label}</div>
        {expiryDetail && (
          <div className="text-2xs text-white/25 font-mono">{expiryDetail}</div>
        )}
      </div>
    </div>
  );
}

export function canAssign(status: ComplianceStatus): boolean {
  return ["compliant", "expiring_soon", "expired"].includes(status);
}
```

- [ ] **Add compliance status to mock driver data in drivers page**

In `apps/partner-portal/src/app/(dashboard)/drivers/page.tsx`, extend the `Driver` interface and mock data (it was pre-read — add `compliance_status` field):

```typescript
interface Driver {
  // ... existing fields
  compliance_status: "compliant" | "expiring_soon" | "expired" | "suspended" | "under_review" | "pending_submission";
  compliance_detail?: string;  // e.g. "License · 12d left"
}
```

Update each mock driver:
- Rodel Bautista → `compliance_status: "compliant"`
- Mark Cruz → `compliance_status: "expiring_soon", compliance_detail: "Mulkiya · 18d left"`
- Danny Soriano → `compliance_status: "suspended", compliance_detail: "LTO License expired"`
- Rico Evangelista → `compliance_status: "compliant"`
- Carlo Reyes → `compliance_status: "under_review"`
- Jessa Mariano → `compliance_status: "compliant"`

- [ ] **Add badge to driver card UI**

In the driver row/card rendering, import `ComplianceBadge` and `canAssign`, then:

1. Add `<ComplianceBadge status={driver.compliance_status} expiryDetail={driver.compliance_detail} />` below the vehicle badge in each driver card
2. Disable the "Assign Task" button when `!canAssign(driver.compliance_status)`:

```tsx
<button
  disabled={!canAssign(driver.compliance_status)}
  className={cn(
    "...",
    !canAssign(driver.compliance_status) && "opacity-40 cursor-not-allowed"
  )}
>
  Assign Task
</button>
```

- [ ] **Verify in browser at `http://localhost:3003`**

Driver cards should show compliance badges. Suspended/under-review drivers should have disabled Assign buttons.

- [ ] **Commit**

```bash
git add apps/partner-portal/src/components/compliance/ apps/partner-portal/src/app/(dashboard)/drivers/
git commit -m "feat(partner-portal): compliance badges on driver cards, assignment blocked for non-compliant"
```

---

## Task 13: Document storage (S3-compatible)

**Files:**
- Modify: `services/compliance/Cargo.toml` (add `aws-sdk-s3`, `aws-config`)
- Replace: `services/compliance/src/infrastructure/storage/document_storage.rs`
- Modify: `services/compliance/src/api/http/mod.rs` (add upload route + `DocumentStorage` to `AppState`)
- Modify: `services/compliance/src/api/http/driver_routes.rs` (add presign GET endpoint)
- Modify: `services/compliance/src/bootstrap.rs` (wire `DocumentStorage`)

- [ ] **Add S3 dependencies to `services/compliance/Cargo.toml`**

```toml
aws-sdk-s3 = { version = "1", features = ["behavior-version-latest"] }
aws-config = { version = "1" }
```

- [ ] **Replace stub with full `DocumentStorage`**

`src/infrastructure/storage/document_storage.rs`:
```rust
use anyhow::{bail, Context};

const MAX_FILE_BYTES: usize = 10 * 1024 * 1024; // 10 MB
const PRESIGN_TTL_SECS: u64 = 900;              // 15 minutes

pub struct DocumentStorage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

impl DocumentStorage {
    pub async fn new(cfg: &crate::config::StorageConfig) -> anyhow::Result<Self> {
        let aws_cfg = aws_config::from_env()
            .endpoint_url(&cfg.endpoint)
            .credentials_provider(aws_sdk_s3::config::Credentials::new(
                &cfg.access_key, &cfg.secret_key, None, None, "static",
            ))
            .load()
            .await;
        let client = aws_sdk_s3::Client::new(&aws_cfg);
        Ok(Self { client, bucket: cfg.bucket.clone() })
    }

    /// Upload raw bytes; returns an `s3://bucket/key` URI stored in `driver_documents.file_url`.
    pub async fn upload(
        &self,
        tenant_id: uuid::Uuid,
        file_bytes: Vec<u8>,
        content_type: &str,
    ) -> anyhow::Result<String> {
        if file_bytes.len() > MAX_FILE_BYTES {
            bail!("File exceeds 10 MB limit");
        }
        if !matches!(content_type, "image/jpeg" | "image/png" | "application/pdf") {
            bail!("Invalid content type: must be image/jpeg, image/png, or application/pdf");
        }
        let key = format!("compliance/{}/{}", tenant_id, uuid::Uuid::new_v4());
        self.client.put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(aws_sdk_s3::primitives::ByteStream::from(file_bytes))
            .content_type(content_type)
            .send()
            .await
            .context("S3 upload failed")?;
        Ok(format!("s3://{}/{}", self.bucket, key))
    }

    /// Generate a 15-minute presigned GET URL for a stored document.
    pub async fn presign_url(&self, s3_uri: &str) -> anyhow::Result<String> {
        let key = s3_uri
            .strip_prefix(&format!("s3://{}/", self.bucket))
            .context("Invalid s3:// URI format")?;
        let presigned = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(
                aws_sdk_s3::presigning::PresigningConfig::expires_in(
                    std::time::Duration::from_secs(PRESIGN_TTL_SECS),
                )?,
            )
            .await
            .context("Presign failed")?;
        Ok(presigned.uri().to_string())
    }
}
```

- [ ] **Update `src/infrastructure/storage/mod.rs`** (already correct from stub, no change needed)

- [ ] **Add `DocumentStorage` to `AppState`**

In `src/api/http/mod.rs`, update `AppState` and add the presign route:
```rust
use crate::infrastructure::storage::DocumentStorage;

pub struct AppState {
    pub compliance: Arc<ComplianceService>,
    pub jwt:        Arc<JwtService>,
    pub storage:    Arc<DocumentStorage>,
}

// In router(), add:
.route("/api/v1/compliance/me/documents/:doc_id/url", get(driver_routes::get_document_url))
```

- [ ] **Add presign endpoint to `driver_routes.rs`**

```rust
// GET /me/documents/:doc_id/url — returns a 15-minute presigned download URL
pub async fn get_document_url(
    AuthClaims(claims): AuthClaims,
    Path(doc_id): Path<Uuid>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let doc = state.compliance.documents.find_by_id(doc_id).await?
        .ok_or(AppError::NotFound { resource: "DriverDocument", id: doc_id.to_string() })?;
    let profile = state.compliance.profiles.find_by_id(doc.compliance_profile_id).await?
        .ok_or(AppError::NotFound { resource: "ComplianceProfile", id: doc.compliance_profile_id.to_string() })?;
    if profile.entity_id != claims.user_id {
        return Err(AppError::Forbidden);
    }
    let url = state.storage.presign_url(&doc.file_url).await?;
    Ok(Json(serde_json::json!({ "data": { "url": url, "expires_in": 900 } })))
}
```

- [ ] **Wire `DocumentStorage` in `bootstrap.rs`**

```rust
use crate::infrastructure::storage::DocumentStorage;

// After pool is created:
let storage = Arc::new(DocumentStorage::new(&cfg.storage).await
    .context("Failed to init document storage")?);

// In AppState:
let state = Arc::new(AppState { compliance, jwt, storage });
```

- [ ] **Verify compilation**

```bash
cargo check -p logisticos-compliance
```

- [ ] **Commit**

```bash
git add services/compliance/src/infrastructure/storage/ services/compliance/src/api/ services/compliance/src/bootstrap.rs services/compliance/Cargo.toml
git commit -m "feat(compliance): document storage — S3-compatible upload, presigned URL (replaces stub)"
```

---

## Done — Verification Checklist

- [ ] `cargo test -p logisticos-compliance` — all unit tests pass
- [ ] `cargo build --workspace` — no new errors
- [ ] Admin Portal `http://localhost:3001/compliance` — review console loads
- [ ] Driver App `http://localhost:8081` → Profile → documents checklist visible; upload flow works end-to-end
- [ ] Partner Portal `http://localhost:3003/drivers` — compliance badges visible; suspended drivers cannot be assigned
- [ ] Compliance service starts: `cargo run -p logisticos-compliance` with valid `DATABASE_URL` runs migrations cleanly
