# Neighborhood Network Phase 4: Partner Money Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pay stores from an append-only ledger in which every line traces to a custody event and replaying events changes nothing.

**Architecture:** Payments projects partner jobs from Kafka and posts idempotent entries to `partner_ledger_entries`, split into `earnings` and `cod_held`. Settlements are a finance-approved view over those entries, and payouts are recorded manually.

**Tech Stack:** Rust, Axum, SQLx, PostgreSQL, Kafka, Next.js 14.

**Spec:** [Neighborhood Delivery Network](../specs/2026-11-23-neighborhood-delivery-network-design.md), section 7 (Payments). **Program plan:** [index](2026-11-23-neighborhood-delivery-network.md).

## Global Constraints

- **Entry condition:** the Phase 3 gate has passed.
- Next migrations: payments `0022`.
- Money is integer `*_cents` plus an ISO 4217 `currency` from the tenant. Never hard-code a currency. The first pilot settles in **AED**.
- `partner_ledger_entries` is append-only: `REVOKE UPDATE, DELETE` for the app role. Corrections are new entries.
- Idempotency keys are exactly: `fee:{shipment}`, `failfee:{shipment}:{attempt}`, `cod:{shipment}`, `remit:{reference}`, `case:{case_id}`, `adj:{id}`, `payout:{settlement}`.
- One fee per delivered shipment, however many attempts it took.
- A failed attempt pays only with valid evidence.
- `earnings` and `cod_held` are reported separately. Netting COD against a payout is an explicit settlement line.
- Store riders create **no** `earning_credits` and **no** `DriverLedger` COD debt.
- This ledger is independent of `vendor_ledger`, `DriverLedger` and tenant wallets.
- `COD_COLLECTED` carries `CodReconciled`, not the shared `CodCollected`.
- Finance executes payouts manually. No automatic bank transfer is built.

## Review Focus

1. `cod_service.rs` builds COD as `Currency::PHP`. A partner COD on an AED tenant would be recorded in the wrong currency (Task 4.4).
2. `job.changed` for `delivered` arrives before the `failed` from an earlier attempt. Both must post once, with correct totals (Task 4.3).
3. A delivery is reversed by a case after the fee was already included in a paid settlement. The correction lands in the next settlement (Task 4.6).
4. A store's COD balance projection is missing or stale. It must receive no COD parcels (Task 4.5).
5. The same person drafts and approves a settlement. It must be refused (Task 4.6).

---

### Task 4.1: Ledger schema and repository

**Files:**
- Create: `services/payments/migrations/0022_partner_ledger.sql`
- Create: `services/payments/src/domain/entities/partner_ledger.rs`
- Create: `services/payments/src/infrastructure/db/partner_ledger_repo.rs`
- Modify: `docs/api/openapi/payments.yaml`, `scripts/openapi-tracked-prefixes.txt`, `services/api-gateway/src/proxy/mod.rs`
- Test: `services/payments/tests/partner_ledger_db.rs`

**Interfaces:**
- Produces:

```rust
pub enum LedgerBucket { Earnings, CodHeld }
pub enum PartnerEntryType { DeliveryFee, FailedAttemptFee, CodCollected, CodRemitted, Shortage, Damage, SlaPenalty, Adjustment, Payout }
pub struct NewPartnerEntry { /* every column of the spec's table except id, recorded_at, settlement_id */ }
pub trait PartnerLedgerRepository {
    async fn post(&self, e: &NewPartnerEntry) -> anyhow::Result<PostOutcome>; // Posted | Duplicate
    async fn post_in_tx(&self, tx: &mut Transaction<'_, Postgres>, e: &NewPartnerEntry) -> anyhow::Result<PostOutcome>;
    async fn balances(&self, tenant: Uuid, carrier: Uuid) -> anyhow::Result<PartnerBalances>;
    async fn list(&self, tenant: Uuid, carrier: Uuid, p: Pagination) -> anyhow::Result<PaginatedResponse<PartnerEntry>>;
}
```

- Also creates `payments.partner_job_refs(shipment_id PK, tenant_id, carrier_id, status, attempt_no, rate_id, delivery_fee_cents, failed_attempt_fee_cents, cod_amount_cents, currency, updated_at)`.

