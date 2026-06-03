# Consolidation End-to-End Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the existing 3D bin-packing plan to the container lifecycle so ops can confirm a plan (auto-creating a container) and hub staff can scan each piece to physically verify the load, with off-plan scans hard-blocked.

**Architecture:** Extend `consolidation_plans` with a `status` state machine (`draft → confirmed → loaded`) and a new `consolidation_plan_loadings` audit table. Two new Rust HTTP handlers orchestrate container creation (confirm) and per-scan validation (scan). The admin-portal 3D viewer gains a loading-mode panel and turns loaded boxes green via a new `loadedAwbs` prop on `PackingCanvas`.

**Tech Stack:** Rust/Axum/SQLx (hub-ops service), Next.js 14 + React Three Fiber (admin-portal), PostgreSQL migration.

**Spec:** `docs/superpowers/specs/2026-06-03-consolidation-e2e-design.md`

---

## File Map

| Action | Path | What changes |
|--------|------|-------------|
| Create | `services/hub-ops/migrations/0012_consolidation_status_and_loadings.sql` | New migration |
| Modify | `services/hub-ops/src/domain/entities/consolidation.rs` | Add `status`, `loaded_awbs` fields to `ConsolidationPlan` |
| Modify | `services/hub-ops/src/application/services/consolidation_service.rs` | Add 4 trait methods, 5 delegating service methods, `ScanPieceResult` |
| Modify | `services/hub-ops/src/bootstrap.rs` | Update `PlanRow`/queries, implement 4 new repo methods, 2 new handlers + routes |
| Modify | `apps/admin-portal/src/lib/api/consolidation.ts` | Add `status`/`loaded_awbs` to types, add `confirmPlan`/`scanPiece` methods |
| Modify | `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/PackingCanvas.tsx` | Add `loadedAwbs?: Set<string>` prop, green color for loaded boxes |
| Modify | `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/ConsolidationPageClient.tsx` | Confirm flow UI, scan flow UI, loaded mode banner |

---

## Task 1: Database Migration

**Files:**
- Create: `services/hub-ops/migrations/0012_consolidation_status_and_loadings.sql`

- [ ] **Step 1: Create the migration file**

```sql
-- services/hub-ops/migrations/0012_consolidation_status_and_loadings.sql

ALTER TABLE hub_ops.consolidation_plans
  ADD COLUMN status TEXT NOT NULL DEFAULT 'draft';

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

- [ ] **Step 2: Verify the file is in place**

```powershell
Get-Item services\hub-ops\migrations\0012_consolidation_status_and_loadings.sql
```

Expected: file path printed, no error.

- [ ] **Step 3: Commit**

```powershell
git add services/hub-ops/migrations/0012_consolidation_status_and_loadings.sql
git commit -m "feat(hub-ops): migration 0012 — consolidation status + plan_loadings table"
```

---

## Task 2: Update ConsolidationPlan Entity + All DB Queries

`status` and `loaded_awbs` must be added to the domain entity, the PlanRow struct, and every SELECT/INSERT that touches `consolidation_plans`.

**Files:**
- Modify: `services/hub-ops/src/domain/entities/consolidation.rs`
- Modify: `services/hub-ops/src/application/services/consolidation_service.rs` (compute_plan struct literal)
- Modify: `services/hub-ops/src/bootstrap.rs` (PlanRow, row_to_plan, list/find/upsert queries)

- [ ] **Step 1: Add `status` and `loaded_awbs` to `ConsolidationPlan`**

In `services/hub-ops/src/domain/entities/consolidation.rs`, replace the `ConsolidationPlan` struct:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub hub_id:           Uuid,
    pub truck_spec_id:    Uuid,
    pub container_id:     Option<Uuid>,
    pub items:            serde_json::Value,
    pub placements:       serde_json::Value,
    pub unplaced:         serde_json::Value,
    pub total_weight_kg:  f64,
    pub volume_used_cm3:  i64,
    pub volume_total_cm3: i64,
    pub piece_count:      i32,
    pub status:           String,      // "draft" | "confirmed" | "loaded"
    pub loaded_awbs:      Vec<String>, // populated by find(); list() returns empty vec
    pub computed_at:      DateTime<Utc>,
    pub created_at:       DateTime<Utc>,
    pub updated_at:       DateTime<Utc>,
}
```

- [ ] **Step 2: Add `status` to the new plan created in `compute_plan`**

In `services/hub-ops/src/application/services/consolidation_service.rs`, find the `ConsolidationPlan { ... }` struct literal inside `compute_plan` and add two fields before `computed_at`:

```rust
        let plan = ConsolidationPlan {
            id:               Uuid::new_v4(),
            tenant_id,
            hub_id:           cmd.hub_id,
            truck_spec_id:    cmd.truck_spec_id,
            container_id:     None,
            items:            items_json,
            placements:       placements_json,
            unplaced:         unplaced_json,
            total_weight_kg:  result.total_weight_kg,
            volume_used_cm3:  result.volume_used_cm3 as i64,
            volume_total_cm3: result.volume_total_cm3 as i64,
            piece_count:      result.placements.len() as i32,
            status:           "draft".to_owned(),
            loaded_awbs:      vec![],
            computed_at:      Utc::now(),
            created_at:       Utc::now(),
            updated_at:       Utc::now(),
        };
```

- [ ] **Step 3: Update `PlanRow` in `bootstrap.rs`**

Find the `struct PlanRow { ... }` block and replace it:

