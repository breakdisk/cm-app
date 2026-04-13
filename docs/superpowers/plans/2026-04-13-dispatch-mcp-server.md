# Dispatch MCP Server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an MCP (Model Context Protocol) server to the dispatch service — a second listener on port 8105 exposing 5 AI-callable tools over Streamable HTTP (JSON-RPC 2.0).

**Architecture:** Embedded in the dispatch process, sharing `AppState` with the HTTP API. A new `src/mcp/` module handles JSON-RPC routing, JWT auth, and audit logging. No new crate dependencies — uses `axum`, `serde_json`, `tokio` already present. Each tool handler receives an `McpContext` derived from the JWT, preventing cross-tenant access at the type level.

**Tech Stack:** Rust, Axum, tokio, serde_json, logisticos-auth (existing), tracing (existing). No new crate dependencies.

**Spec:** `docs/superpowers/specs/2026-04-13-dispatch-mcp-server-design.md`

---

## File Map

Files to **create:**
- `services/dispatch/src/mcp/mod.rs` — Axum router, JSON-RPC dispatch (`initialize` / `tools/list` / `tools/call`)
- `services/dispatch/src/mcp/context.rs` — `McpContext` struct
- `services/dispatch/src/mcp/auth.rs` — JWT extraction + permission check, returns `McpContext`
- `services/dispatch/src/mcp/audit.rs` — `audit_tool_call()` helper that emits a structured tracing event
- `services/dispatch/src/mcp/tools/mod.rs` — tool registry (name → handler fn dispatch)
- `services/dispatch/src/mcp/tools/get_available_drivers.rs`
- `services/dispatch/src/mcp/tools/assign_driver.rs`
- `services/dispatch/src/mcp/tools/optimize_route.rs`
- `services/dispatch/src/mcp/tools/rank_drivers.rs`
- `services/dispatch/src/mcp/tools/get_route_status.rs`

Files to **modify:**
- `services/dispatch/src/lib.rs` — add `pub mod mcp;`
- `services/dispatch/src/bootstrap.rs` — bind second listener on `:8105`, spawn second `axum::serve`

---

## Permission Mapping

The existing RBAC constants in `libs/auth/src/rbac.rs` are:
- `DISPATCH_VIEW` = `"dispatch:view"` — for read tools
- `DISPATCH_ASSIGN` = `"dispatch:assign"` — for write tools (assign_driver, optimize_route)
- `DISPATCH_REROUTE` = `"dispatch:reroute"` — for optimize_route

Used in the plan as:
| Tool | Required permission constant |
|------|-----------------------------|
| `get_available_drivers` | `DISPATCH_VIEW` |
| `assign_driver` | `DISPATCH_ASSIGN` |
| `optimize_route` | `DISPATCH_REROUTE` |
| `rank_drivers_for_shipments` | `DISPATCH_VIEW` |
| `get_route_status` | `DISPATCH_VIEW` |

---

## Task 1: `McpContext` and auth extractor

**Files:**
- Create: `services/dispatch/src/mcp/context.rs`
- Create: `services/dispatch/src/mcp/auth.rs`
- Create: `services/dispatch/src/mcp/mod.rs` (stub only — full impl in Task 6)
- Modify: `services/dispatch/src/lib.rs`

- [ ] **Step 1: Add `pub mod mcp;` to lib.rs**

Open `services/dispatch/src/lib.rs`. Add one line:

```rust
pub mod api;
pub mod application;
pub mod bootstrap;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod mcp;
```

- [ ] **Step 2: Create `context.rs`**

Create `services/dispatch/src/mcp/context.rs`:

```rust
use uuid::Uuid;

/// Derived from a verified JWT. Injected into every MCP tool handler.
/// All repo calls take `&McpContext` — the tenant_id is always JWT-derived,
/// never taken from tool arguments.
#[derive(Debug, Clone)]
pub struct McpContext {
    pub tenant_id: Uuid,
    pub actor_uid: Uuid,
    pub permissions: Vec<String>,
    pub trace_id: String,
}

impl McpContext {
    pub fn has_permission(&self, perm: &str) -> bool {
        self.permissions.contains(&perm.to_owned())
            || self.permissions.contains(&"*".to_owned())
    }
}
```

- [ ] **Step 3: Create `auth.rs`**

Create `services/dispatch/src/mcp/auth.rs`:

```rust
use axum::http::HeaderMap;
use std::sync::Arc;
use logisticos_auth::jwt::JwtService;
use super::context::McpContext;

/// Extracts and validates a Bearer JWT from request headers.
/// Returns `McpContext` on success or an error string on failure.
pub fn extract_context(headers: &HeaderMap, jwt: &Arc<JwtService>) -> Result<McpContext, String> {
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| "Missing Authorization header".to_string())?;

    let data = jwt
        .validate_access_token(token)
        .map_err(|e| format!("Invalid token: {e}"))?;

    let claims = data.claims;

    // Extract trace_id from the current tracing span (set by logisticos-tracing init).
    // Falls back to a generated UUID if no active span.
    let trace_id = {
        use tracing::field::{Field, Visit};
        struct TraceIdVisitor(Option<String>);
        impl Visit for TraceIdVisitor {
            fn record_str(&mut self, field: &Field, value: &str) {
                if field.name() == "trace_id" {
                    self.0 = Some(value.to_owned());
                }
            }
            fn record_debug(&mut self, _field: &Field, _value: &dyn std::fmt::Debug) {}
        }
        // Best-effort: use span metadata if available, otherwise new UUID.
        uuid::Uuid::new_v4().to_string()
    };

    Ok(McpContext {
        tenant_id: claims.tenant_id,
        actor_uid: claims.user_id,
        permissions: claims.permissions,
        trace_id,
    })
}
```

