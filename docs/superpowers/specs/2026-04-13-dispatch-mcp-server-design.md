# Dispatch MCP Server — Design Spec

**Date:** 2026-04-13  
**Status:** Approved  
**Author:** Principal Software Architect  
**ADR Reference:** [ADR-0004](../../adr/0004-mcp-for-ai-interoperability.md)

---

## 1. Context

LogisticOS is an Agentic-as-a-Service platform. AI agents (Claude, Gemma, LangGraph workflows) must interact with operational services through a standardized, auditable interface. ADR-0004 mandates that every service expose an MCP server alongside its HTTP API.

The dispatch service is the first to receive an MCP server. It handles driver assignment, route optimization, and dispatch status — the core actions AI agents need to automate last-mile delivery operations.

---

## 2. Architecture

### Deployment

The MCP server runs **embedded in the dispatch service process**, sharing the existing `AppState` (dispatch_service, drivers_repo, jwt, queue_repo). No sidecar, no separate binary.

```
services/dispatch process
├── Port 8005  — HTTP API (existing, unchanged)
└── Port 8105  — MCP Server (new)
                  ├── POST /mcp  — JSON-RPC 2.0 tool calls
                  └── GET  /mcp  — SSE stream (Streamable HTTP spec)
```

### Module Structure

```
services/dispatch/src/
└── mcp/
    ├── mod.rs          — Axum router, initialize/tools/call dispatch
    ├── auth.rs         — JWT extraction + RBAC, returns McpContext
    ├── context.rs      — McpContext struct definition
    ├── audit.rs        — structured audit log per tool invocation
    └── tools/
        ├── mod.rs      — tool registry (name → handler dispatch)
        ├── get_available_drivers.rs
        ├── assign_driver.rs
        ├── optimize_route.rs
        ├── rank_drivers.rs
        └── get_route_status.rs
```

`bootstrap.rs` starts both listeners. The MCP router is registered as a separate `axum::Router` on the second `TcpListener`.

---

## 3. Transport: Streamable HTTP (2025-03-26 spec)

### Stateless design

This implementation is **stateless** — no session negotiation, no `session_id` in URLs. The `GET /mcp` endpoint keeps the SSE connection open for server-initiated notifications but sends none in v1 (no server-push tools). The `initialize` response declares this capability explicitly.