```rust
#[derive(sqlx::FromRow)]
struct PlanRow {
    id: Uuid, tenant_id: Uuid, hub_id: Uuid, truck_spec_id: Uuid,
    container_id: Option<Uuid>,
    items: serde_json::Value, placements: serde_json::Value, unplaced: serde_json::Value,
    total_weight_kg: f64,
    volume_used_cm3: i64, volume_total_cm3: i64,
    piece_count: i32,
    status: String,
    loaded_awbs: Vec<String>,
    computed_at: chrono::DateTime<chrono::Utc>,
    created_at:  chrono::DateTime<chrono::Utc>,
    updated_at:  chrono::DateTime<chrono::Utc>,
}
```

- [ ] **Step 4: Update `row_to_plan` in `bootstrap.rs`**

Replace the function body:

```rust
fn row_to_plan(r: PlanRow) -> ConsolidationPlan {
    ConsolidationPlan {
        id: r.id, tenant_id: r.tenant_id, hub_id: r.hub_id,
        truck_spec_id: r.truck_spec_id, container_id: r.container_id,
        items: r.items, placements: r.placements, unplaced: r.unplaced,
        total_weight_kg: r.total_weight_kg,
        volume_used_cm3:  r.volume_used_cm3,
        volume_total_cm3: r.volume_total_cm3,
        piece_count:  r.piece_count,
        status:       r.status,
        loaded_awbs:  r.loaded_awbs,
        computed_at:  r.computed_at,
        created_at:   r.created_at,
        updated_at:   r.updated_at,
    }
}
```

- [ ] **Step 5: Update the `list` query in `PgConsolidationPlanRepository`**

Replace the SQL in the `list` async fn:

```rust
    async fn list(&self, hub_id: Uuid, tenant_id: Uuid) -> anyhow::Result<Vec<ConsolidationPlan>> {
        let rows = sqlx::query_as::<_, PlanRow>(
            r#"SELECT id, tenant_id, hub_id, truck_spec_id, container_id,
                      items, placements, unplaced,
                      total_weight_kg::float8, volume_used_cm3, volume_total_cm3,
                      piece_count, status, '{}'::text[] AS loaded_awbs,
                      computed_at, created_at, updated_at
               FROM hub_ops.consolidation_plans
               WHERE hub_id = $1 AND tenant_id = $2
               ORDER BY created_at DESC
               LIMIT 20"#
        ).bind(hub_id).bind(tenant_id).fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(row_to_plan).collect())
    }
```

- [ ] **Step 6: Update the `find` query in `PgConsolidationPlanRepository`**

Replace the SQL in the `find` async fn:

```rust
    async fn find(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<Option<ConsolidationPlan>> {
        let row = sqlx::query_as::<_, PlanRow>(
            r#"SELECT p.id, p.tenant_id, p.hub_id, p.truck_spec_id, p.container_id,
                      p.items, p.placements, p.unplaced,
                      p.total_weight_kg::float8, p.volume_used_cm3, p.volume_total_cm3,
                      p.piece_count, p.status,
                      COALESCE(
                          ARRAY(SELECT awb FROM hub_ops.consolidation_plan_loadings
                                WHERE plan_id = p.id ORDER BY scanned_at),
                          '{}'::text[]
                      ) AS loaded_awbs,
                      p.computed_at, p.created_at, p.updated_at
               FROM hub_ops.consolidation_plans p
               WHERE p.id = $1 AND p.tenant_id = $2"#
        ).bind(id).bind(tenant_id).fetch_optional(&self.pool).await?;
        Ok(row.map(row_to_plan))
    }
```

- [ ] **Step 7: Update the `upsert` query in `PgConsolidationPlanRepository`**

Replace the entire `upsert` async fn body. Add `status` as `$13`, shift `computed_at`→`$14`, `created_at`→`$15`, `updated_at`→`$16`. Do NOT include `status` in the ON CONFLICT update clause (re-optimising must not reset a confirmed plan to draft):

```rust
    async fn upsert(&self, plan: &ConsolidationPlan) -> anyhow::Result<ConsolidationPlan> {
        sqlx::query(
            r#"INSERT INTO hub_ops.consolidation_plans (
                id, tenant_id, hub_id, truck_spec_id, container_id,
                items, placements, unplaced,
                total_weight_kg, volume_used_cm3, volume_total_cm3,
                piece_count, status, computed_at, created_at, updated_at
               ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
               ON CONFLICT (id) DO UPDATE SET
                placements       = EXCLUDED.placements,
                unplaced         = EXCLUDED.unplaced,
                total_weight_kg  = EXCLUDED.total_weight_kg,
                volume_used_cm3  = EXCLUDED.volume_used_cm3,
                volume_total_cm3 = EXCLUDED.volume_total_cm3,
                piece_count      = EXCLUDED.piece_count,
                computed_at      = EXCLUDED.computed_at,
                updated_at       = EXCLUDED.updated_at"#
        )
        .bind(plan.id).bind(plan.tenant_id).bind(plan.hub_id).bind(plan.truck_spec_id)
        .bind(plan.container_id)
        .bind(&plan.items).bind(&plan.placements).bind(&plan.unplaced)
        .bind(plan.total_weight_kg).bind(plan.volume_used_cm3).bind(plan.volume_total_cm3)
        .bind(plan.piece_count).bind(&plan.status)
        .bind(plan.computed_at).bind(plan.created_at).bind(plan.updated_at)
        .execute(&self.pool).await?;
        Ok(plan.clone())
    }
```

- [ ] **Step 8: Verify compilation**

```powershell
$env:CARGO_INCREMENTAL=0; cargo check -p hub-ops
```

Expected: `Finished` with no errors. If `missing field 'status'` or `missing field 'loaded_awbs'` errors appear, a struct literal somewhere else still needs updating — fix it before proceeding.

- [ ] **Step 9: Commit**

```powershell
git add services/hub-ops/src/domain/entities/consolidation.rs
git add services/hub-ops/src/application/services/consolidation_service.rs
git add services/hub-ops/src/bootstrap.rs
git commit -m "feat(hub-ops): add status + loaded_awbs to ConsolidationPlan; update all queries"
```