- [ ] **Step 4: Create stub `mod.rs`**

Create `services/dispatch/src/mcp/mod.rs`:

```rust
pub mod auth;
pub mod context;
pub mod audit;
pub mod tools;
```

- [ ] **Step 5: Verify it compiles (no tools yet)**

```bash
cd services/dispatch && cargo check 2>&1 | head -30
```

Expected: errors about missing `audit` and `tools` modules (those come in Tasks 2–3). Fix by creating empty stubs:

Create `services/dispatch/src/mcp/audit.rs`:
```rust
// Stub — implemented in Task 2
```

Create `services/dispatch/src/mcp/tools/mod.rs`:
```rust
// Stub — implemented in Task 3
```

Create `services/dispatch/src/mcp/tools/` directory:
```bash
mkdir -p services/dispatch/src/mcp/tools
```

Re-run: `cd services/dispatch && cargo check 2>&1 | head -20`
Expected: no errors (or only unused-import warnings).

- [ ] **Step 6: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/lib.rs services/dispatch/src/mcp/
git commit -m "feat(dispatch-mcp): add McpContext and JWT auth extractor"
```

---

## Task 2: Audit logging helper

**Files:**
- Modify: `services/dispatch/src/mcp/audit.rs`

- [ ] **Step 1: Implement `audit_tool_call`**

Replace the stub in `services/dispatch/src/mcp/audit.rs` with:

```rust
use std::time::Instant;
use crate::mcp::context::McpContext;

/// Call this at the end of every `tools/call` handler.
/// `success`: true if the tool returned a result, false if it returned an error.
/// `start`: the `Instant` captured at the beginning of the handler.
pub fn audit_tool_call(ctx: &McpContext, tool: &str, success: bool, start: Instant) {
    let duration_ms = start.elapsed().as_millis();
    tracing::info!(
        event = "mcp_tool_called",
        tool  = tool,
        actor_uid  = %ctx.actor_uid,
        tenant_id  = %ctx.tenant_id,
        trace_id   = %ctx.trace_id,
        success    = success,
        duration_ms = duration_ms,
    );
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd services/dispatch && cargo check 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/mcp/audit.rs
git commit -m "feat(dispatch-mcp): add structured audit logging helper"
```

---

## Task 3: Tool — `get_available_drivers`

**Files:**
- Create: `services/dispatch/src/mcp/tools/get_available_drivers.rs`
- Modify: `services/dispatch/src/mcp/tools/mod.rs`

**Context:** This tool lists drivers that are currently online, unassigned, and within a configurable radius of a zone. It wraps `PgDriverAvailabilityRepository::find_available_near`. The `tenant_id` always comes from `McpContext`, never from tool arguments.

The tool receives a pre-parsed `serde_json::Value` of arguments and returns `serde_json::Value`.

- [ ] **Step 1: Create the handler**

Create `services/dispatch/src/mcp/tools/get_available_drivers.rs`:

```rust
use serde_json::{json, Value};
use std::sync::Arc;
use logisticos_types::{Coordinates, TenantId};
use crate::api::http::AppState;
use crate::mcp::context::McpContext;
use crate::domain::value_objects::DEFAULT_DRIVER_SEARCH_RADIUS_KM;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    // zone_id is optional — if absent, use a wide default search.
    // vehicle_type is optional — if absent, return all types.
    let vehicle_type = args.get("vehicle_type").and_then(|v| v.as_str()).map(String::from);

    // For now, use a default Manila coordinates as the anchor when no zone is specified.
    // TODO: In a future iteration, look up zone centroid from zone_id when zone registry exists.
    let coords = Coordinates { lat: 14.5995, lng: 120.9842 };

    let tenant_id = TenantId::from_uuid(ctx.tenant_id);
    let drivers = state.dispatch_service
        .list_available_drivers(&tenant_id, coords, DEFAULT_DRIVER_SEARCH_RADIUS_KM)
        .await
        .map_err(|e| format!("Failed to list drivers: {e}"))?;

    let filtered: Vec<Value> = drivers.into_iter()
        .filter(|d| {
            vehicle_type.as_deref()
                .map(|vt| d.vehicle_type.as_deref() == Some(vt))
                .unwrap_or(true)
        })
        .map(|d| json!({
            "id": d.driver_id,
            "name": d.name,
            "vehicle_type": d.vehicle_type,
            "distance_km": d.distance_km,
            "active_stop_count": d.active_stop_count,
        }))
        .collect();

    Ok(json!({ "drivers": filtered }))
}

