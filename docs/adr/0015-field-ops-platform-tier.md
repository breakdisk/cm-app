# ADR-0015: Field-Ops Platform Tier — Minimal Extraction

**Status:** Proposed
**Date:** 2026-08-06
**Deciders:** Principal Architect, Senior Rust Engineer — Driver Operations, Engineering Manager — Logistics Domain, Product Manager — Platform

> **Load-bearing invariant.** A courier is claimed by exactly one assignment at a time. Two products racing to dispatch the same courier must resolve to one winner and one explicit loser — never two accepted assignments.

---

## Context

ADR-0009 established two rules that now collide:

1. **Boundary rule 2:** product services may not call other products' services directly.
2. **"Watch the field-ops cluster":** LogisticOS, Ride-Hailing and Food Delivery share courier identity, GPS ingest, geospatial dispatch, ETA and earnings. *"When the second of these products goes live, extract these into a `field-ops` platform tier rather than copying them."*

OmniDeliv AI is that second product. It needs couriers; every courier capability today lives inside LogisticOS's product tier (`services/driver-ops`, `services/dispatch`). Three options, and only one honours both rules.

## Decision

Extract a **minimal** field-ops platform tier — only what a second field-ops product needs to operate a courier:

| Extracted to `services/field-ops` | Stays in LogisticOS |
|---|---|
| Courier identity (the human in the field) | POD / POP capture |
| Assignment + atomic claim | Hub operations, cross-dock |
| GPS ingest and breadcrumbs | Carrier and sub-carrier contracts |
| Earnings ledger (deferred — see below) | Parcel-specific routing and manifests |

Deliberately **not** extracted: ETA prediction, geospatial route optimisation, in-app navigation. Those are still single-consumer; ADR-0009 rule 3 says a service earns platform status only when a second product needs it, and premature platformisation is the second-worst trap after the mega-gateway.

**The earnings ledger is extracted in a later phase**, once OmniDeliv's three-leg settlement model exists. Building it before the order model would mean guessing at the shape.

## Consequences

### Positive
- OmniDeliv gets couriers without breaching boundary rule 2.
- The extraction the ADR-0009 authors predicted happens at the moment they specified, at minimum scope.
- Courier claim becomes atomic and cross-product, which `driver_ops` never needed and therefore never had.

### Negative — stated plainly, not hidden
- **Two courier tables coexist** (`driver_ops.drivers` and `field_ops.couriers`) until LogisticOS migrates. This is precisely the duplication ADR-0009 rule 4 warns against, and it is only acceptable as a *dated* transitional state.
- **Commitment:** LogisticOS migrates onto `field-ops` within two quarters of OmniDeliv slice one reaching production. If that date passes with both live, this ADR has failed and should be revisited — not silently extended.

### Neutral
- Tenant isolation in the new service is application-layer (`WHERE tenant_id = $n`), matching how every other service in this repo actually behaves. See the RLS note in the implementation plan; making RLS genuinely enforce needs its own ADR.

## Alternatives Considered

### Alternative 1: Temporary documented exception — OmniDeliv calls LogisticOS directly
**Rejected.** Fastest to the hero flow and zero risk to production dispatch, but it is a known boundary breach with no forcing function to remove it. Boundary exceptions of this kind historically become permanent; the ADR-0009 authors wrote rule 2 specifically to prevent this shape.

### Alternative 2: OmniDeliv builds its own thin courier module
**Rejected.** No boundary violation and full independence, but it produces two courier systems, two driver-facing apps (or one confused one), and duplicated earnings logic — exactly the copy rule 4 forbids. It also makes the eventual convergence strictly harder than doing it now.

### Alternative 3: Full field-ops extraction (dispatch, ETA, navigation, earnings)
**Rejected for now.** Architecturally the endgame, but it is a multi-quarter programme touching production dispatch, and it would block OmniDeliv slice one entirely. Rule 3 also argues against extracting single-consumer capabilities. Revisit when a third field-ops product appears.

## References
- ADR-0009: Multi-Product Platform Gateway Topology
- ADR-0012: Schema-Isolated SQLx Migrations
- `docs/superpowers/specs/2026-08-06-omnideliv-ai-design.md` §3.1