---

## Task 3: Repository Trait + Service Delegating Methods

Add 4 new methods to `ConsolidationPlanRepository`, implement them in `PgConsolidationPlanRepository`, and add 5 thin public delegating methods to `ConsolidationService` so handlers can orchestrate without touching private fields.

**Files:**
- Modify: `services/hub-ops/src/application/services/consolidation_service.rs`
- Modify: `services/hub-ops/src/bootstrap.rs`

- [ ] **Step 1: Add new trait methods and `ScanPieceResult` to `consolidation_service.rs`**

At the bottom of the `ConsolidationPlanRepository` trait (after `update_placements`), add:

```rust
    async fn confirm(
        &self, id: Uuid, tenant_id: Uuid, container_id: Uuid,
    ) -> anyhow::Result<ConsolidationPlan>;
    async fn mark_loaded(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<()>;
    /// Returns `true` if the row was inserted, `false` if the AWB was already present.
    async fn insert_loading(
        &self, plan_id: Uuid, tenant_id: Uuid, awb: &str, scanned_by: Option<Uuid>,
    ) -> anyhow::Result<bool>;
    async fn loading_count(&self, plan_id: Uuid) -> anyhow::Result<i64>;
```

After the closing `}` of the trait, add the result struct:

```rust
#[derive(Debug, serde::Serialize)]
pub struct ScanPieceResult {
    pub awb:          String,
    pub loaded_count: i64,
    pub total_count:  i32,
    pub plan_status:  String,
    pub hub_id:       Uuid,
    pub container_id: Option<Uuid>,
}
```

- [ ] **Step 2: Add 5 delegating methods to `ConsolidationService` in `consolidation_service.rs`**

After the closing `}` of the last existing `impl ConsolidationService` method (`update_placements`), add:

```rust
    pub async fn find_spec(
        &self, id: Uuid, tenant_id: Uuid,
    ) -> anyhow::Result<Option<TruckSpec>> {
        self.specs.find(id, tenant_id).await
    }

    pub async fn set_confirmed(
        &self, id: Uuid, tenant_id: Uuid, container_id: Uuid,
    ) -> anyhow::Result<ConsolidationPlan> {
        self.plans.confirm(id, tenant_id, container_id).await
    }

    pub async fn insert_loading(
        &self, plan_id: Uuid, tenant_id: Uuid, awb: &str, scanned_by: Option<Uuid>,
    ) -> anyhow::Result<bool> {
        self.plans.insert_loading(plan_id, tenant_id, awb, scanned_by).await
    }

    pub async fn loading_count(&self, plan_id: Uuid) -> anyhow::Result<i64> {
        self.plans.loading_count(plan_id).await
    }

    pub async fn mark_loaded(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<()> {
        self.plans.mark_loaded(id, tenant_id).await
    }
```

- [ ] **Step 3: Implement the 4 new trait methods in `PgConsolidationPlanRepository` in `bootstrap.rs`**

Add these four `async fn` blocks inside the `#[async_trait::async_trait] impl ConsolidationPlanRepository for PgConsolidationPlanRepository` block, after `update_placements`:

```rust
    async fn confirm(
        &self, id: Uuid, tenant_id: Uuid, container_id: Uuid,
    ) -> anyhow::Result<ConsolidationPlan> {
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE hub_ops.consolidation_plans
                SET status = 'confirmed', container_id = $1, updated_at = $2
              WHERE id = $3 AND tenant_id = $4"
        )
        .bind(container_id).bind(now).bind(id).bind(tenant_id)
        .execute(&self.pool).await?;
        self.find(id, tenant_id).await?
            .ok_or_else(|| anyhow::anyhow!("plan not found after confirm"))
    }

    async fn mark_loaded(&self, id: Uuid, tenant_id: Uuid) -> anyhow::Result<()> {
        let now = chrono::Utc::now();
        sqlx::query(
            "UPDATE hub_ops.consolidation_plans
                SET status = 'loaded', updated_at = $1
              WHERE id = $2 AND tenant_id = $3"
        )
        .bind(now).bind(id).bind(tenant_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    async fn insert_loading(
        &self, plan_id: Uuid, tenant_id: Uuid, awb: &str, scanned_by: Option<Uuid>,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"INSERT INTO hub_ops.consolidation_plan_loadings
               (plan_id, tenant_id, awb, scanned_by)
               VALUES ($1, $2, $3, $4)
               ON CONFLICT (plan_id, awb) DO NOTHING"#
        )
        .bind(plan_id).bind(tenant_id).bind(awb).bind(scanned_by)
        .execute(&self.pool).await?;
        Ok(result.rows_affected() > 0)
    }

    async fn loading_count(&self, plan_id: Uuid) -> anyhow::Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM hub_ops.consolidation_plan_loadings WHERE plan_id = $1"
        )
        .bind(plan_id)
        .fetch_one(&self.pool).await?;
        Ok(row.0)
    }
```

- [ ] **Step 4: Verify compilation**

```powershell
$env:CARGO_INCREMENTAL=0; cargo check -p hub-ops
```

Expected: `Finished` with no errors. Fix any "method not provided" or "missing in implementation" errors before continuing.

- [ ] **Step 5: Commit**

```powershell
git add services/hub-ops/src/application/services/consolidation_service.rs
git add services/hub-ops/src/bootstrap.rs
git commit -m "feat(hub-ops): consolidation repo trait extensions + service delegating methods"
```

---

## Task 4: Confirm HTTP Handler + Route

**Files:**
- Modify: `services/hub-ops/src/bootstrap.rs`

- [ ] **Step 1: Add the `ConfirmPlanBody` struct and handler**

