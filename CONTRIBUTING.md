# Contributing to LogisticOS

LogisticOS is an AI-agentic, multi-tenant SaaS platform for logistics and last-mile delivery. It spans Rust microservices, Next.js web portals, and React Native mobile apps. This document establishes the engineering standards, workflows, and review criteria that every contributor must follow.

---

## Table of Contents

1. [Getting Started](#1-getting-started)
2. [Branch Naming Conventions](#2-branch-naming-conventions)
3. [Commit Message Format](#3-commit-message-format)
4. [API-First Development](#4-api-first-development)
5. [Architecture Decision Records](#5-architecture-decision-records)
6. [Rust Coding Standards](#6-rust-coding-standards)
7. [Frontend Coding Standards](#7-frontend-coding-standards)
8. [Testing Requirements](#8-testing-requirements)
9. [Security Guidelines](#9-security-guidelines)
10. [Pull Request Process](#10-pull-request-process)
11. [CI/CD Gates](#11-cicd-gates)
12. [Performance Standards](#12-performance-standards)
13. [Accessibility Requirements](#13-accessibility-requirements)

---

## 1. Getting Started

### Prerequisites

| Tool | Minimum Version | Purpose |
|------|----------------|---------|
| Rust (stable) | 1.78+ | All backend services |
| Node.js | 20 LTS | Next.js portals |
| pnpm | 9+ | Monorepo package management |
| Docker + Docker Compose | 24+ | Local service dependencies |
| sqlx-cli | latest | Database migrations |
| protoc | 25+ | Protobuf compilation |
| cargo-nextest | latest | Rust test runner |

### Repository Setup

```bash
git clone git@github.com:logisticos/logisticos.git
cd logisticos

# Install frontend dependencies
pnpm install

# Start local infrastructure (Postgres, Redis, Kafka, etc.)
docker compose up -d

# Run database migrations for all services
./scripts/migrate-all.sh

# Build all Rust services
cargo build --workspace

# Verify everything is healthy
./scripts/health-check.sh
```

### Environment Configuration

All secrets are managed through HashiCorp Vault. No secrets are stored in `.env` files or source code. For local development, a Vault dev server is started via Docker Compose and seeded automatically.

```bash
# Authenticate to the local Vault dev instance
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=dev-root-token

# Pull local dev secrets into your shell (read-only, never committed)
source ./scripts/dev-secrets.sh
```

Do not create `.env` files. If a service fails because a secret is missing, add it to the Vault seed script at `scripts/vault-seed.sh`.

### Running a Single Service

```bash
# From the repo root
cargo run -p logisticos-dispatch

# Or from the service directory
cd services/dispatch
cargo run
```

Each service exposes `/health`, `/metrics`, and `/ready` endpoints on its configured port. Ports are documented in `docs/architecture/service-ports.md`.

---

## 2. Branch Naming Conventions

All branches are created from `main`. Use the following prefixes:

| Prefix | Use for |
|--------|---------|
| `feat/` | New features or capabilities |
| `fix/` | Bug fixes |
| `refactor/` | Code restructuring without behavior change |
| `chore/` | Dependency updates, tooling, configuration |
| `docs/` | Documentation changes only |
| `perf/` | Performance improvements |
| `test/` | Adding or fixing tests without changing production code |

**Format:** `<prefix>/<service-name>/<short-description>`

The `<service-name>` segment must match the directory name under `services/`, `apps/`, or `libs/`. For cross-cutting changes, use the name of the primary service affected.

**Examples:**

```
feat/dispatch/ai-driver-scoring
fix/order-intake/duplicate-awb-validation
refactor/identity/extract-jwt-middleware
chore/merchant-portal/upgrade-next-14-5
docs/adr/add-0007-timescale-for-telemetry
perf/routing/spatial-index-optimization
```

Branch names must be lowercase, hyphen-separated, and contain no spaces or special characters other than `/` and `-`.

---

## 3. Commit Message Format

LogisticOS uses [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/). All commits are linted by a pre-commit hook and enforced in CI.

### Structure

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

### Types

| Type | When to use |
|------|-------------|
| `feat` | A new feature visible to users or downstream services |
| `fix` | A bug fix |
| `refactor` | Code change that neither adds a feature nor fixes a bug |
| `perf` | A change that improves performance |
| `test` | Adding or correcting tests |
| `docs` | Documentation only |
| `chore` | Build process, dependency updates, tooling |
| `ci` | Changes to CI/CD configuration |
| `revert` | Reverting a previous commit |

### Scope

The scope is the service or app directory name: `dispatch`, `order-intake`, `merchant-portal`, `driver-app`, `identity`, `libs/common`, etc.

### Rules

- The summary line must be 72 characters or fewer.
- Use the imperative mood: "add feature" not "added feature" or "adds feature".
- Do not end the summary line with a period.
- The body, when present, must be separated from the summary by a blank line.
- Reference issues and PRs in the footer using `Closes #123` or `Refs #456`.
- Breaking changes must include `BREAKING CHANGE:` in the footer with a description of what changes and migration steps.

### Examples

```
feat(dispatch): add ML-based driver scoring to assignment engine

Integrates the ONNX-served driver_score_v2 model into the assignment
pipeline. Scores are computed per candidate driver before VRP
optimization runs. Falls back to FIFO if the model returns an error.

Closes #412
```

```
fix(order-intake): reject duplicate AWB within same tenant

Previously, the uniqueness check was scoped globally rather than
per-tenant, causing false rejects across tenant boundaries.

Closes #389
```

```
feat(identity)!: replace API key hashing with BLAKE3

BREAKING CHANGE: Existing API keys hashed with SHA-256 are invalidated.
Tenants must rotate their keys via the portal before upgrading.
Migration guide: docs/runbooks/api-key-rotation.md
```

---

## 4. API-First Development

No service implementation work may begin until the API contract has been authored, reviewed, and merged into `main`.

### Rule

**Spec merged before code merged.** Implementation PRs that arrive without a corresponding merged spec PR will be rejected at review.

### gRPC Services

1. Author the `.proto` definition in `libs/proto/<service-name>/`.
2. Open a PR titled `spec(<service-name>): add <operation> protobuf definition`.
3. Obtain review and merge.
4. Begin implementation in a separate `feat/` branch.

Proto files must:
- Use `proto3` syntax.
- Include `google.api.http` bindings for HTTP transcoding where applicable.
- Version all packages: `package logisticos.dispatch.v1;`
- Document every message field and RPC with a comment.

### HTTP/REST Services and MCP Tools

1. Author the OpenAPI 3.1 spec in `docs/api/<service-name>.openapi.yaml`.
2. For MCP tool surfaces, document the tool schema in `docs/api/<service-name>.mcp.yaml`.
3. Open a spec PR, obtain review, and merge.
4. Begin implementation separately.

OpenAPI specs must:
- Define all request/response schemas with `required` fields explicitly listed.
- Use `$ref` to share schemas rather than duplicating inline definitions.
- Include at least one example per operation.
- Specify error response schemas for all documented error codes.

### Why This Matters

API contracts are reviewed by engineers across multiple services. Changing them after implementation is costly and often requires coordination across service owners, mobile teams, and external integrators. Spec-first prevents rework and makes parallel development possible.

---

## 5. Architecture Decision Records

An ADR is required for every decision that affects system architecture, technology selection, cross-service contracts, data models, or security posture.

### When to Write an ADR

Write an ADR when:
- Introducing a new technology or library into the stack.
- Changing inter-service communication patterns.
- Making a decision that would be difficult to reverse.
- Choosing between two or more viable approaches that others might reasonably question.
- Establishing a new platform-wide standard.

You do not need an ADR for:
- Implementing a feature that follows established patterns.
- Routine dependency upgrades.
- Bug fixes.

If you are unsure, write the ADR. The cost of writing one unnecessarily is low; the cost of missing one is high.

### ADR Format

ADRs are stored in `docs/adr/` and named `NNNN-short-title.md` where `NNNN` is the next sequential number.

```markdown
# ADR-NNNN: Title

**Date:** YYYY-MM-DD
**Status:** Proposed | Accepted | Deprecated | Superseded by ADR-XXXX
**Deciders:** [List of people who participated in the decision]

## Context

Describe the problem, constraints, and forces at play. What is the situation
that requires a decision?

## Decision

State the decision clearly. "We will..." not "We should consider..."

## Consequences

### Positive
- List the benefits and improvements.

### Negative
- List the drawbacks, trade-offs, and risks accepted.

### Neutral
- List notable side effects that are neither good nor bad.

## Alternatives Considered

Brief description of alternatives that were evaluated and why they were not chosen.
```

### ADR Workflow

1. Copy the template: `cp docs/adr/template.md docs/adr/NNNN-your-title.md`
2. Assign the next sequential number. Check existing ADRs to avoid conflicts.
3. Set status to `Proposed`.
4. Open a PR. Tag the Principal Software Architect and the relevant Engineering Manager.
5. After approval and merge, status becomes `Accepted`.
6. If a later ADR supersedes this one, update the status to `Superseded by ADR-XXXX`.

---

## 6. Rust Coding Standards

### Compiler and Linting

All Rust code must compile without warnings and pass Clippy at the deny level:

```rust
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
```

These attributes must appear at the top of `lib.rs` or `main.rs` in every crate. CI will fail if any lint fires.

### Error Handling

Zero use of `unwrap()` or `expect()` in production code paths. This is enforced by Clippy lint `clippy::unwrap_used` set to `deny` in service crates.

- Use `thiserror` for library-level and service-level error types.
- Use `anyhow` for application-level error propagation in binaries.
- Every error variant must have a human-readable message.
- Error types must implement `std::error::Error`.
- Errors that cross service boundaries (gRPC, HTTP) must map to appropriate status codes. Do not leak internal error details to external callers.

```rust
// Correct
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("no available drivers in zone {zone_id}")]
    NoAvailableDrivers { zone_id: ZoneId },

    #[error("driver {driver_id} is not active")]
    DriverNotActive { driver_id: DriverId },

    #[error("database error")]
    Database(#[from] sqlx::Error),
}

// Forbidden in production paths
let driver = pool.get_driver(id).unwrap();
```

### Async Code

- All I/O must be async using Tokio.
- Do not block the async executor. Use `tokio::task::spawn_blocking` for CPU-bound work exceeding approximately 100 microseconds.
- Use structured concurrency. Prefer `tokio::join!` and `tokio::select!` over spawning unconstrained tasks.
- Every spawned task must have its `JoinHandle` handled. Dropped handles that silently swallow panics are not acceptable.

### Database Access

- All queries use `sqlx` with compile-time checked macros (`sqlx::query!`, `sqlx::query_as!`).
- No raw string queries in production code.
- All queries must be analyzed with `EXPLAIN (ANALYZE, BUFFERS)` before merging. No unbounded sequential scans on large tables.
- Database migrations live in `services/<service>/migrations/` and are numbered sequentially. Migrations must be reversible where possible.
- Tenant isolation is enforced at the PostgreSQL layer via Row-Level Security (RLS). All queries operate within a tenant context set on the connection.

### Service Structure

Every service must expose:
- `GET /health` — returns `200 OK` when the process is running.
- `GET /ready` — returns `200 OK` only when all dependencies (database, Redis, etc.) are reachable.
- `GET /metrics` — Prometheus text format metrics.

These endpoints must not require authentication.

### Multi-Tenancy

- Tenant ID is extracted from the JWT claim and stored in the request context via an Axum extension.
- Never accept tenant ID from the request body or query parameters for security-sensitive operations.
- All database queries must include the tenant filter, even when RLS is active, as a defense-in-depth measure.

### Code Organization

```
services/<service-name>/
  src/
    main.rs          # Binary entry point, server setup
    lib.rs           # Crate root, deny attributes
    config.rs        # Configuration types and loading
    error.rs         # Error types
    domain/          # Core business logic, no I/O dependencies
    handlers/        # Axum or Tonic handler functions
    repositories/    # Database access layer
    services/        # Application service layer
  migrations/        # sqlx migrations
  tests/             # Integration tests
  Cargo.toml
```

---

## 7. Frontend Coding Standards

### General

- All frontend code is TypeScript with strict mode enabled (`"strict": true` in `tsconfig.json`).
- No `any` types in committed code. Use `unknown` and narrow appropriately.
- All components must be typed with explicit prop interfaces. No implicit prop types.
- Use `pnpm` for all package management. Do not use `npm` or `yarn`.

### Next.js Portals

- Use the App Router exclusively. Do not add Pages Router routes to existing App Router projects.
- Server Components are the default. Use Client Components (`"use client"`) only when browser APIs, event handlers, or client-side state are required.
- Data fetching happens in Server Components or Server Actions. Do not fetch data in Client Components that could be fetched on the server.
- All pages must export metadata. Do not leave `generateMetadata` unimplemented.

### Design System

All portals use the LogisticOS dark-first glassmorphism design system located at `apps/merchant-portal/src/lib/design-system/` (exported as `@logisticos/ui`).

**Do not:**
- Create inline styles.
- Introduce new color values outside the design token palette.
- Use solid opaque cards where the design system provides glassmorphism panel variants.
- Use browser default focus styles — use the design system's focus ring utilities.

**Do:**
- Use design tokens for all colors, spacing, and typography.
- Use Framer Motion for all state-change animations. Elements must not jump between states without a transition.
- Use `JetBrains Mono` for tracking numbers, AWB codes, and all data identifiers.
- Use the neon glow shadow utilities for active states and alerts.

### Responsive Design

Every frontend change must pass a responsive audit before merging:

- Test at 375px (mobile), 768px (tablet), 1280px (desktop), and 1920px (wide).
- No horizontal scrollbars at any viewport.
- Touch targets must be at least 44x44px on mobile viewports.
- Text must remain readable without zooming at all sizes.

Include screenshots of the affected views at mobile and desktop widths in your PR description.

### Internationalization

All user-visible strings must be wrapped in the i18n translation function from the start. Do not merge hardcoded English strings into production UI components. English and Filipino (Tagalog) are the priority locales.

```tsx
// Correct
const { t } = useTranslation('dispatch');
return <p>{t('driver.assigned', { driverName })}</p>;

// Forbidden
return <p>Driver assigned: {driverName}</p>;
```

---

## 8. Testing Requirements

### Rust Services

| Test type | Requirement |
|-----------|------------|
| Unit tests | Required for all domain logic and pure functions |
| Integration tests | Required for all public HTTP and gRPC endpoints |
| Repository tests | Required for all database query functions (test against real Postgres via Docker) |
| Contract tests | Required for all inter-service gRPC consumers |

Tests are co-located with source in `src/` for unit tests and in `tests/` for integration tests.

Run tests with:

```bash
# All tests in the workspace
cargo nextest run --workspace

# A specific service
cargo nextest run -p logisticos-dispatch
```

Integration tests require a running Docker Compose stack. The CI environment starts this automatically. Locally, run `docker compose up -d` before running integration tests.

Minimum coverage thresholds are enforced in CI:
- Domain logic modules: 90% line coverage.
- Handler and repository modules: 80% line coverage.

### Frontend

| Test type | Requirement |
|-----------|------------|
| Unit tests (Vitest) | Required for utility functions, hooks, and data-transformation logic |
| Component tests (Vitest + Testing Library) | Required for non-trivial components with conditional rendering or user interaction |
| E2E tests (Playwright) | Required for all critical user flows (booking, dispatch, tracking) |

Run frontend tests:

```bash
# Unit and component tests
pnpm test

# E2E tests against a running staging environment
pnpm test:e2e
```

E2E tests run in CI against the staging deployment after every merge to `main`.

### Test Data

- Never use production data in tests.
- Use factory functions and fixtures, not hardcoded IDs.
- Database integration tests must run in isolated schemas or transactions that are rolled back after each test.

---

## 9. Security Guidelines

### Secrets Management

- No secrets, credentials, API keys, or tokens in source code.
- No secrets in `.env` files committed to the repository.
- No secrets in Kubernetes manifests or Helm values files.
- All secrets are stored in and retrieved from HashiCorp Vault at runtime.
- If a secret is accidentally committed, treat it as compromised immediately: rotate it, then contact the security team.

### Input Validation

All input validation occurs at the API boundary before any business logic executes:
- Use the Rust `validator` crate for struct-level validation.
- Validate and sanitize all free-text fields for injection risks.
- Reject requests that exceed defined size limits before deserialization where possible.
- Never trust tenant ID, user ID, or role claims from the request body. Read them from the validated JWT context only.

### PCI-DSS Scope

Payment card data must never appear in non-payment services. If a feature requires payment information:
- Reference a payment token or transaction ID, not card details.
- Route the operation through the `payments` service.
- Do not log payment card numbers, CVVs, or full PANs anywhere in the system.

### GDPR and PDPA Compliance

- Behavioral tracking requires explicit consent, recorded in the CDP consent store.
- All services must support the right-to-erasure workflow. If your service stores personal data, implement the erasure handler defined in `libs/common/src/gdpr.rs`.
- Do not store personal data beyond its retention period. Use the shared data-retention job framework in `libs/common`.
- Personal data fields in database schemas must be annotated with `-- pii` comments for automated scanning.

### Dependencies

- Do not add a new dependency without confirming it has no known critical CVEs.
- Run `cargo audit` locally before opening a PR that adds or upgrades Rust dependencies.
- Run `pnpm audit` for frontend dependency changes.
- CI blocks merges when `cargo audit` or `pnpm audit` report high-severity vulnerabilities.

---

## 10. Pull Request Process

### Before Opening a PR

- [ ] The relevant API spec (OpenAPI or Protobuf) is already merged.
- [ ] An ADR has been created if the change involves an architectural decision.
- [ ] All CI checks pass locally (`cargo clippy`, `cargo nextest run`, `pnpm test`).
- [ ] A responsive audit has been performed for any frontend changes.
- [ ] No secrets appear anywhere in the diff.

### PR Description Template

Every PR must include:

1. **Summary** — What does this change do and why?
2. **Spec reference** — Link to the merged OpenAPI/Protobuf spec PR or ADR.
3. **Testing** — How was this tested? What test cases cover the change?
4. **Screenshots** — Required for any frontend change. Include mobile (375px) and desktop (1280px) views.
5. **Breaking changes** — List any breaking changes and the migration path.
6. **Checklist** — The items above.

### Review Requirements

- Minimum 2 approvals before merge.
- At least one approver must be a Senior Engineer, Staff Engineer, or Principal Architect.
- For changes to shared libraries (`libs/`), the Principal Software Architect must approve.
- For changes to security-sensitive code (identity service, payments service, auth middleware), the CISO or their designated Security Engineer must approve.
- The PR author may not approve their own PR.

### Review Criteria

Reviewers evaluate:
- Correctness and completeness relative to the spec.
- Adherence to the coding standards in this document.
- Test coverage and test quality.
- Error handling and edge cases.
- Performance implications (query plans, N+1 queries, blocking calls).
- Security posture (input validation, secret handling, tenant isolation).
- Breaking changes that are not documented.

### Merge Strategy

- All PRs are merged using **squash and merge**.
- The squash commit message must follow the Conventional Commits format.
- Delete the branch after merge.
- Do not merge a PR while any required CI check is failing or pending.

---

## 11. CI/CD Gates

All gates must pass before a PR can be merged. Bypassing CI gates requires written approval from the Engineering Manager and a documented reason.

### Gate: Lint and Format

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
pnpm lint
pnpm type-check
```

Failure: the PR is blocked. Fix all warnings and formatting issues before requesting re-review.

### Gate: Tests

```
cargo nextest run --workspace
pnpm test
```

Failure: the PR is blocked. All tests must pass.

### Gate: Security Scan

```
cargo audit
pnpm audit --audit-level=high
```

Failure: the PR is blocked. Any high or critical vulnerability in the dependency graph must be resolved before merge. Medium vulnerabilities generate a warning and must be triaged.

### Gate: Performance Regression

Automated performance benchmarks run against the staging environment after each merge to `main`. If a benchmark regresses by more than 15% from the baseline, an alert is raised and the relevant team lead is notified. Benchmarks are defined in `tests/benchmarks/` per service.

### Gate: E2E Tests

Playwright E2E tests run against the staging environment after merge to `main`. Failures create a P1 incident and must be resolved before the next production deploy.

### Deployment Pipeline

```
PR merged to main
       |
       v
Build + unit tests (< 5 min)
       |
       v
Docker image build and push
       |
       v
Staging deploy (Kubernetes rolling update)
       |
       v
E2E test suite against staging
       |
       v
Production deploy (Istio canary: 5% -> 25% -> 100%)
       |
       v
Automated smoke tests against production canary
       |
       v
Full rollout or automatic rollback on failure
```

Production canary rollouts are monitored for error rate, P99 latency, and business metrics for 30 minutes at each traffic split stage before advancing.

---

## 12. Performance Standards

All production services must meet the following thresholds. Exceeding a threshold is a blocker for that service's release.

| Metric | Threshold |
|--------|-----------|
| P99 API latency (operational endpoints) | < 200ms |
| P99 dispatch driver assignment | < 500ms |
| Live tracking update propagation | < 2 seconds end-to-end |
| Notification delivery (WhatsApp/SMS) | < 5 seconds from trigger event |

### Database Query Requirements

- Every new or modified query must be accompanied by an `EXPLAIN (ANALYZE, BUFFERS)` output in the PR description when it runs against a table with more than 10,000 expected rows.
- Sequential scans on large tables are not permitted in production query paths.
- Foreign key columns must be indexed unless there is a documented justification.
- Queries that return unbounded result sets must use cursor-based pagination.

### Profiling

When a performance regression is reported:
1. Reproduce the regression with `cargo bench` or by replaying the workload against staging.
2. Profile with `perf` (Linux) or `cargo flamegraph`.
3. Attach the flamegraph to the fix PR.

---

## 13. Accessibility Requirements

All web portals must meet WCAG 2.1 Level AA as a minimum. Accessibility failures are treated as bugs, not enhancements, and are prioritized accordingly.

### Requirements

- All interactive elements must be keyboard-navigable. No mouse-only interactions.
- Focus order must follow the visual reading order.
- All form inputs must have associated `<label>` elements. Do not use `placeholder` as a substitute for a label.
- All images must have descriptive `alt` attributes. Decorative images use `alt=""`.
- Color must not be the sole means of conveying information (e.g., error states must use icon or text in addition to color).
- Minimum contrast ratio of 4.5:1 for normal text and 3:1 for large text against the background.
- All custom interactive components (modals, dropdowns, date pickers) must implement the appropriate ARIA roles and keyboard patterns as specified in the WAI-ARIA Authoring Practices Guide.
- Animations that involve motion must respect the `prefers-reduced-motion` media query.

### Automated Checks

Axe-core runs as part of the Playwright E2E suite. Any axe violation at the `critical` or `serious` level fails the E2E gate and blocks deployment.

### Manual Review

Every new page or significantly modified view must be tested:
- With a screen reader (NVDA + Firefox on Windows, VoiceOver + Safari on macOS).
- Using keyboard navigation only (no mouse).
- At 200% browser zoom.

Record the results in the PR description under a dedicated Accessibility section.

---

## Questions and Escalations

For questions about this guide, open a discussion in the engineering Slack channel. For architectural questions, bring them to the weekly Architecture Review. For urgent security concerns, contact the security team directly and do not post details publicly.

This document is maintained by the Principal Software Architect and updated as the project evolves. Proposed changes to contributing guidelines follow the same PR process as code changes.