- [ ] **Step 1: Write failing tests:** `same_key_posts_once`, `balances_are_separate_by_bucket`, `app_role_cannot_update_or_delete`, `mixed_currency_for_one_store_is_refused`, `concurrent_posts_with_one_key_have_one_winner`, gateway routes for `/v1/partner-ledger` and `/v1/partner-settlements`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-payments --test partner_ledger_db` → FAIL.
- [ ] **Step 3: Implement** with `INSERT ... ON CONFLICT (idempotency_key) DO NOTHING`. Leave the spec's `settlement_id` column out, because Task 4.6 links entries through `partner_settlement_lines` and the ledger stays strictly append-only.
- [ ] **Step 4: Confirm they pass.** Same command → PASS, non-zero duration.
- [ ] **Step 5: Commit.** `feat(payments): append-only partner ledger`

---

### Task 4.2: Partner job projection

**Files:**
- Create: `services/payments/src/infrastructure/messaging/partner_job_consumer.rs`
- Modify: `services/payments/src/infrastructure/messaging/mod.rs`, `src/bootstrap.rs`

**Interfaces:**
- Consumes: `PartnerJobChanged`.
- Produces: `PartnerJobRefRepository::{upsert, find(shipment_id)}`.

- [ ] **Step 1: Write failing tests:** `job_is_projected`, `older_event_does_not_overwrite_newer_status`, `rate_snapshot_is_never_changed_by_a_later_event`, `unknown_status_is_stored_and_posts_nothing`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-payments --test partner_ledger_db projection` → FAIL.
- [ ] **Step 3: Implement** with the retry-in-place consumer shape.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(payments): project partner jobs locally`

---

### Task 4.3: Fee and case postings

**Files:**
- Create: `services/payments/src/application/services/partner_posting_service.rs`
- Create: `services/payments/src/infrastructure/messaging/partner_discrepancy_consumer.rs`

**Interfaces:**
- Produces: `pub fn entries_for_job_event(e: &PartnerJobChanged) -> Vec<NewPartnerEntry>` (pure), and `entry_for_resolved_case(e: &PartnerDiscrepancyResolved) -> Option<NewPartnerEntry>`.

- [ ] **Step 1: Write failing tests:**
  - `delivered_posts_one_fee`, `delivered_after_a_failed_attempt_posts_fee_and_one_failfee`
  - `failed_attempt_with_valid_evidence_posts_failfee`, `without_evidence_posts_nothing`
  - `events_in_either_order_give_the_same_balance`
  - `case_with_store_liability_debits_earnings`, `case_with_platform_liability_posts_nothing`
  - `cancelled_job_posts_no_delivery_fee`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-payments --lib partner_posting` → FAIL.
- [ ] **Step 3: Implement** the pure functions and thin consumers.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(payments): post store fees and resolved cases`

---

### Task 4.4: COD in the store's ledger

**Files:**
- Modify: `services/payments/src/application/services/cod_service.rs`, `src/domain/repositories/mod.rs:44`
- Modify: the Postgres `CodRepository` implementation
- Modify: `services/payments/src/infrastructure/messaging/pod_consumer.rs`
- Test: `services/payments/tests/partner_cod_db.rs`

**Interfaces:**
- Produces: `CodRepository::save_with_partner_entry(&self, cod: &CodCollection, entry: Option<&NewPartnerEntry>) -> anyhow::Result<()>`, one transaction.

- [ ] **Step 1: Write failing tests:**
  - `partner_cod_posts_cod_held_in_the_same_transaction`
  - `failed_ledger_insert_rolls_back_the_cod_row`
  - `partner_cod_uses_the_job_currency_aed`
  - `partner_cod_creates_no_driver_ledger_debt`
  - `replayed_pod_posts_once`
  - `non_partner_cod_is_unchanged`
  - `partner_job_with_no_currency_is_refused`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-payments --test partner_cod_db` → FAIL.
- [ ] **Step 3: Implement.** Look up `partner_job_refs` before building `Money`. A partner job's currency parses to `Currency`, and an unknown value is an error, not a default.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(payments): collected cash becomes the store's liability`

---

### Task 4.5: COD cap at sort

**Files:**
- Modify: `libs/events/src/topics.rs`, `libs/events/src/payloads.rs`, `scripts/kafka/create-topics.sh`, `scripts/create-kafka-topics.sh`
- Modify: `services/payments/src/application/services/partner_posting_service.rs`
- Create: `services/hub-ops/migrations/0025_partner_cod_balance.sql`, `src/infrastructure/partner_cod_consumer.rs`
- Modify: `services/hub-ops/src/application/services/tote_sort_service.rs`

**Interfaces:**
- Produces topic `logisticos.partner.cod_balance.changed`, event `PartnerCodBalanceChanged { tenant_id, carrier_id, cod_held_cents, currency, version }`. This topic is an addition to the spec's table.

- [ ] **Step 1: Write failing tests:** `store_at_cap_gets_no_cod_parcel`, `remittance_restores_headroom`, `missing_balance_means_no_cod`, `older_version_is_ignored`, `prepaid_parcel_is_unaffected`, `held_plus_reserved_cannot_exceed_cap`.
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-hub-ops --test tote_sort_db cod` → FAIL.
- [ ] **Step 3: Implement.** The headroom check is `cap - cod_held - reserved_cod_cents >= parcel_cod`.
- [ ] **Step 4: Confirm they pass.** Same command and `bash scripts/check-kafka-topics.sh` → PASS.
- [ ] **Step 5: Commit.** `feat(network): stores at their COD cap receive no COD parcels`