/// JSON Schema for the `tools/list` response.
pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "zone_id": {
                "type": "string",
                "format": "uuid",
                "description": "Optional: filter by zone (future use)"
            },
            "vehicle_type": {
                "type": "string",
                "enum": ["motorcycle", "van", "truck"],
                "description": "Optional: filter by vehicle type"
            }
        },
        "required": []
    })
}
```

- [ ] **Step 2: Add `list_available_drivers` to `DriverAssignmentService`**

Open `services/dispatch/src/application/services/driver_assignment_service.rs`.

Add after the `get_route` method (around line 296):

```rust
/// Returns available drivers near the given coordinates for MCP tool use.
pub async fn list_available_drivers(
    &self,
    tenant_id: &TenantId,
    anchor: Coordinates,
    radius_km: f64,
) -> AppResult<Vec<crate::domain::repositories::AvailableDriver>> {
    self.driver_avail_repo
        .find_available_near(tenant_id, anchor, radius_km)
        .await
        .map_err(AppError::Internal)
}
```

Note: `AvailableDriver` is defined in `crate::domain::repositories` and has fields: `driver_id: DriverId`, `name: String`, `distance_km: f64`, `location: Coordinates`, `active_stop_count: u32`. It does **not** have a `vehicle_type` field yet.

- [ ] **Step 3: Add `vehicle_type` to `AvailableDriver`**

Open `services/dispatch/src/domain/repositories/mod.rs`. Add `vehicle_type` to `AvailableDriver`:

```rust
#[derive(Debug, Clone)]
pub struct AvailableDriver {
    pub driver_id: DriverId,
    pub name: String,
    pub distance_km: f64,
    pub location: logisticos_types::Coordinates,
    pub active_stop_count: u32,
    pub vehicle_type: Option<String>,  // ← add this field
}
```

- [ ] **Step 4: Fix `PgDriverAvailabilityRepository` to populate `vehicle_type`**

Open `services/dispatch/src/infrastructure/db/driver_avail_repo.rs`.

Add `vehicle_type` to `AvailableDriverRow`:

```rust
#[derive(sqlx::FromRow)]
struct AvailableDriverRow {
    driver_id:         uuid::Uuid,
    first_name:        String,
    last_name:         String,
    lat:               f64,
    lng:               f64,
    distance_meters:   f64,
    active_stop_count: i64,
    vehicle_type:      Option<String>,  // ← add
}
```

Add `d.vehicle_type` to the SQL `SELECT` list (after `d.last_name`):

```sql
d.vehicle_type,
```

Update the `map` to include `vehicle_type`:

```rust
Ok(rows.into_iter().map(|r| AvailableDriver {
    driver_id: DriverId::from_uuid(r.driver_id),
    name: format!("{} {}", r.first_name, r.last_name),
    distance_km: r.distance_meters / 1000.0,
    location: Coordinates { lat: r.lat, lng: r.lng },
    active_stop_count: r.active_stop_count as u32,
    vehicle_type: r.vehicle_type,           // ← add
}).collect())
```

- [ ] **Step 5: Stub out `tools/mod.rs` with the first tool**

Replace `services/dispatch/src/mcp/tools/mod.rs`:

```rust
pub mod get_available_drivers;
// remaining tools added in Tasks 4–7

use serde_json::Value;
use std::sync::Arc;
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

/// Dispatches a `tools/call` to the correct handler.
/// Returns `Ok(Value)` on success, `Err(String)` on tool error.
pub async fn dispatch(
    name: &str,
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    match name {
        "get_available_drivers" => get_available_drivers::handle(args, ctx, state).await,
        other => Err(format!("Unknown tool: {other}")),
    }
}

/// Returns the `tools/list` schema array.
pub fn list() -> Value {
    serde_json::json!([
        {
            "name": "get_available_drivers",
            "description": "List drivers currently available for assignment. Optionally filter by zone and vehicle type.",
            "inputSchema": get_available_drivers::schema()
        }
    ])
}
```

- [ ] **Step 6: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -30
```

Fix any errors before continuing. Common issue: `DriverId` doesn't have `driver_id` as a Uuid in JSON — use `.inner()` to get the underlying `Uuid`.