Add this block in `bootstrap.rs` just before the `// ---------------------------------------------------------------------------` comment that precedes the WebSocket handler section:

```rust
// ---------------------------------------------------------------------------
// Consolidation confirm handler
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ConfirmPlanBody {
    destination_hub_id: Uuid,
    /// "road" | "sea" | "air" — derived from TruckSpec on the frontend.
    transport_mode: String,
}

/// `POST /v1/consolidation/plans/:id/confirm`
///
/// Transitions a draft plan to confirmed: creates a container and links it.
/// Returns 409 if the plan is not in draft status.
async fn confirm_plan_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<ConfirmPlanBody>,
) -> impl IntoResponse {
    claims.require_permission(permissions::FLEET_MANAGE)?;

    if !["road", "sea", "air"].contains(&body.transport_mode.as_str()) {
        return Err(AppError::Validation(
            "transport_mode must be 'road', 'sea', or 'air'".into(),
        ));
    }

    let plan = s.consolidation_svc
        .get_plan(id, claims.tenant_id).await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "consolidation_plan", id: id.to_string() })?;

    if plan.status != "draft" {
        return Ok::<_, AppError>((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error":   "PLAN_NOT_DRAFT",
                "message": "Plan must be in draft status to confirm.",
                "status":  plan.status,
            })),
        ));
    }

    let container_id = Uuid::new_v4();
    s.containers.create(
        container_id,
        claims.tenant_id,
        &body.transport_mode,
        plan.hub_id,
        body.destination_hub_id,
        None,
    ).await.map_err(AppError::internal)?;

    let confirmed = s.consolidation_svc
        .set_confirmed(id, claims.tenant_id, container_id).await
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;

    let event = serde_json::json!({
        "type":         "plan_confirmed",
        "plan_id":      id,
        "container_id": container_id,
        "hub_id":       plan.hub_id,
    });
    s.hub_broadcaster.broadcast(plan.hub_id, event.to_string()).await;

    tracing::info!(
        plan_id      = %id,
        container_id = %container_id,
        tenant_id    = %claims.tenant_id,
        "Consolidation plan confirmed"
    );

    Ok((StatusCode::OK, Json(confirmed)))
}
```

- [ ] **Step 2: Register the route**

In the `let protected = Router::new()` block in `bootstrap.rs`, find the consolidation plan routes section and add the confirm route:

```rust
        // Consolidation — plans
        .route("/v1/consolidation/plans",                       post(compute_plan))
        .route("/v1/consolidation/plans/:id",                   get(get_plan))
        .route("/v1/consolidation/plans/:id/confirm",           post(confirm_plan_handler))
        .route("/v1/consolidation/plans/:id/placements",        axum::routing::put(update_placements_handler))
        .route("/v1/hubs/:hub_id/consolidation/plans",          get(list_hub_plans))
```

- [ ] **Step 3: Verify compilation**

```powershell
$env:CARGO_INCREMENTAL=0; cargo check -p hub-ops
```

Expected: `Finished` with no errors.

- [ ] **Step 4: Commit**

```powershell
git add services/hub-ops/src/bootstrap.rs
git commit -m "feat(hub-ops): POST /v1/consolidation/plans/:id/confirm handler"
```

---

## Task 5: Scan Piece HTTP Handler + Route

**Files:**
- Modify: `services/hub-ops/src/bootstrap.rs`

- [ ] **Step 1: Add the `ScanPieceBody` struct and handler**

Add this block immediately after the `confirm_plan_handler` closing brace:

```rust
// ---------------------------------------------------------------------------
// Consolidation scan handler
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ScanPieceBody {
    awb: String,
}

/// `POST /v1/consolidation/plans/:id/scan`
///
/// Validates that `awb` is in the plan's placements list, inserts a loading
/// record, and transitions the plan to `loaded` (auto-finalising the container)
/// once every placed AWB has been scanned.
///
/// Returns:
///   409 PLAN_NOT_CONFIRMED  — plan is not in confirmed status
///   422 AWB_NOT_IN_PLAN     — awb not found in plan.placements
///   409 ALREADY_SCANNED     — this awb already has a loading record
async fn scan_piece_handler(
    State(s): State<AppState>,
    claims: AuthClaims,
    Path(id): Path<Uuid>,
    Json(body): Json<ScanPieceBody>,
) -> impl IntoResponse {
    use crate::domain::algorithms::bin_pack::Placement;
    claims.require_permission(permissions::SHIPMENT_UPDATE)?;

    let plan = s.consolidation_svc
        .get_plan(id, claims.tenant_id).await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound { resource: "consolidation_plan", id: id.to_string() })?;

    if plan.status != "confirmed" {
        return Ok::<_, AppError>((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error":   "PLAN_NOT_CONFIRMED",
                "message": "Scanning requires the plan to be in confirmed status.",
                "status":  plan.status,
            })),
        ));
    }

    // Validate AWB is in the plan's placements.
    let placements: Vec<Placement> = serde_json::from_value(plan.placements.clone())
        .map_err(|e| AppError::internal(anyhow::anyhow!(e)))?;

    if !placements.iter().any(|p| p.awb == body.awb) {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({
                "error":   "AWB_NOT_IN_PLAN",
                "message": "The scanned AWB is not in this load plan.",
                "awb":     body.awb,
            })),
        ));
    }

    // Insert the loading record. `false` means UNIQUE conflict → already scanned.
    let inserted = s.consolidation_svc
        .insert_loading(id, claims.tenant_id, &body.awb, Some(claims.user_id)).await
        .map_err(AppError::internal)?;

    if !inserted {
        return Ok((
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error":   "ALREADY_SCANNED",
                "message": "This AWB has already been loaded against this plan.",
                "awb":     body.awb,
            })),
        ));
    }

    let loaded_count = s.consolidation_svc
        .loading_count(id).await
        .map_err(AppError::internal)?;
    let total_count = plan.piece_count;

    // Broadcast box_scanned for live 3D viewer update.
    let box_event = serde_json::json!({
        "type":         "box_scanned",
        "awb":          body.awb,
        "plan_id":      id,
        "hub_id":       plan.hub_id,
        "loaded_count": loaded_count,
        "total_count":  total_count,
    });
    s.hub_broadcaster.broadcast(plan.hub_id, box_event.to_string()).await;

    let plan_status = if loaded_count >= total_count as i64 {
        s.consolidation_svc
            .mark_loaded(id, claims.tenant_id).await
            .map_err(AppError::internal)?;

        if let Some(container_id) = plan.container_id {
            s.containers.finalise(container_id).await
                .map_err(|e| AppError::BusinessRule(e.to_string()))?;

            let loaded_event = serde_json::json!({
                "type":         "plan_loaded",
                "plan_id":      id,
                "container_id": container_id,
                "hub_id":       plan.hub_id,
            });
            s.hub_broadcaster.broadcast(plan.hub_id, loaded_event.to_string()).await;

            tracing::info!(
                plan_id      = %id,
                container_id = %container_id,
                tenant_id    = %claims.tenant_id,
                "Consolidation plan fully loaded — container finalised"
            );
        }
        "loaded"
    } else {
        "confirmed"
    };

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "awb":          body.awb,
            "loaded_count": loaded_count,
            "total_count":  total_count,
            "plan_status":  plan_status,
        })),
    ))
}
```