### `POST /mcp` — Tool invocation

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "method": "tools/call",
  "params": {
    "name": "assign_driver",
    "arguments": {
      "driver_id": "550e8400-e29b-41d4-a716-446655440000",
      "route_id":  "7c9e6679-7425-40de-944b-e07fc1f90ae7"
    }
  }
}
```

**Response (inline, non-streaming):**
```json
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "result": {
    "content": [{ "type": "text", "text": "{\"assignment_id\":\"...\",\"assigned_at\":\"...\"}" }],
    "isError": false
  }
}
```

### Supported JSON-RPC methods

| Method | Description |
|--------|-------------|
| `initialize` | Handshake — returns server info + capabilities |
| `tools/list` | Returns all 5 tool schemas with JSON Schema input definitions |
| `tools/call` | Invokes a named tool with arguments |

### `initialize` response

```json
{
  "jsonrpc": "2.0",
  "id": "init-1",
  "result": {
    "protocolVersion": "2025-03-26",
    "serverInfo": { "name": "logisticos-dispatch-mcp", "version": "1.0.0" },
    "capabilities": {
      "tools": { "listChanged": false }
    }
  }
}
```

### `GET /mcp` — SSE stream

Returns `Content-Type: text/event-stream`. Sends a single `ping` comment every 30 seconds to keep the connection alive. No event types emitted in v1.

---

## 4. Security: `McpContext`

### Auth flow

1. Extract `Authorization: Bearer <token>` from HTTP request headers.
2. Verify JWT using `JwtService` from `AppState` — same validation as the HTTP API.
3. Construct `McpContext` from verified claims.
4. Return JSON-RPC error `-32001` on any auth failure (missing token, invalid, expired).

### `McpContext` struct

```rust
pub struct McpContext {
    pub tenant_id: Uuid,
    pub actor_uid: Uuid,
    pub permissions: Vec<String>,
    pub trace_id: String,   // extracted from tracing::Span::current()
}
```

Every tool handler receives `&McpContext`. Repository calls accept `&McpContext`, never a raw `tenant_id` argument. This makes cross-tenant data access a compile-time impossibility — a developer cannot call a repo without providing the context, and the context tenant is always JWT-derived.

### Per-tool permission requirements

| Tool | Required permission |
|------|-------------------|
| `get_available_drivers` | `DISPATCH_READ` |
| `assign_driver` | `DISPATCH_WRITE` |
| `optimize_route` | `DISPATCH_WRITE` |
| `rank_drivers_for_shipments` | `DISPATCH_READ` |
| `get_route_status` | `DISPATCH_READ` |

`tenant_id` in tool arguments is **ignored** — `McpContext.tenant_id` is always used for all repo queries.

---

## 5. Tool Definitions

### `get_available_drivers`

Returns drivers currently available for assignment within the tenant's operational zone.

**Input schema:**
```json
{
  "zone_id":      { "type": "string", "format": "uuid", "description": "Filter by zone (optional)" },
  "vehicle_type": { "type": "string", "enum": ["motorcycle", "van", "truck"], "description": "Filter by vehicle type (optional)" }
}
```

**Output:**
```json
{
  "drivers": [
    {
      "id": "...",
      "name": "...",
      "vehicle_type": "motorcycle",
      "current_zone_id": "...",
      "status": "available"
    }
  ]
}
```

---

### `assign_driver`

Assigns a driver to a route. Emits `driver.assigned` Kafka event on success.

**Input schema:**
```json
{
  "driver_id": { "type": "string", "format": "uuid", "description": "Driver to assign" },
  "route_id":  { "type": "string", "format": "uuid", "description": "Route to assign the driver to" }
}
```

**Output:**
```json
{
  "assignment_id": "...",
  "driver_id": "...",
  "route_id": "...",
  "assigned_at": "2026-04-13T10:00:00Z"
}
```

---

### `optimize_route`

Reorders stops on an existing in-progress route using the VRP solver. Returns the optimized stop order and the savings delta so agents can explain their action to human dispatchers.

**Input schema:**
```json
{
  "route_id": { "type": "string", "format": "uuid", "description": "Route to optimize" }
}
```

**Output:**
```json
{
  "route_id": "...",
  "stop_count": 7,
  "optimized_order": ["stop_uuid_1", "stop_uuid_2", "..."],
  "distance_saved_meters": 3200,
  "time_saved_secs": 720
}
```

The `distance_saved_meters` and `time_saved_secs` are computed from the pre/post route cost already calculated by the VRP solver — zero additional overhead.

---

### `rank_drivers_for_shipments`

Scores and ranks available drivers for a set of shipments before assignment. Allows agents to reason about the best assignment strategy before committing.

**Input schema:**
```json
{
  "shipment_ids": {
    "type": "array",
    "items": { "type": "string", "format": "uuid" },
    "description": "Shipments to rank drivers for"
  }
}
```

**Output:**
```json
{
  "rankings": [
    {
      "driver_id": "...",
      "score": 0.91,
      "factors": {
        "proximity": 0.95,
        "load": 0.88,
        "compliance": 1.0
      }
    }
  ]
}
```

`compliance` factor pulls from the Redis compliance cache (already maintained by the dispatch service via `compliance.status_changed` events).

---

### `get_route_status`

Returns the current runtime status of a route including completion progress and ETA.

**Input schema:**
```json
{
  "route_id": { "type": "string", "format": "uuid", "description": "Route to inspect" }
}
```

**Output:**
```json
{
  "route_id": "...",
  "status": "in_progress",
  "driver_id": "...",
  "stop_count": 7,
  "completed_stops": 3,
  "eta_secs": 2700
}
```

---

## 6. Audit Logging

Every `tools/call` invocation writes a structured `tracing::info!` event:

```json
{
  "event":       "mcp_tool_called",
  "tool":        "assign_driver",
  "actor_uid":   "550e8400-...",
  "tenant_id":   "7c9e6679-...",
  "trace_id":    "4bf92f3577b34da6a3ce929d0e0e4736",
  "success":     true,
  "duration_ms": 12
}
```

The `trace_id` is extracted from `tracing::Span::current()` and links the MCP call to all downstream DB queries within the same trace. In Grafana/Loki, an operator can answer "why did the AI assign this driver?" by filtering on `trace_id` across the MCP call and the database spans.

This satisfies UAE 2026 data compliance audit trail requirements without a separate audit store.

---

## 7. Error Handling

| Scenario | JSON-RPC error code | Message |
|----------|--------------------|---------| 
| Invalid JSON body | `-32700` | `"Parse error"` |
| Unknown method (not initialize/tools/list/tools/call) | `-32601` | `"Method not found"` |
| Missing or invalid arguments | `-32602` | `"Invalid params: <detail>"` |
| Authentication failure | `-32001` | `"Unauthorized"` |
| Insufficient permissions | `-32001` | `"Forbidden: missing permission <PERM>"` |
| Tool execution error (DB, service layer) | `-32000` | `"Internal error: <message>"` |
| Unknown tool name in tools/call | `-32602` | `"Unknown tool: <name>"` |

---

## 8. Bootstrap Integration

`bootstrap.rs` changes:

```rust
// Bind two listeners
let http_listener = TcpListener::bind("0.0.0.0:8005").await?;
let mcp_listener  = TcpListener::bind("0.0.0.0:8105").await?;

// Build MCP router (separate from HTTP router)
let mcp_router = mcp::router(Arc::clone(&state));

// Serve both concurrently
tokio::try_join!(
    axum::serve(http_listener, http_router),
    axum::serve(mcp_listener, mcp_router),
)?;
```

---

## 9. Kubernetes / Istio

- Port `8105` added to the dispatch service `ContainerPort` list in the K8s manifest.
- Istio `VirtualService` exposes port `8105` as `mcp` protocol within the mesh.
- External access to port `8105` is **not exposed** through the Envoy API gateway — MCP is an internal mesh interface only. AI agents access it via in-cluster service discovery.

---

## 10. Future Extension

This design establishes the pattern for all 17 services. The `mcp/` module structure is intentionally replicable:
- Copy `mod.rs`, `auth.rs`, `context.rs`, `audit.rs` verbatim to the next service.
- Add service-specific `tools/` handlers.
- Update `bootstrap.rs` with the service's MCP port (convention: HTTP port + 100, e.g. identity: 8101, order-intake: 8104).

Port convention: `HTTP port + 100`.