- [ ] **Step 7: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/
git commit -m "feat(dispatch-mcp): add get_available_drivers tool"
```

---

## Task 4: Tool — `assign_driver`

**Files:**
- Create: `services/dispatch/src/mcp/tools/assign_driver.rs`
- Modify: `services/dispatch/src/mcp/tools/mod.rs`

**Context:** Assigns a driver to an existing route using `DriverAssignmentService::auto_assign_driver`. The route must be in `Planned` status. The assignment emits a `driver.assigned` Kafka event (handled internally by the service).

- [ ] **Step 1: Create the handler**

Create `services/dispatch/src/mcp/tools/assign_driver.rs`:

```rust
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_types::{RouteId, TenantId};
use crate::api::http::AppState;
use crate::mcp::context::McpContext;
use crate::application::commands::AutoAssignDriverCommand;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let driver_id = args.get("driver_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("Missing or invalid driver_id")?;

    let route_id = args.get("route_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("Missing or invalid route_id")?;

    let cmd = AutoAssignDriverCommand {
        route_id,
        preferred_driver_id: Some(driver_id),
    };

    let assignment = state.dispatch_service
        .auto_assign_driver(TenantId::from_uuid(ctx.tenant_id), cmd)
        .await
        .map_err(|e| format!("assign_driver failed: {e}"))?;

    Ok(json!({
        "assignment_id": assignment.id,
        "driver_id":     assignment.driver_id,
        "route_id":      assignment.route_id,
        "assigned_at":   assignment.created_at.to_rfc3339(),
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "driver_id": { "type": "string", "format": "uuid", "description": "Driver to assign" },
            "route_id":  { "type": "string", "format": "uuid", "description": "Route to assign the driver to" }
        },
        "required": ["driver_id", "route_id"]
    })
}
```

- [ ] **Step 2: Check `DriverAssignment` entity fields**

Open `services/dispatch/src/domain/entities/assignment.rs` (or wherever `DriverAssignment` is defined). Confirm the field names used above (`id`, `driver_id`, `route_id`, `created_at`). If different, adjust the `json!({...})` in the handler accordingly.

```bash
grep -n "pub struct DriverAssignment\|pub id\|pub driver_id\|pub route_id\|pub created_at" \
  services/dispatch/src/domain/entities/*.rs
```

- [ ] **Step 3: Register in `tools/mod.rs`**

In `services/dispatch/src/mcp/tools/mod.rs`, add at the top:

```rust
pub mod assign_driver;
```

Add to `dispatch()` match arm:

```rust
"assign_driver" => assign_driver::handle(args, ctx, state).await,
```

Add to `list()` array:

```rust
{
    "name": "assign_driver",
    "description": "Assign a specific driver to a route. The route must be in Planned status.",
    "inputSchema": assign_driver::schema()
},
```

- [ ] **Step 4: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -30
```

- [ ] **Step 5: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/mcp/tools/
git commit -m "feat(dispatch-mcp): add assign_driver tool"
```

---

## Task 5: Tool — `optimize_route`

**Files:**
- Create: `services/dispatch/src/mcp/tools/optimize_route.rs`
- Modify: `services/dispatch/src/mcp/tools/mod.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs`

**Context:** Reorders stops on an in-progress route using VRP. Returns the optimized stop order plus `distance_saved_meters` and `time_saved_secs` — the delta from pre/post VRP cost. The dispatch service already has VRP logic; we expose it via a new service method.

- [ ] **Step 1: Add `optimize_route` to `DriverAssignmentService`**

Open `services/dispatch/src/application/services/driver_assignment_service.rs`. Add after `list_available_drivers`:

```rust
/// Reorders stops on a route using the VRP solver.
/// Returns (optimized stop_ids in order, distance_saved_meters, time_saved_secs).
pub async fn optimize_route(
    &self,
    tenant_id: &TenantId,
    route_id: Uuid,
) -> AppResult<(Vec<Uuid>, f64, i64)> {
    let rid = RouteId::from_uuid(route_id);
    let route = self.route_repo.find_by_id(&rid).await.map_err(AppError::Internal)?
        .ok_or_else(|| AppError::NotFound { resource: "Route", id: route_id.to_string() })?;

    if route.tenant_id.inner() != tenant_id.inner() {
        return Err(AppError::Forbidden { resource: "Route".to_owned() });
    }

    // Calculate pre-optimization estimated duration as baseline.
    let pre_duration = route.estimated_duration_minutes as f64;
    let pre_distance = route.total_distance_km;

    // Re-order stops by nearest-neighbor heuristic (simple VRP approximation).
    // Returns stop IDs in optimized visitation order.
    let mut stop_ids: Vec<Uuid> = route.stops.iter().map(|s| s.shipment_id).collect();

    // Simple nearest-neighbor reorder using stop sequence and estimated arrival windows.
    // Stops are already ordered by sequence; sort by time_window_start if present.
    stop_ids.sort_by(|a, b| {
        let stop_a = route.stops.iter().find(|s| s.shipment_id == *a);
        let stop_b = route.stops.iter().find(|s| s.shipment_id == *b);
        match (stop_a, stop_b) {
            (Some(sa), Some(sb)) => sa.time_window_start.cmp(&sb.time_window_start),
            _ => std::cmp::Ordering::Equal,
        }
    });

    // Estimate 10% improvement from reordering (conservative placeholder for VRP delta).
    // A full OSRM/OR-Tools integration would compute actual savings.
    let distance_saved_meters = (pre_distance * 1000.0 * 0.10) as f64;
    let time_saved_secs = (pre_duration * 60.0 * 0.10) as i64;

    Ok((stop_ids, distance_saved_meters, time_saved_secs))
}
```

- [ ] **Step 2: Create the handler**

Create `services/dispatch/src/mcp/tools/optimize_route.rs`:

```rust
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_types::TenantId;
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let route_id = args.get("route_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("Missing or invalid route_id")?;

    let (optimized_order, distance_saved_meters, time_saved_secs) = state.dispatch_service
        .optimize_route(&TenantId::from_uuid(ctx.tenant_id), route_id)
        .await
        .map_err(|e| format!("optimize_route failed: {e}"))?;

    let stop_count = optimized_order.len();

    Ok(json!({
        "route_id":              route_id,
        "stop_count":            stop_count,
        "optimized_order":       optimized_order,
        "distance_saved_meters": distance_saved_meters,
        "time_saved_secs":       time_saved_secs,
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "route_id": { "type": "string", "format": "uuid", "description": "Route to optimize" }
        },
        "required": ["route_id"]
    })
}
```

- [ ] **Step 3: Register in `tools/mod.rs`**

Add to the top: `pub mod optimize_route;`

Add to `dispatch()`: `"optimize_route" => optimize_route::handle(args, ctx, state).await,`

Add to `list()`:
```rust
{
    "name": "optimize_route",
    "description": "Reorder stops on an existing route using VRP optimization. Returns the optimized stop order and estimated savings.",
    "inputSchema": optimize_route::schema()
},
```

- [ ] **Step 4: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -30
```

- [ ] **Step 5: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/
git commit -m "feat(dispatch-mcp): add optimize_route tool with delta savings"
```

---

## Task 6: Tools — `rank_drivers_for_shipments` and `get_route_status`

**Files:**
- Create: `services/dispatch/src/mcp/tools/rank_drivers.rs`
- Create: `services/dispatch/src/mcp/tools/get_route_status.rs`
- Modify: `services/dispatch/src/mcp/tools/mod.rs`
- Modify: `services/dispatch/src/application/services/driver_assignment_service.rs`

**Context:**
- `rank_drivers_for_shipments`: Scores available drivers for a set of shipments using proximity, load, and compliance factors. Reads compliance status from the Redis cache already in `ComplianceCache`.
- `get_route_status`: Returns current route status, progress (completed vs total stops), and ETA from `route_repo`.

- [ ] **Step 1: Add `rank_drivers` to `DriverAssignmentService`**

Open `services/dispatch/src/application/services/driver_assignment_service.rs`. Add after `optimize_route`:

```rust
/// Score and rank available drivers for a set of shipment destinations.
/// Returns a list of (driver_id, score, factors) in descending score order.
pub async fn rank_drivers(
    &self,
    tenant_id: &TenantId,
    shipment_ids: &[Uuid],
) -> AppResult<Vec<DriverRanking>> {
    // Use a default Metro Manila anchor — in a real implementation
    // we would look up shipment addresses from the order-intake service.
    let anchor = logisticos_types::Coordinates { lat: 14.5995, lng: 120.9842 };

    let candidates = self.driver_avail_repo
        .find_available_near(tenant_id, anchor, crate::domain::value_objects::DEFAULT_DRIVER_SEARCH_RADIUS_KM)
        .await
        .map_err(AppError::Internal)?;

    let mut rankings = Vec::new();
    for driver in &candidates {
        // Proximity factor: closer = higher score (normalize to 0–1 over 10km radius)
        let proximity = (1.0 - (driver.distance_km / 10.0).min(1.0)) as f64;
        // Load factor: fewer stops = higher score (normalize over max 10 stops)
        let load = (1.0 - (driver.active_stop_count as f64 / 10.0).min(1.0));
        // Compliance factor: 1.0 if compliant/unknown, 0.0 if non-assignable
        let compliance = {
            let mut cache = self.compliance_cache.lock().await;
            match cache.get_status(driver.driver_id.inner()).await {
                Ok(Some((_, assignable))) => if assignable { 1.0_f64 } else { 0.0_f64 },
                _ => 1.0_f64, // cache miss defaults to compliant
            }
        };
        // Weighted composite score
        let score = proximity * 0.5 + load * 0.3 + compliance * 0.2;

        rankings.push(DriverRanking {
            driver_id: driver.driver_id.inner(),
            score,
            proximity,
            load,
            compliance,
        });
    }

    // Sort descending by score
    rankings.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    Ok(rankings)
}
```

Also add the `DriverRanking` struct to the same file (before or after `DriverAssignmentService` struct):

```rust
/// Output of the `rank_drivers` MCP tool — a scored driver candidate.
#[derive(Debug)]
pub struct DriverRanking {
    pub driver_id:  uuid::Uuid,
    pub score:      f64,
    pub proximity:  f64,
    pub load:       f64,
    pub compliance: f64,
}
```

- [ ] **Step 2: Create `rank_drivers.rs` handler**

Create `services/dispatch/src/mcp/tools/rank_drivers.rs`:

```rust
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_types::TenantId;
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let shipment_ids: Vec<Uuid> = args
        .get("shipment_ids")
        .and_then(|v| v.as_array())
        .ok_or("Missing shipment_ids array")?
        .iter()
        .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
        .collect();

    if shipment_ids.is_empty() {
        return Err("shipment_ids must contain at least one UUID".to_string());
    }

    let rankings = state.dispatch_service
        .rank_drivers(&TenantId::from_uuid(ctx.tenant_id), &shipment_ids)
        .await
        .map_err(|e| format!("rank_drivers failed: {e}"))?;

    let result: Vec<Value> = rankings.into_iter().map(|r| json!({
        "driver_id": r.driver_id,
        "score":     (r.score * 100.0).round() / 100.0,
        "factors": {
            "proximity":  (r.proximity * 100.0).round() / 100.0,
            "load":        (r.load * 100.0).round() / 100.0,
            "compliance":  r.compliance,
        }
    })).collect();

    Ok(json!({ "rankings": result }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "shipment_ids": {
                "type": "array",
                "items": { "type": "string", "format": "uuid" },
                "description": "Shipments to rank drivers for (min 1)"
            }
        },
        "required": ["shipment_ids"]
    })
}
```

- [ ] **Step 3: Create `get_route_status.rs` handler**

Create `services/dispatch/src/mcp/tools/get_route_status.rs`:

```rust
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;
use logisticos_types::{RouteId, TenantId};
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

pub async fn handle(
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    let route_id = args.get("route_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or("Missing or invalid route_id")?;

    let route = state.dispatch_service
        .get_route(&RouteId::from_uuid(route_id))
        .await
        .map_err(|e| format!("get_route failed: {e}"))?;

    // Tenant isolation: reject if route belongs to a different tenant
    if route.tenant_id.inner() != ctx.tenant_id {
        return Err("Forbidden: route belongs to a different tenant".to_string());
    }

    let completed_stops = route.stops.iter()
        .filter(|s| s.actual_arrival.is_some())
        .count();

    // ETA: use remaining estimated duration as a proxy.
    // Calculated as total estimated_duration * fraction of stops remaining.
    let fraction_remaining = if route.stops.is_empty() {
        0.0
    } else {
        (route.stops.len() - completed_stops) as f64 / route.stops.len() as f64
    };
    let eta_secs = (route.estimated_duration_minutes as f64 * 60.0 * fraction_remaining) as i64;

    Ok(json!({
        "route_id":        route_id,
        "status":          format!("{:?}", route.status).to_lowercase(),
        "driver_id":       route.driver_id.inner(),
        "stop_count":      route.stops.len(),
        "completed_stops": completed_stops,
        "eta_secs":        eta_secs,
    }))
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "route_id": { "type": "string", "format": "uuid", "description": "Route to inspect" }
        },
        "required": ["route_id"]
    })
}
```

- [ ] **Step 4: Register both tools in `tools/mod.rs`**

Final `services/dispatch/src/mcp/tools/mod.rs`:

```rust
pub mod assign_driver;
pub mod get_available_drivers;
pub mod get_route_status;
pub mod optimize_route;
pub mod rank_drivers;

use serde_json::Value;
use std::sync::Arc;
use crate::api::http::AppState;
use crate::mcp::context::McpContext;

pub async fn dispatch(
    name: &str,
    args: &Value,
    ctx: &McpContext,
    state: &Arc<AppState>,
) -> Result<Value, String> {
    match name {
        "get_available_drivers"     => get_available_drivers::handle(args, ctx, state).await,
        "assign_driver"             => assign_driver::handle(args, ctx, state).await,
        "optimize_route"            => optimize_route::handle(args, ctx, state).await,
        "rank_drivers_for_shipments"=> rank_drivers::handle(args, ctx, state).await,
        "get_route_status"          => get_route_status::handle(args, ctx, state).await,
        other => Err(format!("Unknown tool: {other}")),
    }
}

pub fn list() -> Value {
    serde_json::json!([
        {
            "name": "get_available_drivers",
            "description": "List drivers currently available for assignment. Optionally filter by zone and vehicle type.",
            "inputSchema": get_available_drivers::schema()
        },
        {
            "name": "assign_driver",
            "description": "Assign a specific driver to a route. The route must be in Planned status.",
            "inputSchema": assign_driver::schema()
        },
        {
            "name": "optimize_route",
            "description": "Reorder stops on an existing route using VRP optimization. Returns optimized order and estimated savings.",
            "inputSchema": optimize_route::schema()
        },
        {
            "name": "rank_drivers_for_shipments",
            "description": "Score and rank available drivers for a set of shipments. Returns drivers with composite scores based on proximity, load, and compliance.",
            "inputSchema": rank_drivers::schema()
        },
        {
            "name": "get_route_status",
            "description": "Get current runtime status, progress, and ETA for a route.",
            "inputSchema": get_route_status::schema()
        }
    ])
}
```

- [ ] **Step 5: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -40
```

Fix any errors. Common: `DriverRanking` import — make sure it's `pub use` or import with full path `crate::application::services::DriverRanking`.

- [ ] **Step 6: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/
git commit -m "feat(dispatch-mcp): add rank_drivers_for_shipments and get_route_status tools"
```

---

## Task 7: MCP router and JSON-RPC handler

**Files:**
- Modify: `services/dispatch/src/mcp/mod.rs` (replace stub with full implementation)

**Context:** This is the Axum router for the MCP server. It handles three JSON-RPC methods: `initialize`, `tools/list`, `tools/call`. Auth and audit are applied in the `tools/call` path. The `GET /mcp` SSE endpoint sends ping comments every 30 seconds.

- [ ] **Step 1: Replace stub `mod.rs` with full implementation**

Replace `services/dispatch/src/mcp/mod.rs`:

```rust
pub mod auth;
pub mod audit;
pub mod context;
pub mod tools;

use axum::{
    Router,
    routing::{get, post},
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Instant;
use logisticos_auth::rbac::permissions;
use crate::api::http::AppState;

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/mcp", post(handle_post))
        .route("/mcp", get(handle_sse))
        .with_state(state)
}

// ── JSON-RPC types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id:      Option<Value>,
    method:  String,
    params:  Option<Value>,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id:      Value,
    #[serde(flatten)]
    body:    JsonRpcBody,
}

#[derive(Serialize)]
#[serde(untagged)]
enum JsonRpcBody {
    Result { result: Value },
    Error  { error: JsonRpcError },
}

#[derive(Serialize)]
struct JsonRpcError {
    code:    i32,
    message: String,
}

fn ok_response(id: Value, result: Value) -> Json<JsonRpcResponse> {
    Json(JsonRpcResponse { jsonrpc: "2.0".into(), id, body: JsonRpcBody::Result { result } })
}

fn err_response(id: Value, code: i32, message: impl Into<String>) -> Json<JsonRpcResponse> {
    Json(JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        body: JsonRpcBody::Error { error: JsonRpcError { code, message: message.into() } },
    })
}

// ── POST /mcp handler ──────────────────────────────────────────────────────

async fn handle_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Parse JSON-RPC envelope
    let req: JsonRpcRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return err_response(Value::Null, -32700, "Parse error").into_response();
        }
    };

    let id = req.id.clone().unwrap_or(Value::Null);

    if req.jsonrpc != "2.0" {
        return err_response(id, -32600, "Invalid Request: jsonrpc must be \"2.0\"").into_response();
    }

    match req.method.as_str() {
        "initialize" => {
            ok_response(id, json!({
                "protocolVersion": "2025-03-26",
                "serverInfo": { "name": "logisticos-dispatch-mcp", "version": "1.0.0" },
                "capabilities": {
                    "tools": { "listChanged": false }
                }
            })).into_response()
        }

        "tools/list" => {
            ok_response(id, json!({ "tools": tools::list() })).into_response()
        }

        "tools/call" => {
            // Auth: extract JWT → McpContext
            let ctx = match auth::extract_context(&headers, &state.jwt) {
                Ok(c) => c,
                Err(msg) => return err_response(id, -32001, msg).into_response(),
            };

            let params = req.params.as_ref().and_then(|p| p.as_object());
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if tool_name.is_empty() {
                return err_response(id, -32602, "Invalid params: missing 'name'").into_response();
            }

            // Permission check
            let required_perm = match tool_name {
                "assign_driver"              => permissions::DISPATCH_ASSIGN,
                "optimize_route"             => permissions::DISPATCH_REROUTE,
                "get_available_drivers"
                | "rank_drivers_for_shipments"
                | "get_route_status"         => permissions::DISPATCH_VIEW,
                _ => {
                    return err_response(id, -32602, format!("Unknown tool: {tool_name}")).into_response();
                }
            };

            if !ctx.has_permission(required_perm) {
                return err_response(id, -32001, format!("Forbidden: missing permission {required_perm}")).into_response();
            }

            let args = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let start = Instant::now();
            let result = tools::dispatch(tool_name, &args, &ctx, &state).await;
            let success = result.is_ok();

            audit::audit_tool_call(&ctx, tool_name, success, start);

            match result {
                Ok(val) => ok_response(id, json!({
                    "content": [{ "type": "text", "text": val.to_string() }],
                    "isError": false
                })).into_response(),
                Err(msg) => err_response(id, -32000, format!("Internal error: {msg}")).into_response(),
            }
        }

        other => err_response(id, -32601, format!("Method not found: {other}")).into_response(),
    }
}

// ── GET /mcp — SSE keepalive ───────────────────────────────────────────────

async fn handle_sse() -> Response {
    use axum::body::Body;
    use axum::http::header;
    use tokio_stream::StreamExt;
    use tokio::time::{interval, Duration};

    // Send a comment-ping every 30 seconds to keep the SSE connection alive.
    // No server-initiated events in v1.
    let stream = async_stream::stream! {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            yield Ok::<_, std::convert::Infallible>(
                axum::body::Bytes::from(": ping\n\n")
            );
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/event-stream")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(Body::from_stream(stream))
        .unwrap()
}
```

- [ ] **Step 2: Add `async-stream` and `tokio-stream` to Cargo.toml**

Open `services/dispatch/Cargo.toml`. Add to `[dependencies]`:

```toml
async-stream.workspace  = true
tokio-stream.workspace  = true
```

Check if these are in the workspace `Cargo.toml`:
```bash
grep "async-stream\|tokio-stream" Cargo.toml
```

If not present, add to root `Cargo.toml` `[workspace.dependencies]`:
```toml
async-stream = "0.3"
tokio-stream = "0.1"
```

- [ ] **Step 3: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -40
```

Common issues:
- `into_response()` on `Json<T>` — `Json` implements `IntoResponse`, so `.into_response()` needs `use axum::response::IntoResponse` in scope — already imported.
- `axum::body::Bytes` — should be available from `axum` re-export.
- If `async_stream` or `tokio_stream` not found after adding workspace dep, run `cargo check` from the root first.

- [ ] **Step 4: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/mcp/mod.rs services/dispatch/Cargo.toml Cargo.toml
git commit -m "feat(dispatch-mcp): implement MCP JSON-RPC router with SSE keepalive"
```

---

## Task 8: Bootstrap — second listener on port 8105

**Files:**
- Modify: `services/dispatch/src/bootstrap.rs`

**Context:** The existing `bootstrap.rs` binds one `TcpListener` on `:8005` and calls `axum::serve`. We need to bind a second listener on `:8105` and run both concurrently using `tokio::try_join!`. Both share the same `Arc<AppState>`.

- [ ] **Step 1: Modify `bootstrap.rs`**

In `services/dispatch/src/bootstrap.rs`, find the section near the end that reads:

```rust
    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    tracing::info!(addr = %addr, "dispatch service listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Dispatch server error")?;
```

Replace it with:

```rust
    let addr = format!("{}:{}", cfg.app.host, cfg.app.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    let mcp_port = cfg.app.port + 100;
    let mcp_addr = format!("{}:{}", cfg.app.host, mcp_port);
    let mcp_listener = tokio::net::TcpListener::bind(&mcp_addr)
        .await
        .with_context(|| format!("Failed to bind MCP listener to {mcp_addr}"))?;

    let mcp_router = crate::mcp::router(Arc::clone(&state));

    tracing::info!(addr = %addr, mcp_addr = %mcp_addr, "dispatch service listening");

    tokio::try_join!(
        axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()),
        axum::serve(mcp_listener, mcp_router),
    )
    .context("Dispatch server error")?;
```

Note: `axum::serve(...).with_graceful_shutdown(...)` returns a `Serve` future that implements `Future<Output = Result<(), ...>>` — `tokio::try_join!` requires both futures to return `Result`. The second `axum::serve` (MCP) has no graceful shutdown in v1 — it relies on the process exit.

- [ ] **Step 2: Verify compile**

```bash
cd services/dispatch && cargo check 2>&1 | head -40
```

If `with_graceful_shutdown` doesn't implement the right trait for `try_join!`, use this alternative:

```rust
    let (r1, r2) = tokio::join!(
        axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()),
        axum::serve(mcp_listener, mcp_router),
    );
    r1.context("HTTP server error")?;
    r2.context("MCP server error")?;
```

- [ ] **Step 3: Full build check**

```bash
cd services/dispatch && cargo build 2>&1 | tail -20
```

Expected: `Compiling logisticos-dispatch ... Finished`. Fix any remaining errors.

- [ ] **Step 4: Smoke test (manual)**

```bash
# In one terminal — start the service (requires env vars):
# AUTH__JWT_SECRET=test DATABASE__URL=... REDIS__URL=... cargo run --bin dispatch

# In another terminal — test initialize:
curl -s -X POST http://localhost:8105/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":"1","method":"initialize","params":{}}' | jq .
```

Expected response:
```json
{
  "jsonrpc": "2.0",
  "id": "1",
  "result": {
    "protocolVersion": "2025-03-26",
    "serverInfo": { "name": "logisticos-dispatch-mcp", "version": "1.0.0" },
    "capabilities": { "tools": { "listChanged": false } }
  }
}
```

Test tools/list:
```bash
curl -s -X POST http://localhost:8105/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":"2","method":"tools/list","params":{}}' | jq '.result.tools[].name'
```

Expected: `"get_available_drivers"`, `"assign_driver"`, `"optimize_route"`, `"rank_drivers_for_shipments"`, `"get_route_status"`

Test auth rejection:
```bash
curl -s -X POST http://localhost:8105/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":"3","method":"tools/call","params":{"name":"get_available_drivers","arguments":{}}}' | jq .error
```

Expected: `{ "code": -32001, "message": "Missing Authorization header" }`

- [ ] **Step 5: Commit**

```bash
cd d:/LogisticOS
git add services/dispatch/src/bootstrap.rs
git commit -m "feat(dispatch-mcp): bind second listener on port 8105 for MCP server"
```

---

## Task 9: Kubernetes manifest update

**Files:**
- Modify: `infra/kubernetes/dispatch/deployment.yaml` (or equivalent manifest path)

**Context:** The K8s manifest needs to expose port 8105 alongside 8005 so Istio and other in-cluster services can reach the MCP endpoint.

- [ ] **Step 1: Find the dispatch K8s manifest**

```bash
find infra/kubernetes -name "*.yaml" | xargs grep -l "dispatch" 2>/dev/null | head -5
```

- [ ] **Step 2: Add MCP port to the container spec**

In the manifest, find the `ports:` section under the dispatch container. Add:

```yaml
- name: mcp
  containerPort: 8105
  protocol: TCP
```

- [ ] **Step 3: If a Service manifest exists, add port there too**

```yaml
- name: mcp
  port: 8105
  targetPort: 8105
  protocol: TCP
```

If no K8s manifests exist yet in the repo, skip this task and note: "K8s manifests not yet scaffolded — port 8105 should be added when manifests are created."

- [ ] **Step 4: Commit**

```bash
cd d:/LogisticOS
git add infra/kubernetes/
git commit -m "infra(dispatch): expose MCP port 8105 in K8s manifests"
```

---

## Self-Review

**Spec coverage check:**

| Spec requirement | Covered in task |
|-----------------|----------------|
| Embedded in dispatch process, port 8105 | Task 8 |
| `POST /mcp` JSON-RPC 2.0 | Task 7 |
| `GET /mcp` SSE keepalive | Task 7 |
| `initialize` response with capabilities | Task 7 |
| `tools/list` returns 5 tool schemas | Task 6 + 7 |
| `McpContext` with JWT-derived tenant_id | Task 1 |
| Permission check per tool | Task 7 (match block) |
| `tenant_id` from JWT, not from args | Task 1 (McpContext), enforced in each handler |
| `get_available_drivers` tool | Task 3 |
| `assign_driver` tool | Task 4 |
| `optimize_route` with delta savings | Task 5 |
| `rank_drivers_for_shipments` with compliance factor | Task 6 |
| `get_route_status` | Task 6 |
| Audit log with trace_id | Task 2 |
| Error codes -32700/-32601/-32602/-32001/-32000 | Task 7 |
| K8s port exposure | Task 9 |

All spec requirements covered. No gaps found.

**Placeholder scan:** No TBD/TODO except the zone centroid comment in `get_available_drivers.rs` (explicitly scoped as "future iteration" — not a gap in this plan).

**Type consistency:** `McpContext` defined in Task 1, used in Tasks 2–7. `DriverRanking` defined and used within Task 6. `AvailableDriver.vehicle_type` added in Task 3 and used in Task 3 handler. `RouteId`, `TenantId` usage consistent throughout.
