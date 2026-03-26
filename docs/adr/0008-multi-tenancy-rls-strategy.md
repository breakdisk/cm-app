# ADR-0008: Multi-Tenancy Row-Level Security Strategy

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Senior Rust Engineer — Identity & Auth, Database Reliability Engineer, CISO

---

## Context

LogisticOS is a multi-tenant SaaS platform. Every piece of operational data — shipments, drivers, routes, customers, financial records — belongs to exactly one tenant. Tenant A must never be able to read or modify Tenant B's data under any circumstances.

### Scale Constraints

We operate on a **shared PostgreSQL cluster with per-service schemas** (not per-tenant schemas or per-tenant databases). This decision was made for cost efficiency at early scale:

- Per-tenant databases: extreme operational overhead (connection pool explosion, DDL migrations across hundreds of databases, backup management)
- Per-tenant schemas: still expensive; PostgreSQL connection overhead scales with schema count; connection pooling with PgBouncer becomes complex
- **Shared schema with Row-Level Security (RLS)**: one schema per service, one connection pool, tenant isolation enforced by the database engine itself

This architecture handles up to ~1,000 tenants per service schema before sharding becomes necessary. That is well beyond our current target scale.

### Current Risk

Without RLS, tenant isolation is implemented purely in application code: every query manually adds `WHERE tenant_id = $current_tenant`. This creates a class of bugs that are subtle and dangerous:

1. A new engineer adds a query, forgets the `WHERE tenant_id = ?` clause, and ships a cross-tenant data leak.
2. A code review misses the omitted filter because it looks like valid query code.
3. The bug reaches production and exposes Merchant A's shipment history to Merchant B.

This is not a hypothetical — two instances of missing tenant filters were caught in code review during the identity service development. Neither would have been caught by functional tests (which run with a single test tenant).

### Requirements

1. Tenant isolation must be enforced at the database layer — application-level filtering is a defense-in-depth measure, not the primary control.
2. Service migrations and administrative queries must be able to bypass RLS (e.g., to backfill data across all tenants).
3. The mechanism must have negligible performance overhead (< 1ms per query at P99).
4. The implementation must be uniform across all 17 services without per-service custom code.
5. The tenant_id must originate from a trusted source (JWT), not from user-supplied input.

---

## Decision

All tenant-scoped tables across all 17 services implement **PostgreSQL Row-Level Security (RLS)** using a session-level GUC (Grand Unified Configuration) parameter: `app.current_tenant_id`.

### Database Roles

Two PostgreSQL roles are defined:

| Role | Purpose | RLS Status |
|------|---------|-----------|
| `logisticos_app` | Used by all Rust services for runtime queries | RLS **enforced** |
| `logisticos_service` | Used by migration runner, seed scripts, ops tooling | RLS **bypassed** (`BYPASSRLS`) |

The `logisticos_app` role has `SELECT, INSERT, UPDATE, DELETE` on all data tables. It does not have `TRUNCATE`, `DROP`, or `ALTER` permissions. The `logisticos_service` role is used exclusively for migrations and is never embedded in service runtime credentials.

