# ADR-0003: PostgreSQL Row-Level Security for Multi-Tenant Isolation

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, CISO

## Context

LogisticOS is a multi-tenant SaaS. Every table in every service schema contains data from multiple tenants. We must guarantee that Tenant A can never read or modify Tenant B's data — even if application code has a bug.

Approaches evaluated:
- **Separate databases per tenant** — operational nightmare at 100+ tenants
- **Schema per tenant** — migration sprawl, schema explosion
- **Shared schema + application filtering** — relies on every query having a `WHERE tenant_id = $x` — one missed WHERE clause leaks data
- **PostgreSQL Row-Level Security (RLS)** — database-enforced, application bugs can't bypass it

## Decision

**PostgreSQL RLS** on every table in every service schema.

## Implementation Pattern

```sql
-- Every tenant-scoped table:
ALTER TABLE order_intake.shipments ENABLE ROW LEVEL SECURITY;
ALTER TABLE order_intake.shipments FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON order_intake.shipments
    USING (tenant_id = current_setting('app.tenant_id')::uuid);
```

The `app.tenant_id` session variable is set at connection time by the application layer, extracted from the validated JWT claim. The `libs/auth` crate's DB pool wrapper sets this automatically on every connection checkout.

## Consequences

- **Defense in depth:** data isolation enforced at the DB layer, independent of application code
- **No performance regression:** RLS policies on indexed `tenant_id` columns add negligible overhead
- **Requires discipline in migrations:** every new table must include `tenant_id UUID NOT NULL` and the RLS policy
- **Testing requirement:** integration tests must verify cross-tenant isolation explicitly
