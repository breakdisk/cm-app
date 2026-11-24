# Neighborhood Network Phase 5: Pilot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run 3 to 5 stores in one zone against a comparable control zone, and decide on evidence whether to expand.

**Architecture:** A per-hub mode flag turns the network on for new sorts only. Parcels already in totes finish on their existing path. A read-only report compares the pilot and control zones.

**Tech Stack:** Rust, SQL reporting, MCP tools in `ai-layer`, runbooks.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), sections 10 and 11. **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- **Entry condition:** the Phase 4 gate has passed and every item in Task 5.3 is confirmed.
- Scope is 3 to 5 stores, one zone, **prepaid parcels only** unless the owner changes that default.
- A security deposit is required before any store is enabled for COD.
- Exit targets are the section 11 table, unchanged.
- Timers stay observe-only until the pilot produces a baseline.
- Remote MCP exposes **read-only** tools only until per-tool RBAC exists.
- Every production change in this phase needs the owner's go-ahead.

## Review Focus

1. The kill switch is used with totes in the field. Those parcels must finish, and no new parcels may enter (Task 5.1).
2. The control zone differs in volume or service mix, so a good result is an artifact. The report must show both (Task 5.2).
3. A remote MCP caller asks for another tenant's tote (Task 5.4).
4. A store is suspended mid-wave with parcels in hand (Task 5.5).
5. A phone is offline through an entire wave, then syncs (Task 5.5).

---

### Task 5.1: Live mode and kill switch

**Files:**
- Modify: `services/hub-ops/src/application/services/tote_sort_service.rs`
- Create: `services/hub-ops/src/api/http/network_config.rs`
- Modify: `apps/admin-portal/src/app/(dashboard)/neighborhood-network/page.tsx`

**Interfaces:**
- Produces: `GET, PUT /v1/network-config/{hub_id}` (`TOTES_ADMIN`), audit-logged.

- [ ] **Step 1: Write failing tests:** `off_sends_new_parcels_to_fallback`, `off_leaves_open_totes_flowing`, `a_second_hub_is_unaffected`, `change_is_audited_with_actor_and_reason`, `store_token_cannot_change_mode`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_sort_db mode` → FAIL.
- [ ] **Step 3: Implement** the route and an Admin control with a confirmation that states how many open totes are in flight.
- [ ] **Step 4: Confirm they pass.** Same command, plus `npm run lint && npm run build` in `apps/admin-portal`.
- [ ] **Step 5: Commit.** `feat(network): per-hub live mode with a kill switch`

---

### Task 5.2: Pilot scorecard

**Files:**
- Create: `scripts/reports/pilot-scorecard.sql`
- Create: `apps/admin-portal/src/app/(dashboard)/neighborhood-network/scorecards/page.tsx`

**Interfaces:**
- Output: one row per zone per week with every measure in the section 11 table, plus volume and service mix.

- [ ] **Step 1: Seed** a dataset with hand-computed values for both zones.
- [ ] **Step 2: Write the report** and assert it matches those values. SLA measures use `COALESCE(device_timestamp, server_timestamp)`.
- [ ] **Step 3: Build the page** with both zones side by side, and a visible warning when the zones' volume differs by more than 25%.
- [ ] **Step 4: Verify.** `npm run lint && npm run typecheck && npm run build`, with screenshots at 375, 768 and 1280 px.
- [ ] **Step 5: Commit.** `feat(network): pilot scorecard against a control zone`

---

### Task 5.3: Launch preconditions

**Files:**
- Create: `docs/runbooks/neighborhood-pilot-readiness.md`

- [ ] **Step 1: Write the checklist**, with an owner and evidence link per line:
  - `cargomarket.net` reachable, and Twilio PIN delivery tested.
  - Mapbox permanent geocoding terms and cost confirmed, flag set.
  - Compliance and Legal have signed off on the store agreement, rider status and permits, and UAE data sharing.
  - Delivery and failed-attempt fees set from finance's AED cost per parcel.
  - Pilot languages confirmed.
  - Seals, totes, labels and a thermal printer on site.
  - FCM configured, or a documented decision that the dashboard is the only alert channel.
  - All new topics verified on the broker with `--members --verbose`.
  - Database backups taken for each service with a new migration.
- [ ] **Step 2: Review it with the owner** and record each confirmation.
- [ ] **Step 3: Commit.** `docs(network): pilot readiness checklist`

---

### Task 5.4: Read-only MCP tools

**Files:**
- Modify: `services/ai-layer/src/infrastructure/tools/mod.rs`
- Create: `services/ai-layer/src/infrastructure/tools/neighborhood.rs`
- Create in hub-ops: `GET /v1/internal/network/totes/{id}`, `GET /v1/internal/network/capacity?hub_id=`, `GET /v1/internal/network/territories/{id}/performance`, each with a required `tenant_id` query

**Interfaces:**
- Produces tools: `get_tote_status`, `get_partner_capacity`, `get_territory_performance`.

- [ ] **Step 1: Write failing tests:** `tools_are_registered`, `tenant_id_is_overwritten_from_the_jwt`, `other_tenants_tote_returns_not_found`, `no_recipient_fields_in_any_tool_output`, `no_mutating_tool_is_registered_for_totes`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-ai-layer --lib neighborhood` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(ai-layer): read-only neighborhood network tools`

---

### Task 5.5: Failure drills

**Files:**
- Create: `docs/runbooks/neighborhood-incidents.md`

- [ ] **Step 1: Write a runbook entry** for each row in spec section 9.
- [ ] **Step 2: Run these drills on staging:** dispatch down during sort; store closed at drop-off; store suspended with parcels in hand; rider silent mid-batch; phone offline for a full wave; broken seal; kill switch with totes in the field.
- [ ] **Step 3: Record each result.** Expected: custody reconciliation and ledger trace both return zero rows after every drill.
- [ ] **Step 4: Commit.** `docs(network): incident runbooks and drill results`

---

### Task 5.6: Run the pilot

- [ ] **Step 1:** With the owner's go-ahead, enroll 3 to 5 stores, publish the zone, and choose the control zone before launch.
- [ ] **Step 2:** Set the pilot hub to `live`, prepaid only.
- [ ] **Step 3:** Review custody and ledger exceptions daily, and the scorecard weekly.
- [ ] **Step 4:** Tune timer values from observe-only data and record the changes.
- [ ] **Step 5: Gate.** All section 11 exit targets are met:
  - On-time delivery and first-attempt success at or above control.
  - Cost per delivered parcel below control.
  - Zero unexplained parcel loss open beyond 48 hours.
  - Fallback rate stable or falling.
  - Every store willing to continue.
  - Complaints per 1,000 at or below control.