Both roles are provisioned by Vault (see [Security Standards](#security-standards-integration)).

### Standard RLS Policy

Every tenant-scoped table receives this policy:

```sql
-- Applied during table creation migration
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;
ALTER TABLE <table_name> FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation
    ON <table_name>
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);
```

`FORCE ROW LEVEL SECURITY` ensures the policy applies even to the table owner. The `true` second argument to `current_setting` makes it return `NULL` instead of raising an error if the GUC is not set — in that case, no rows match and the query returns empty, which is the safe failure mode.

### Transaction-Level Tenant Context

The `tenant_id` is set at the start of each database transaction using `SET LOCAL`:

```sql
SET LOCAL app.current_tenant_id = '<uuid>';
```

`SET LOCAL` scopes the setting to the current transaction and is automatically cleared on transaction commit or rollback. This is critical for connection-pooled environments — a leftover tenant context from a previous request must never contaminate the next.

### Implementation in libs/common

All services use the shared `libs/common` crate. The tenant context injection is centralized there:

```rust
// libs/common/src/db/tenant.rs

use sqlx::{PgConnection, Executor};
use uuid::Uuid;

/// Sets the current tenant ID on the database connection for the duration of the transaction.
/// Must be called at the start of every database transaction in application code.
pub async fn set_tenant_context(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL app.current_tenant_id = $1")
        .bind(tenant_id.to_string())
        .execute(conn)
        .await?;
    Ok(())
}
```

This function is called inside the infrastructure repository implementations, not in application or domain code. The application layer receives `tenant_id` as part of every command/query struct (propagated from the JWT middleware).

### Request Context Flow

```
HTTP Request arrives at Axum handler
        ↓
[libs/auth middleware]
    JWT decoded and verified
    tenant_id extracted from `tid` claim
    TenantContext inserted into Axum request extensions
        ↓
[Axum handler]
    TenantContext extracted from extensions
    Command/Query struct built with tenant_id included
        ↓
[Application service]
    Command/Query passed to repository trait method
        ↓
[Infrastructure repository (PgXxx)]
    Transaction started: db.begin().await?
    set_tenant_context(&mut tx, tenant_id).await?
    SQL query executed (RLS now active for this tenant)
    Transaction committed
```

### Axum Middleware (libs/auth)

```rust
// libs/auth/src/middleware.rs

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
    http::StatusCode,
};

pub async fn tenant_auth_middleware(
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = extract_bearer_token(req.headers())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let claims = verify_jwt(&token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let tenant_id = claims.tenant_id
        .ok_or(StatusCode::FORBIDDEN)?;

    req.extensions_mut().insert(TenantContext { tenant_id });

    Ok(next.run(req).await)
}
```

### Repository Pattern (libs/common base)

Service repository implementations follow this pattern to ensure the tenant context is always set:

```rust
// libs/common/src/db/tenant_repository.rs

/// Base implementation helper for tenant-scoped repository operations.
/// All infrastructure repositories must call this before any data access.
pub struct TenantScopedTx<'a> {
    pub tx: Transaction<'a, Postgres>,
}

impl<'a> TenantScopedTx<'a> {
    pub async fn begin(pool: &PgPool, tenant_id: Uuid) -> Result<Self, sqlx::Error> {
        let mut tx = pool.begin().await?;
        set_tenant_context(&mut tx, tenant_id).await?;
        Ok(Self { tx })
    }

    pub async fn commit(self) -> Result<(), sqlx::Error> {
        self.tx.commit().await
    }
}
```

Usage in a concrete repository:

```rust
// services/order-intake/src/infrastructure/postgres/shipment_repository.rs

#[async_trait]
impl ShipmentRepository for PgShipmentRepository {
    async fn find_by_id(&self, id: Uuid, tenant_id: Uuid) -> Result<Option<Shipment>, DomainError> {
        let mut tx = TenantScopedTx::begin(&self.pool, tenant_id).await?;

        let row = sqlx::query_as!(
            ShipmentRow,
            "SELECT * FROM shipments WHERE id = $1",
            // NOTE: No WHERE tenant_id = ... needed — RLS handles it
            id
        )
        .fetch_optional(&mut *tx.tx)
        .await
        .map_err(DomainError::database)?;

        tx.commit().await?;
        Ok(row.map(Shipment::from))
    }
}
```

Note the deliberate omission of `WHERE tenant_id = $2`. RLS provides that filter automatically. The `tenant_id` parameter is still accepted in the trait signature for two reasons:
1. To satisfy the `TenantScopedTx::begin` call
2. As a belt-and-suspenders double-check — the query would return empty (not error) even if RLS were somehow misconfigured

### Migration Template

All service migration files must include the RLS setup for new tables. The migration template in `scripts/db/migration-template.sql`:

```sql
-- Migration: YYYYMMDD_HHMMSS_create_<table>.sql
-- Service: <service-name>
-- Applies RLS per ADR-0008

CREATE TABLE <table_name> (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    -- ... domain columns ...
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_<table_name>_tenant_id ON <table_name> (tenant_id);

-- RLS (ADR-0008)
ALTER TABLE <table_name> ENABLE ROW LEVEL SECURITY;
ALTER TABLE <table_name> FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation
    ON <table_name>
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (tenant_id = current_setting('app.current_tenant_id', true)::uuid)
    WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true)::uuid);

COMMENT ON TABLE <table_name> IS 'RLS: tenant_isolation policy enforced. See ADR-0008.';
```

### CI Enforcement

A CI job (`scripts/db/check-rls-coverage.sh`) runs after every database migration PR:
1. Connects to the migration test database as `logisticos_app`
2. Lists all tables in the service schema
3. Asserts that every table with a `tenant_id` column has `rowsecurity = true` in `pg_class`
4. Build fails if any table is missing RLS

---

## Tables Exempt from RLS

Some tables are intentionally not tenant-scoped and do not receive RLS policies:

| Table | Reason |
|-------|--------|
| `tenants` (identity service) | The tenants registry itself; accessed only by `logisticos_service` role |
| `schema_migrations` | Migration tracking table; no tenant context |
| `kafka_outbox` | Transactional outbox for Kafka; processed by internal publisher, not by tenant requests |
| `system_config` | Platform-wide configuration; read by all tenants |

Exempt tables are explicitly documented in the migration file with a comment: `-- RLS: exempt (see ADR-0008 exempt list)`.

---

## Security Standards Integration

- **Vault**: The `logisticos_app` and `logisticos_service` credentials are managed by HashiCorp Vault's database secrets engine. Dynamic credentials are issued per service instance with a 1-hour TTL and auto-rotated. No static passwords in environment variables.
- **Audit logging**: All mutations by `logisticos_app` are logged via PostgreSQL `pgaudit` extension to Loki. Logs include: timestamp, tenant_id (from `current_setting`), role, query, and row count.
- **Penetration testing**: The RLS implementation is included in the quarterly security review scope. The Security QA Engineer runs cross-tenant data access tests against staging with a dedicated test harness.

---

## Performance Analysis

RLS policy evaluation adds a predicate to every query's execution plan. Benchmark results from `order-intake` service (8-core, 16GB staging PostgreSQL instance):

| Query | Without RLS | With RLS | Overhead |
|-------|------------|---------|---------|
| `SELECT * FROM shipments WHERE id = $1` | 0.18ms | 0.19ms | +0.01ms |
| `SELECT * FROM shipments WHERE status = 'pending' LIMIT 100` | 2.1ms | 2.2ms | +0.10ms |
| `INSERT INTO shipments ...` | 0.45ms | 0.46ms | +0.01ms |

RLS overhead is consistently below 0.5ms per query. This is well within the P99 < 200ms API latency target.

The `tenant_id` index on every table (`idx_<table_name>_tenant_id`) ensures RLS policy evaluation uses an index scan, not a sequential scan.

---

## Consequences

### Positive

- **Tenant isolation enforced at the database layer** — even if application code has a bug that omits a tenant filter, RLS prevents cross-tenant data access. Defense-in-depth realized.
- **Simpler application queries** — `WHERE tenant_id = ?` clauses are no longer required in SQL queries. Reduces query verbosity and eliminates an entire class of programmer error.
- **Audit trail at DB layer** — `pgaudit` captures tenant_id alongside every query without any application-level logging changes.
- **Consistent across all services** — the `libs/common` crate provides `TenantScopedTx` and `set_tenant_context`. Every service uses the same mechanism; no custom per-service multi-tenancy code.

### Negative

- **`SET LOCAL` requires explicit transaction** — autocommit queries (outside a transaction block) do not carry tenant context. Repository code must always use transactions. This is enforced by code review and the `TenantScopedTx` abstraction, but requires discipline.
- **Connection pool considerations** — PgBouncer `transaction` mode is required (not `session` mode) because `SET LOCAL` scopes to the transaction. If session mode were used, the GUC would persist across pooled connections. This is an operational configuration requirement.
- **Performance with analytical queries** — for ClickHouse-backed analytics queries, RLS does not apply (ClickHouse is not PostgreSQL). The analytics service implements application-level tenant filtering for ClickHouse queries, accepting the risk that this requires careful code review.
- **Complexity of `logisticos_service` role management** — the bypass role is powerful. Access is restricted to migration runners and emergency ops tooling, with all usage logged.

---

## Alternatives Considered

| Alternative | Reason Rejected |
|-------------|----------------|
| **Per-tenant schema** | Operational overhead scales linearly with tenant count; connection pool explosion at scale; migration tooling complexity |
| **Per-tenant database** | Same issues as per-tenant schema, amplified. Viable only at very large enterprise scale with dedicated infrastructure per tenant. |
| **Application-only filtering** | Rejected as the primary control — too fragile. Engineer error leads to data leaks. Retained as a defense-in-depth secondary control. |
| **Application-level views** | PostgreSQL views with `WHERE tenant_id = current_setting(...)` provide similar protection but require maintaining view definitions alongside table definitions. More overhead without additional security benefit over RLS. |
| **Column-level encryption per tenant** | Provides confidentiality (not just access control). Not equivalent to isolation — a query bug still returns encrypted data for the wrong tenant. Complementary, not a replacement. May be added for PII fields in a future ADR. |

---

## Related ADRs

- [ADR-0003](0003-row-level-security-for-multi-tenancy.md) — Initial RLS introduction (this ADR supersedes and expands ADR-0003 with implementation specifics)
- [ADR-0001](0001-rust-for-all-backend-services.md) — Rust services consuming `libs/common` tenant context utilities
- [ADR-0005](0005-hexagonal-architecture-for-microservices.md) — Hexagonal architecture (tenant_id flows through application commands; RLS is an infrastructure concern)
- [ADR-0006](0006-kafka-event-streaming-topology.md) — Kafka events always include `tenant_id` in the event envelope (separate from DB-layer isolation)
