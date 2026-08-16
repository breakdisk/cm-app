# ADR-0016: Application-Layer Tenancy — Abandon Row-Level Security

**Status:** Accepted
**Date:** 2026-08-07
**Deciders:** Principal Architect, Engineering Manager — Platform Core, Database Reliability Engineer, CISO

> **Load-bearing invariant.** Tenant isolation is enforced by the repository signature: every query method takes `tenant_id` and every statement filters on it. There is no second layer, and the schema must not imply one.

---

## Context

Every service in this repo enables Row-Level Security and defines a policy against `current_setting('app.tenant_id')`. As of 2026-08-07 that is **83 `ENABLE ROW LEVEL SECURITY` statements and 92 policies across 18 services**.

None of it does anything.

1. **No service ever sets the variable.** There is no `SET LOCAL app.tenant_id` anywhere in the codebase — a repo-wide search returns only a comment noting its absence.
2. **Services connect as the schema owner**, and PostgreSQL exempts the owning role from RLS unless `FORCE ROW LEVEL SECURITY` is set. So the policies neither filter nor fail.
3. **Where `FORCE` was tried, it broke reads and was reverted** — `order-intake/0007` and `driver-ops/0005_disable_rls_drivers.sql` are both in the tree as evidence.

The result is worse than having nothing. A reader who greps for `ENABLE ROW LEVEL SECURITY` concludes the database enforces tenant isolation. It does not. Every real isolation guarantee this platform has comes from application code, and the schema currently disagrees with the code about who is responsible.

This came to a head when `services/omnideliv` and `services/field-ops` were built: both deliberately omitted RLS and documented why, which left two contradictory conventions in one codebase.

## Decision

**Tenant isolation is application-layer. The decorative RLS is removed.**

1. Every repository method takes `tenant_id` as an argument, and every SQL statement filters on it. The method signature is the enforcement point — a method that can be called without a tenant is a method that can leak across tenants, and that is a review-visible defect rather than an invisible one.
2. `ENABLE ROW LEVEL SECURITY` and its policies are dropped from every schema, by migration.
3. New schemas do not add RLS. `omnideliv` and `field_ops` already comply.

### Why not make RLS real

Making it enforce requires, together and in this order: a per-request `SET LOCAL app.tenant_id` on every checked-out connection; a pooling model where a connection cannot be reused across tenants without resetting it; a non-owner role for every service; and `FORCE ROW LEVEL SECURITY` on 83 tables. Any one of those done alone silently changes nothing, and the combination is a platform-wide programme touching every service's connection handling.

It is defensible work. It is not work anyone has scheduled, and it has now sat undone long enough to be mistaken for done — which is the specific failure this ADR exists to end. If it is ever scheduled, this ADR is superseded rather than amended: re-introducing RLS is a new decision with a new plan, not a footnote on this one.

## Consequences

### Positive
- The schema stops asserting a guarantee that does not exist.
- One convention, applied everywhere, instead of two that contradict.
- Removing dead policies removes a class of confusing failure: a future engineer setting `FORCE` on one table would break reads in production, as has already happened twice.

### Negative — stated plainly
- **A repository method that forgets its `tenant_id` filter leaks across tenants, and nothing below it will catch that.** This is the cost of the decision and it is real. It is mitigated by the signature convention and by review, not by the database.
- **A direct SQL console has no guardrail.** Anyone with database credentials can read every tenant. That was already true — RLS never applied to the owner — but it is now explicit rather than apparently mitigated.

### Neutral
- No runtime behaviour changes. The policies were inert; dropping them is observably a no-op, which is exactly why this is safe to do in one pass.

## Alternatives Considered

### Alternative 1: Leave the policies in place
**Rejected.** Zero effort, and it preserves the option of enabling them later. But it leaves the schema stating something false, and the two reverted `FORCE` attempts show that the false statement actively misleads engineers into breaking production.

### Alternative 2: Make RLS genuinely enforce
**Rejected for now,** on scope rather than merit — see "Why not make RLS real". Revisit as its own ADR with its own plan.

### Alternative 3: Keep RLS only on the highest-risk tables (payments, identity)
**Rejected.** A partial guarantee is the hardest kind to reason about: every reader would have to know which tables are covered, and the tables most worth protecting are exactly the ones where a wrong assumption is most expensive.

## References
- ADR-0003, ADR-0008 — the original tenancy decisions this supersedes in part
- ADR-0012: Schema-Isolated SQLx Migrations
- `services/driver-ops/migrations/0005_disable_rls_drivers.sql` — the first reversal
- `services/omnideliv/migrations/0001_create_vendors.sql` — the tenancy note that prompted this
