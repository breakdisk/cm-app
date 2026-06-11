# Consolidation End-to-End Integration

**Date:** 2026-06-03  
**Status:** Approved  
**Services affected:** `hub-ops` (Rust backend), `admin-portal` (Next.js)

---

## Problem

The consolidation system (3D bin-packing, `TruckSpec`, `ConsolidationPlan`) exists and is wired to the admin portal 3D viewer, but the plan is advisory only — there is no link to the container lifecycle and no physical loading verification. Ops has no way to confirm a plan, no container is created, and the `box_scanned` WebSocket event the frontend already listens for is never emitted.

---

## Scope

This spec covers the two missing integration layers:

1. **Confirm flow** — ops confirms a plan in the portal; a container is created and linked to the plan.
2. **Scan verification flow** — hub staff scan each piece against the confirmed plan; off-plan scans are hard-blocked; the container is auto-finalised when all pieces are loaded.

Not in scope: multi-vehicle plan splitting, zone-based auto-grouping, reoptimisation on scan failure.

---

## Plan State Machine

```
draft ──► confirmed ──► loaded
```

| Status | Meaning |
|--------|---------|
| `draft` | Plan is advisory. No container. Ops can re-optimise freely. |
| `confirmed` | Ops confirmed the plan. Container created and linked via `container_id`. Physical scanning may begin. |
| `loaded` | Every AWB in `plan.placements` has a `consolidation_plan_loadings` row. Container auto-finalised (`manifested`). |

The existing `container_id: Option<Uuid>` on `ConsolidationPlan` is set at confirmation time. All plans today default to `draft`.

---

## Data Model

### Migration `0012_consolidation_status_and_loadings.sql`

```sql
-- Add status to existing plans table (all existing rows default to 'draft').
ALTER TABLE hub_ops.consolidation_plans
  ADD COLUMN status TEXT NOT NULL DEFAULT 'draft';

-- Per-scan audit table: one row per physical piece loaded against a plan.
CREATE TABLE hub_ops.consolidation_plan_loadings (
  id         UUID        NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
  tenant_id  UUID        NOT NULL,
  plan_id    UUID        NOT NULL REFERENCES hub_ops.consolidation_plans(id),
  awb        TEXT        NOT NULL,
  scanned_by UUID,
  scanned_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (plan_id, awb)
);
CREATE INDEX ON hub_ops.consolidation_plan_loadings (plan_id);
```

The `UNIQUE(plan_id, awb)` constraint makes duplicate scan inserts fail at the DB level, preventing double-counting under concurrent scans.

---

## Backend

### New routes (added to `bootstrap.rs`)

#### `POST /v1/consolidation/plans/:id/confirm`

Permission: `FLEET_MANAGE`

Request body:
```json
{ "destination_hub_id": "<uuid>" }
```

Logic:
1. Load plan by `(id, tenant_id)` — 404 if not found.
2. Reject with 409 `PLAN_NOT_DRAFT` if `status != 'draft'`.
3. Derive `transport_mode` from the plan's linked `TruckSpec` row.
4. Call `PgContainerRepository::create(new_uuid, tenant_id, transport_mode, plan.hub_id, destination_hub_id, carrier_ref=None)`.
5. `UPDATE hub_ops.consolidation_plans SET container_id = $1, status = 'confirmed', updated_at = NOW() WHERE id = $2`.
6. Broadcast `plan_confirmed { plan_id, container_id, hub_id }` via `HubBroadcaster`.
7. Return `{ plan_id, container_id, status: "confirmed" }`.

#### `POST /v1/consolidation/plans/:id/scan`

Permission: `SHIPMENT_UPDATE`

Request body:
```json
{ "awb": "<tracking-number>" }
```

Logic:
1. Load plan — 404 if not found.
2. Reject with 409 if `status != 'confirmed'`.
3. Deserialize `plan.placements` as `Vec<Placement>`; check that `awb` appears in the list.  
   If not found → 422 `AWB_NOT_IN_PLAN` with `{ error, message, awb }`.
4. `INSERT INTO hub_ops.consolidation_plan_loadings(plan_id, awb, scanned_by, tenant_id)`.  
   On `UNIQUE` conflict → 409 `ALREADY_SCANNED`.
5. Query `loaded_count = SELECT COUNT(*) FROM consolidation_plan_loadings WHERE plan_id = $1`.  
   `total_count = plan.piece_count`.
6. Broadcast `box_scanned { awb, plan_id, hub_id, loaded_count, total_count }`.
7. If `loaded_count == total_count`:
   - `UPDATE consolidation_plans SET status = 'loaded', updated_at = NOW() WHERE id = $1`.
   - Call `PgContainerRepository::finalise(container_id)` (transitions container to `manifested`).
   - Broadcast `plan_loaded { plan_id, container_id, hub_id }`.
