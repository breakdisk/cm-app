# Neighborhood Delivery Network Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let OmniDeliv vendors earn revenue by delivering sealed LogisticOS parcels in their neighborhoods, with every parcel and every dirham traceable to a recorded scan.

**Architecture:** A vendor links to a carrier profile. Hub-ops owns territories, waves, totes and custody. Dispatch owns store jobs and batches, and payments owns a separate append-only partner ledger. No new microservice, portal or mobile app is created.

**Tech Stack:** Rust (Axum, SQLx, Tokio), PostgreSQL + PostGIS, Kafka, Next.js 14, Mapbox GL + Terra Draw, Kotlin Android.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md). **Decision record:** [ADR-0018](../../adr/0018-neighborhood-retailer-delivery-network.md).

## How this plan is organized

The work is six plans, executed in order. Each ends on a gate and leaves working software. A phase does not start until the previous gate has passed.

| Phase | Plan | Tasks | Delivers | Gate |
|---|---|---|---|---|
| 0 | [Partner safety](2026-11-23-neighborhood-network-phase-0-partner-safety.md) | 17 | Closes live data exposure, frees riders after a job, geocoding quality, translations | Partner tokens are isolated, a rider can take a second job, domain and PIN work |
| 1 | [Enrollment and shadow](2026-11-23-neighborhood-network-phase-1-enrollment-shadow.md) | 14 | Opt-in, approval, territories, resolver on real parcels | Enough eligible parcels per territory to meet the economics |
| 2 | [Totes and custody](2026-11-23-neighborhood-network-phase-2-totes-custody.md) | 15 | Waves, seals, labels, transfer, store receipt | Full tote cycle reconciles to zero unexplained pieces |
| 3 | [Store delivery](2026-11-23-neighborhood-network-phase-3-store-delivery.md) | 15 | Batches, stop order, retries, returns, recall | Real-device run, no stolen parcels, no gig pay |
| 4 | [Partner money](2026-11-23-neighborhood-network-phase-4-partner-money.md) | 8 | Ledger, COD liability, settlements | Replaying events changes nothing, every line traces |
| 5 | [Pilot](2026-11-23-neighborhood-network-phase-5-pilot.md) | 6 | 3 to 5 stores against a control zone | Section 11 exit criteria |
| 6 | Below | Not task-planned | Expansion | Planned from pilot evidence |

Phase 0 is worth doing even if the network never ships. **Task 0.1 should ship first, by itself**, because it closes an exposure that is live today.

## Global Constraints

Each phase plan repeats the constraints that apply to it. These apply everywhere:

- Base every branch on current `origin/master` in a fresh worktree. The local `feat/field-ops/delivery-exceptions` checkout is stale. Branch names follow `feat/neighborhood/phase-N-<name>`.
- Load-bearing invariant: every parcel has exactly one accountable party at every moment, and that party changes only on a recorded two-sided scan.
- Tenant and partner identity always come from the JWT. A store never supplies `partner_id`.
- New routes use `/v1`, reuse `PaginatedResponse` and the `AppError` body, and go into the OpenAPI file before implementation.
- New Kafka topics go in both topic scripts and are verified on the broker before a consumer depends on them. New payload fields use `#[serde(default)]`.
- Zero `unwrap()` in production paths. `cargo clippy -p <crate> --all-targets -- -D warnings` must pass.
- DB-backed tests use the `database_url()` pattern from `services/field-ops/tests/claim_race.rs`. A DB test reporting `0.00s` did not run.
- Frontend gate is `npm run lint && npm run typecheck && npm run build`. `tsc --noEmit` alone is not enough.
- New screens: dark-first, WCAG 2.1 AA, externalized strings, RTL-ready, with screenshots at 375, 768 and 1280 px.
- Migrations are tested on a disposable database. Nothing runs on the VPS, deploys, or merges without the owner's go-ahead.
- Set `CARGO_INCREMENTAL=0`. Disks on the dev machine are near full.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

Across the whole program, these are the likeliest failures the spec implies but does not spell out. Each has a test in its owning task.