- [ ] **Step 2: Register the route**

In the protected router, add the scan route alongside confirm:

```rust
        .route("/v1/consolidation/plans/:id/confirm",           post(confirm_plan_handler))
        .route("/v1/consolidation/plans/:id/scan",              post(scan_piece_handler))
```

- [ ] **Step 3: Verify compilation**

```powershell
$env:CARGO_INCREMENTAL=0; cargo check -p hub-ops
```

Expected: `Finished` with no errors. Common issue: `use crate::domain::algorithms::bin_pack::Placement;` — if this path is wrong, check `services/hub-ops/src/domain/algorithms/bin_pack.rs` for the correct module path.

- [ ] **Step 4: Commit**

```powershell
git add services/hub-ops/src/bootstrap.rs
git commit -m "feat(hub-ops): POST /v1/consolidation/plans/:id/scan handler + PLAN_NOT_CONFIRMED/AWB_NOT_IN_PLAN/ALREADY_SCANNED guards"
```

---

## Task 6: Frontend API Client Updates

**Files:**
- Modify: `apps/admin-portal/src/lib/api/consolidation.ts`

- [ ] **Step 1: Add `status` and `loaded_awbs` to `ConsolidationPlan` type**

Find the `interface ConsolidationPlan` block and add two fields:

```ts
export interface ConsolidationPlan {
  id:               string;
  tenant_id:        string;
  hub_id:           string;
  truck_spec_id:    string;
  container_id:     string | null;
  items:            BoxItem[];
  placements:       Placement[];
  unplaced:         UnplacedItem[];
  total_weight_kg:  number;
  volume_used_cm3:  number;
  volume_total_cm3: number;
  piece_count:      number;
  status:           'draft' | 'confirmed' | 'loaded';
  loaded_awbs:      string[];
  computed_at:      string;
  created_at:       string;
  updated_at:       string;
}
```

- [ ] **Step 2: Add `ConfirmPlanBody` and `ScanPieceResult` types**

After the `UpdatePlacementsBody` interface (or after the last existing export interface), add:

```ts
export interface ConfirmPlanBody {
  destination_hub_id: string;
  transport_mode:     'road' | 'sea' | 'air';
}

export interface ScanPieceResult {
  awb:          string;
  loaded_count: number;
  total_count:  number;
  plan_status:  'confirmed' | 'loaded';
}
```

- [ ] **Step 3: Add `confirmPlan` and `scanPiece` to the API factory**

Inside `createConsolidationApi`, after the `updatePlacements` method, add:

```ts
    /** Confirm a draft plan — creates a container and links it. */
    async confirmPlan(planId: string, body: ConfirmPlanBody): Promise<ConsolidationPlan> {
      const res = await client.post<ConsolidationPlan>(
        `/v1/consolidation/plans/${planId}/confirm`,
        body,
      );
      return res.data;
    },

    /** Scan a piece AWB against a confirmed plan. */
    async scanPiece(planId: string, awb: string): Promise<ScanPieceResult> {
      const res = await client.post<ScanPieceResult>(
        `/v1/consolidation/plans/${planId}/scan`,
        { awb },
      );
      return res.data;
    },
```

- [ ] **Step 4: Verify TypeScript**

```powershell
cd apps/admin-portal; npx tsc --noEmit
```

Expected: no errors. Fix any type mismatches before continuing.

- [ ] **Step 5: Commit**

```powershell
git add apps/admin-portal/src/lib/api/consolidation.ts
git commit -m "feat(admin-portal): consolidation API client — confirmPlan + scanPiece + status types"
```

---

## Task 7: PackingCanvas — `loadedAwbs` Prop