---

### Task 4.6: Settlements and remittance

**Files:**
- Create: `services/payments/migrations/0023_partner_settlements.sql`
- Create: `services/payments/src/application/services/partner_settlement_service.rs`
- Create: `services/payments/src/api/http/partner_ledger.rs`
- Test: `services/payments/tests/partner_settlement_db.rs`

**Interfaces:**
- Produces: `GET /v1/partner-ledger/me`; `GET, POST /v1/partner-settlements`; `POST /v1/partner-settlements/{id}/approve`; `POST /v1/partner-settlements/{id}/mark-paid`, body `{ bank_reference, expected_version }`; `POST /v1/partner-remittances`.
- `partner_settlements(id, tenant_id, carrier_id, period_from, period_to, earnings_cents, deductions_cents, cod_netted_cents, net_cents, currency, status, created_by, approved_by, bank_reference, paid_at, version)`.

- [ ] **Step 1: Write failing tests:**
  - `settlement_claims_each_entry_once`, `overlapping_settlements_cannot_claim_one_entry`
  - `cod_netting_is_an_explicit_line`
  - `creator_cannot_approve`
  - `mark_paid_posts_one_payout`, `mark_paid_twice_posts_once`
  - `negative_net_carries_forward_and_pays_nothing`
  - `reversal_after_paid_lands_in_the_next_settlement`
  - `store_sees_only_its_own_ledger`, and the carrier always comes from `pid`
  - `partner_cannot_call_finance_routes`
  - `amounts_are_returned_with_currency`
- [ ] **Step 2: Confirm they fail.** `cargo test -p logisticos-payments --test partner_settlement_db` → FAIL.
- [ ] **Step 3: Implement.** Create `partner_settlement_lines(settlement_id, entry_id UNIQUE)`, whose unique `entry_id` is what stops two settlements claiming one entry.
- [ ] **Step 4: Confirm they pass.** Same command → PASS.
- [ ] **Step 5: Commit.** `feat(payments): partner settlements with separated duties`

---

### Task 4.7: Store earnings and Admin settlement run

**Files:**
- Modify: `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx`
- Create: `apps/partner-portal/src/app/(dashboard)/store/cash/page.tsx`
- Create: `apps/admin-portal/src/app/(dashboard)/neighborhood-network/settlements/page.tsx`

**Interfaces:**
- Consumes: Task 4.6 routes through generated types.

- [ ] **Step 1: Write failing jest test:** `formats_cents_with_the_response_currency`, `earnings_and_cash_are_never_summed`.
- [ ] **Step 2: Confirm it fails.** `npm test -- ledger` in `apps/partner-portal` → FAIL.
- [ ] **Step 3: Implement** statements, cash to remit, and the Admin draft, approve and mark-paid flow with a confirmation step.
- [ ] **Step 4: Verify.** `npm run lint && npm run typecheck && npm test && npm run build` per portal, with screenshots at 375, 768 and 1280 px in `en` and `ar`.
- [ ] **Step 5: Commit.** `feat(portals): store statements and settlement run`

---

### Task 4.8: Replay proof and gate

**Files:**
- Create: `services/payments/tests/partner_ledger_replay.rs`, `scripts/reports/partner-ledger-trace.sql`

- [ ] **Step 1: Write the test.** Generate a seeded mixed event stream, apply it, record balances, apply every event again in shuffled order, and assert identical balances and entry counts.
- [ ] **Step 2: Write the trace query.** Every statement line must join to a custody event, a resolved case, or a finance reference. It returns orphan lines.
- [ ] **Step 3: Run both.** Expected: test passes, query returns zero rows.
- [ ] **Step 4: Commit.** `test(payments): replaying partner events changes nothing`
- [ ] **Step 5: Gate.** Replaying every event twice yields identical balances, and each statement line traces to a custody event.