1. A store rider's `driver` role carries tenant-wide read access (Tasks 0.7, 0.8).
2. An accepted assignment closes before its tasks arrive, freeing a rider who has work (Task 0.12).
3. A late `DELIVERY_ATTEMPTED` reopens a shipment that has already failed (Task 3.7).
4. An offline scan replays after the tote has moved on and pushes state backward (Task 2.4).
5. Partner COD on an AED tenant is recorded as PHP (Task 4.4).

---

## Changes this plan makes to the approved design

Planning against `origin/master` turned up the following. Each is built into the phase plans.

| # | Finding or change | Where | Why |
|---|---|---|---|
| 1 | **`GET /v1/shipments` has no owner filter.** Any merchant or customer token can list the whole tenant, including recipient contact details | Task 0.1 | Live exposure, wider than the partner gap in the design |
| 2 | Store riders hold `driver`, with tenant-wide shipment and queue reads. Riders get a membership and `pid` too | Tasks 0.3, 0.7, 0.8, 3.2 | Otherwise any store rider could read the tenant |
| 3 | Partners also see tenant-wide KPIs through `analytics:view` | Task 0.9 | Same class as the wallet exposure |
| 4 | Order-intake has no shipment-to-carrier link. Added a local projection and backfill | Task 0.6 | Scoping needs something to filter on |
| 5 | The Payouts page stops showing the tenant wallet in Phase 0 and shows a neutral state until the ledger arrives in Phase 4 | Task 0.9 | The design's order would leave partners reading company money until Phase 4 |
| 6 | One active partner membership per user | Task 0.4 | Otherwise token issue must guess which store a user acts for |
| 7 | Closing an assignment also requires that its tasks have arrived | Task 0.12 | "No open tasks" is true before the first task is consumed |
| 8 | Reservation moves from Phase 3 to Phase 2 | Task 2.2 | A bagged parcel must already be out of reach of auto-assigners |
| 9 | `TOTES_ADMIN` permission and `transfer_driver` role | Task 2.1 | The design named the actions but no role for them |
| 10 | New topic `logisticos.partner.cod_balance.changed` | Task 4.5 | Hub-ops needs the store's COD position without reading payments tables |
| 11 | `partner_settlement_lines` replaces the ledger's `settlement_id` column | Task 4.6 | Writing that column would break the ledger's append-only rule |
| 12 | Store rates travel on `partner.store.updated` | Task 1.5 | Sorting must not depend on a synchronous call to carrier |

---

## Phase 6: Expansion

Phase 6 is deliberately not broken into tasks. Each item depends on what the pilot shows, so a task list written today would be guesswork. Each gets its own design check and plan when its entry condition is met.

| Work | Entry condition |
|---|---|
| COD store by store | Clean pilot custody record, deposit in place, store's COD cap set |
| More zones | Pilot exit criteria met in the first zone |
| Non-OmniDeliv retailers via `origin_product` | Commercial agreement. No schema change should be needed |
| Automated payouts | Manual settlements have run cleanly, and Finance and PCI scope are reviewed |
| VRP solver behind `StopSequencer` | Measured route cost shows nearest-neighbor is the limiting factor |
| Industrial label printing and ZPL | Label volume justifies it |
| Enforcing timers | Observe-only baseline reviewed and thresholds tuned |

---

## Open owner decisions

These block a specific point in the plan and nothing before it.

| Decision | Needed by | Default |
|---|---|---|
| Mapbox permanent geocoding, terms and cost | Task 0.14 ships with it off. Needed by Task 5.3 | Enable once confirmed |
| Store delivery and failed-attempt fees | Phase 1 gate | Needs finance's current AED cost per parcel |
| Pilot languages | Task 5.3 | English and Arabic for customers, English for staff |
| COD in the pilot | Task 5.3 | Prepaid only |
| Security deposit | Before any store gets COD | None during prepaid |
| Legal review: store agreement, rider permits, UAE data sharing | Task 5.3 | Required |
| `cargomarket.net` registrar hold, Twilio | Phase 0 gate | Required |