Add a `loadedAwbs` prop so scanned boxes turn signal green in the 3D viewer.

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/PackingCanvas.tsx`

- [ ] **Step 1: Add `isLoaded` to `PackedBoxProps`**

Find `interface PackedBoxProps` and add the new field:

```tsx
interface PackedBoxProps {
  placement:  Placement;
  isSelected: boolean;
  isLoaded:   boolean;
  onClick:    () => void;
  index:      number;
}
```

- [ ] **Step 2: Update `PackedBox` to accept and use `isLoaded`**

Find the `function PackedBox({ placement: p, isSelected, onClick, index }: PackedBoxProps)` signature and update it:

```tsx
function PackedBox({ placement: p, isSelected, isLoaded, onClick, index }: PackedBoxProps) {
```

Find the color derivation lines:

```tsx
  const baseColor = p.estimated
    ? AMBER
    : BOX_COLORS[index % BOX_COLORS.length];

  const color = isSelected ? '#FFFFFF' : baseColor;
```

Replace them:

```tsx
  const baseColor = p.estimated
    ? AMBER
    : BOX_COLORS[index % BOX_COLORS.length];

  const color = isSelected ? '#FFFFFF' : (isLoaded ? GREEN : baseColor);
```

- [ ] **Step 3: Add `loadedAwbs` to `PackingCanvasProps`**

Find `export interface PackingCanvasProps` and add the optional field:

```tsx
export interface PackingCanvasProps {
  spec:        TruckSpec;
  placements:  Placement[];
  selectedAwb: string | null;
  onSelect:    (awb: string | null) => void;
  onNudge:     (awb: string, axis: NudgeAxis, delta: number) => void;
  loadedAwbs?: Set<string>;
}
```

- [ ] **Step 4: Destructure and pass `loadedAwbs` down to `PackedBox`**

Update the function signature:

```tsx
export default function PackingCanvas({
  spec,
  placements,
  selectedAwb,
  onSelect,
  onNudge,
  loadedAwbs,
}: PackingCanvasProps) {
```

Find the `placements.map` call inside the Canvas and add `isLoaded`:

```tsx
      {placements.map((p, i) => (
        <PackedBox
          key={p.awb}
          placement={p}
          index={i}
          isSelected={selectedAwb === p.awb}
          isLoaded={loadedAwbs?.has(p.awb) ?? false}
          onClick={() => onSelect(p.awb)}
        />
      ))}
```

- [ ] **Step 5: Verify TypeScript**

```powershell
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 6: Commit**

```powershell
git add apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/PackingCanvas.tsx
git commit -m "feat(admin-portal): PackingCanvas — loadedAwbs prop turns scanned boxes green"
```

---

## Task 8: ConsolidationPageClient — Confirm Flow

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/ConsolidationPageClient.tsx`

- [ ] **Step 1: Add new imports**

At the top of the file, add `Hub` and `hubIdOf` to the hubs import and add `CheckCircle2` to lucide:

```tsx
import {
  Boxes, Truck, Weight, Package, AlertTriangle,
  Wifi, WifiOff, RefreshCw, ChevronRight, Settings2,
  Download, ArrowRight, ArrowLeft, ArrowUp, ArrowDown,
  Info, CheckCircle2, MapPin,
} from 'lucide-react';
import { createHubsApi } from '@/lib/api/hubs';
import { hubIdOf, type Hub } from '@/lib/api/hubs';
```

(The `createHubsApi` import already exists — only add the named type imports if not present.)

- [ ] **Step 2: Add state variables**

Inside `ConsolidationPageClient`, after the existing state declarations, add:

```tsx
  const [hubs,            setHubs]            = useState<Hub[]>([]);
  const [destHubId,       setDestHubId]       = useState('');
  const [showConfirmForm, setShowConfirmForm]  = useState(false);
  const [confirming,      setConfirming]       = useState(false);
  const [loadedAwbs,      setLoadedAwbs]       = useState<Set<string>>(new Set());
```

- [ ] **Step 3: Load the hub list and restore `loadedAwbs` on mount**

Inside the existing `useEffect` init function, after the `if (plans.length > 0) { setCurrentPlan(...) }` block, add:

```tsx
      // Seed loadedAwbs from the plan if it was previously confirmed.
      if (plans.length > 0 && plans[0].loaded_awbs.length > 0) {
        setLoadedAwbs(new Set(plans[0].loaded_awbs));
      }
```

After the `init()` call's `setLoading(false)`, also load the hub list:

```tsx
    async function loadHubs() {
      try {
        const list = await hubsApi.list();
        setHubs(list);
      } catch {
        // non-fatal — confirm form will just show an empty dropdown
      }
    }
    loadHubs();
```

- [ ] **Step 4: Add `handleConfirm` function**

After `handleOptimize`, add:

```tsx
  async function handleConfirm() {
    if (!currentPlan || !selectedSpec || !destHubId) return;
    setConfirming(true);
    try {
      const confirmed = await consolidationApi.confirmPlan(currentPlan.id, {
        destination_hub_id: destHubId,
        transport_mode: selectedSpec.transport_mode as 'road' | 'sea' | 'air',
      });
      setCurrentPlan(confirmed);
      setShowConfirmForm(false);
      toast.success('Plan confirmed — container created. Ready for scanning.');
    } catch (e) {
      toast.error((e as { message?: string })?.message ?? 'Failed to confirm plan.');
    } finally {
      setConfirming(false);
    }
  }
```

- [ ] **Step 5: Handle `plan_confirmed` WS event**

Inside the `onEvent` callback in `useHubEvents`, add a case for `plan_confirmed`:

```tsx
      if (event.type === 'plan_confirmed' && event.plan_id) {
        try {
          const plan = await consolidationApi.getPlan(event.plan_id);
          setCurrentPlan(plan);
          toast.success('Plan confirmed by another terminal.');
        } catch {
          toast.error('Failed to fetch confirmed plan.');
        }
      }
```

- [ ] **Step 6: Add the Confirm button + inline destination-hub form to the left panel**

In the `{/* Actions */}` div (which contains the existing Re-optimise and Load from Manifest buttons), add the confirm button block after those buttons. The confirm button should only appear when `currentPlan?.status === 'draft'` and `placements.length > 0`:

```tsx
          {currentPlan?.status === 'draft' && placements.length > 0 && (
            <div className="flex flex-col gap-2">
              {!showConfirmForm ? (
                <button
                  onClick={() => setShowConfirmForm(true)}
                  className={cn(
                    'flex items-center justify-center gap-2 rounded-xl py-3 text-sm font-semibold transition-all',
                    'border border-green-500/50 bg-green-500/10 text-green-300',
                    'hover:bg-green-500/20 hover:border-green-400',
                  )}
                >
                  <CheckCircle2 size={14} />
                  Confirm &amp; Create Container
                </button>
              ) : (
                <div className="flex flex-col gap-2 rounded-xl border border-green-500/20 bg-green-500/5 p-3">
                  <p className="text-[11px] text-green-300/70 uppercase tracking-wider">Destination Hub</p>
                  <select
                    value={destHubId}
                    onChange={e => setDestHubId(e.target.value)}
                    className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm text-white focus:border-green-400 focus:outline-none"
                  >
                    <option value="">Select destination…</option>
                    {hubs.map(h => {
                      const hid = hubIdOf(h);
                      return hid !== hubId ? (
                        <option key={hid} value={hid} className="bg-gray-900">{h.name}</option>
                      ) : null;
                    })}
                  </select>
                  <div className="flex gap-2">
                    <button
                      onClick={handleConfirm}
                      disabled={!destHubId || confirming}
                      className="flex-1 rounded-lg bg-green-500/20 py-2 text-sm font-semibold text-green-300 hover:bg-green-500/30 disabled:opacity-40 transition-colors"
                    >
                      {confirming ? 'Confirming…' : 'Confirm'}
                    </button>
                    <button
                      onClick={() => setShowConfirmForm(false)}
                      className="rounded-lg bg-white/5 px-3 py-2 text-sm text-white/50 hover:bg-white/10 transition-colors"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
```

- [ ] **Step 7: Pass `loadedAwbs` to `PackingCanvas`**

Find the `<PackingCanvas` JSX and add the prop:

```tsx
            <PackingCanvas
              spec={selectedSpec}
              placements={placements}
              selectedAwb={selectedAwb}
              onSelect={setSelectedAwb}
              onNudge={handleNudge}
              loadedAwbs={loadedAwbs}
            />
```

- [ ] **Step 8: Verify TypeScript**

```powershell
npx tsc --noEmit
```

Expected: no errors.

- [ ] **Step 9: Commit**

```powershell
git add apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/ConsolidationPageClient.tsx
git commit -m "feat(admin-portal): consolidation confirm flow — hub picker, confirm button, plan_confirmed WS"
```

---

## Task 9: ConsolidationPageClient — Scan + Loaded Mode UI

**Files:**
- Modify: `apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/ConsolidationPageClient.tsx`

- [ ] **Step 1: Add scan state variables**

After the state added in Task 8, add:

```tsx
  const [scanInput,  setScanInput]  = useState('');
  const [scanning,   setScanning]   = useState(false);
  const scanInputRef = useRef<HTMLInputElement>(null);
```

Add `useRef` to the React import if not already present:
```tsx
import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
```

- [ ] **Step 2: Add `handleScan` function**

After `handleConfirm`, add:

```tsx
  async function handleScan(awb: string) {
    if (!currentPlan || !awb.trim()) return;
    setScanning(true);
    try {
      const result = await consolidationApi.scanPiece(currentPlan.id, awb.trim());
      setLoadedAwbs(prev => new Set([...prev, result.awb]));
      setScanInput('');
      if (result.plan_status === 'loaded') {
        setCurrentPlan(prev => prev ? { ...prev, status: 'loaded' } : prev);
        toast.success('All pieces loaded — container sealed!');
      } else {
        toast.success(`Loaded ${result.loaded_count} / ${result.total_count}`);
      }
    } catch (e: unknown) {
      const err = e as { response?: { data?: { error?: string; message?: string } }; message?: string };
      const code    = err.response?.data?.error;
      const message = err.response?.data?.message ?? err.message ?? 'Scan failed.';
      if (code === 'AWB_NOT_IN_PLAN') {
        toast.error(`Not in plan: ${awb}`);
      } else if (code === 'ALREADY_SCANNED') {
        toast.warning(`Already scanned: ${awb}`);
      } else {
        toast.error(message);
      }
      setScanInput('');
    } finally {
      setScanning(false);
      scanInputRef.current?.focus();
    }
  }
```

- [ ] **Step 3: Handle `box_scanned` and `plan_loaded` WS events**

In the `onEvent` callback, replace the existing `box_scanned` case and add `plan_loaded`:

```tsx
      if (event.type === 'box_scanned' && event.awb) {
        setLoadedAwbs(prev => new Set([...prev, event.awb as string]));
        setScanFeed(prev => [event.awb as string, ...prev].slice(0, 20));
      }
      if (event.type === 'plan_loaded' && event.plan_id) {
        setCurrentPlan(prev => prev ? { ...prev, status: 'loaded' } : prev);
        toast.success('All pieces loaded — container sealed!', { duration: 6000 });
      }
```

- [ ] **Step 4: Add the scan panel (confirmed mode) and loaded banner (loaded mode) to the left panel**

In the left panel `<div className="flex w-64 shrink-0 flex-col gap-3 overflow-y-auto">`, add these sections **below** the existing `{/* Actions */}` div:

```tsx
          {/* ── Confirmed mode: scan input ────────────────────────── */}
          {currentPlan?.status === 'confirmed' && (
            <div className="flex flex-col gap-3">
              {/* Progress bar */}
              <div>
                <div className="mb-1 flex justify-between text-[11px] text-white/40">
                  <span className="uppercase tracking-wider">Loading progress</span>
                  <span className="font-mono text-green-400">
                    {loadedAwbs.size} / {currentPlan.piece_count}
                  </span>
                </div>
                <div className="h-2 w-full overflow-hidden rounded-full bg-white/10">
                  <div
                    className="h-full rounded-full transition-all duration-300"
                    style={{
                      width:      `${currentPlan.piece_count > 0 ? (loadedAwbs.size / currentPlan.piece_count) * 100 : 0}%`,
                      background: '#00FF88',
                      boxShadow:  '0 0 8px #00FF8880',
                    }}
                  />
                </div>
              </div>

              {/* AWB scan input */}
              <div className="flex flex-col gap-1">
                <label className="text-[11px] text-white/40 uppercase tracking-wider">Scan AWB</label>
                <input
                  ref={scanInputRef}
                  type="text"
                  value={scanInput}
                  onChange={e => setScanInput(e.target.value)}
                  onKeyDown={e => { if (e.key === 'Enter') handleScan(scanInput); }}
                  placeholder="Scan or type AWB…"
                  disabled={scanning}
                  autoFocus
                  className="rounded-lg border border-white/10 bg-white/5 px-3 py-2 text-sm font-mono text-white placeholder-white/20 focus:border-green-400 focus:outline-none disabled:opacity-40"
                />
              </div>

              {/* Piece checklist */}
              <div>
                <div className="mb-1 text-[11px] uppercase tracking-wider text-white/40">
                  Pieces ({placements.length})
                </div>
                <div className="flex max-h-48 flex-col gap-0.5 overflow-y-auto">
                  {placements.map(p => (
                    <div
                      key={p.awb}
                      className={cn(
                        'flex items-center gap-2 rounded-lg px-2 py-1 text-xs transition-colors',
                        loadedAwbs.has(p.awb)
                          ? 'bg-green-500/10 text-green-400'
                          : 'bg-white/5 text-white/40',
                      )}
                    >
                      <CheckCircle2
                        size={11}
                        className={loadedAwbs.has(p.awb) ? 'text-green-400' : 'text-white/20'}
                      />
                      <span className="truncate font-mono">{p.awb}</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}

          {/* ── Loaded mode: success banner ───────────────────────── */}
          {currentPlan?.status === 'loaded' && (
            <div className="flex flex-col gap-3 rounded-xl border border-green-500/30 bg-green-500/10 p-4">
              <div className="flex items-center gap-2 text-green-400">
                <CheckCircle2 size={16} />
                <span className="text-sm font-semibold">Container Sealed</span>
              </div>
              {currentPlan.container_id && (
                <>
                  <div className="flex flex-col gap-1">
                    <span className="text-[10px] text-white/40 uppercase tracking-wider">Container ID</span>
                    <div className="flex items-center gap-2">
                      <span className="flex-1 truncate rounded bg-white/5 px-2 py-1 font-mono text-[11px] text-green-300">
                        {currentPlan.container_id}
                      </span>
                      <button
                        onClick={() => {
                          navigator.clipboard.writeText(currentPlan.container_id!);
                          toast.success('Copied');
                        }}
                        className="shrink-0 text-[10px] text-white/40 hover:text-white transition-colors"
                      >
                        Copy
                      </button>
                    </div>
                  </div>
                  <a
                    href={`/hub-transfer?container_id=${currentPlan.container_id}`}
                    className="flex items-center justify-center gap-1.5 rounded-lg bg-white/5 py-2 text-xs text-cyan-400 hover:bg-white/10 transition-colors"
                  >
                    <MapPin size={11} />
                    View in Hub Transfer Board
                  </a>
                </>
              )}
            </div>
          )}
```

- [ ] **Step 5: Auto-focus scan input when plan transitions to `confirmed`**

Add an effect after the init `useEffect`:

```tsx
  useEffect(() => {
    if (currentPlan?.status === 'confirmed') {
      setTimeout(() => scanInputRef.current?.focus(), 100);
    }
  }, [currentPlan?.status]);
```

- [ ] **Step 6: Verify TypeScript**

```powershell
npx tsc --noEmit
```

Expected: no errors. Common issue: `currentPlan.container_id` used with `!` non-null assertion — TypeScript should accept it inside the `currentPlan.container_id &&` guard block.

- [ ] **Step 7: Commit**

```powershell
git add apps/admin-portal/src/app/(dashboard)/hubs/[hubId]/consolidation/ConsolidationPageClient.tsx
git commit -m "feat(admin-portal): consolidation scan + loaded mode UI — progress bar, piece checklist, green 3D boxes, container banner"
```

---

## Self-Review Checklist

Spec section → task coverage:

| Spec requirement | Covered by |
|-----------------|-----------|
| `draft → confirmed → loaded` state machine | Task 2 (migration), Task 3 (trait), Task 4 (handler) |
| Migration `0012` | Task 1 |
| `status` column on `consolidation_plans` | Tasks 1, 2 |
| `consolidation_plan_loadings` table | Task 1 |
| `POST /confirm` — creates container, links plan | Task 4 |
| `POST /confirm` — 409 if not draft | Task 4 |
| `POST /scan` — 422 AWB_NOT_IN_PLAN | Task 5 |
| `POST /scan` — 409 ALREADY_SCANNED | Task 5 |
| `POST /scan` — auto-finalise container + `plan_loaded` broadcast | Task 5 |
| `plan_confirmed` WS event | Tasks 4, 8 |
| `box_scanned` WS event | Tasks 5, 9 |
| `plan_loaded` WS event | Tasks 5, 9 |
| Frontend confirm button + destination hub picker | Task 8 |
| Frontend scan input + piece checklist | Task 9 |
| Loaded state banner with container ID | Task 9 |
| 3D viewer turns loaded boxes green | Tasks 7, 8 |
| `loaded_awbs` restored on page load | Task 8 |

No gaps found.