8. Return `{ awb, loaded_count, total_count, plan_status }`.

### Repository additions

`ConsolidationService` gains two new methods:

```rust
pub async fn confirm_plan(
    &self, id: Uuid, tenant_id: Uuid, destination_hub_id: Uuid,
    container_repo: &PgContainerRepository,
) -> anyhow::Result<ConsolidationPlan>

pub async fn scan_piece(
    &self, id: Uuid, tenant_id: Uuid, awb: &str, scanned_by: Option<Uuid>,
    container_repo: &PgContainerRepository,
) -> anyhow::Result<ScanPieceResult>
```

`ConsolidationPlanRepository` gains:

```rust
async fn confirm(&self, id: Uuid, tenant_id: Uuid, container_id: Uuid) -> anyhow::Result<ConsolidationPlan>;
async fn mark_loaded(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<()>;
async fn insert_loading(&self, plan_id: Uuid, tenant_id: Uuid, awb: &str, scanned_by: Option<Uuid>) -> anyhow::Result<bool>; // false = duplicate
async fn loading_count(&self, plan_id: Uuid) -> anyhow::Result<i64>;
```

---

## Frontend — `ConsolidationPageClient.tsx`

### Mode transitions

The component renders one of three mode panels in the left sidebar, keyed on `currentPlan?.status`:

| Mode | Left panel content |
|------|--------------------|
| `draft` (or no plan) | Existing optimizer controls (unchanged) |
| `confirmed` | Scan input + loading progress bar + piece checklist |
| `loaded` | "Container Sealed" success banner with container ID + link |

### Confirm flow

- A "Confirm & Create Container" button appears when `status === 'draft'` and `placements.length > 0`.
- Clicking it reveals an inline `destination_hub_id` dropdown (populated from `hubsApi.list()`).
- On submit: calls `consolidationApi.confirmPlan(planId, { destination_hub_id })`.
- On success: updates `currentPlan` in state with the returned plan (now `status: 'confirmed'`).

### Scan flow (`confirmed` mode)

- Auto-focused AWB text input. Submits on Enter, clears after each scan.
- Successful scan: piece row gets a green checkmark; 3D viewer box turns `#00FF88`.
- `AWB_NOT_IN_PLAN` (422): toast error "AWB not in this load plan — check the barcode."
- `ALREADY_SCANNED` (409): toast warning "Already scanned."
- Progress bar: `loaded_count / total_count` with `#00FF88` fill.
- WS `box_scanned` events update the progress bar and 3D viewer for multi-terminal scenarios (two scanners at the same container).
- WS `plan_loaded` event transitions the UI to `loaded` mode.

### Loaded mode

- Full-width success banner: "Container Sealed — ready for manifest".
- Container ID displayed in `JetBrains Mono` with a copy button.
- "View in Hub Transfer Board →" link to `/hub-transfer?container_id=<id>`.

### API client additions (`consolidation.ts`)

```ts
confirmPlan(planId: string, body: { destination_hub_id: string }): Promise<ConsolidationPlan>
scanPiece(planId: string, awb: string): Promise<ScanPieceResult>
```

```ts
interface ScanPieceResult {
  awb:         string;
  loaded_count: number;
  total_count:  number;
  plan_status:  'confirmed' | 'loaded';
}
```

`ConsolidationPlan` type gains:
```ts
status:       'draft' | 'confirmed' | 'loaded';
loaded_count?: number;  // included in scan response, not stored on plan row
```

---

## WebSocket Events

| Event type | Payload | Triggered by |
|------------|---------|-------------|
| `plan_confirmed` | `{ plan_id, container_id, hub_id }` | `POST .../confirm` |
| `box_scanned` | `{ awb, plan_id, hub_id, loaded_count, total_count }` | `POST .../scan` |
| `plan_loaded` | `{ plan_id, container_id, hub_id }` | `POST .../scan` when all pieces loaded |

All three are broadcast to the hub's WS channel via the existing `HubBroadcaster`.

---

## Error Codes

| HTTP | Code | Condition |
|------|------|-----------|
| 409 | `PLAN_NOT_DRAFT` | Confirm attempted on non-draft plan |
| 409 | `PLAN_NOT_CONFIRMED` | Scan attempted on non-confirmed plan |
| 422 | `AWB_NOT_IN_PLAN` | Scanned AWB not in `plan.placements` |
| 409 | `ALREADY_SCANNED` | AWB already has a loading row for this plan |

---

## Out of Scope

- Re-optimisation triggered by a scan failure (off-plan piece is simply rejected)
- Multi-vehicle splitting (one plan per container)
- Barcode format validation beyond what already exists in `Awb::parse`
- Loading manifest PDF export
