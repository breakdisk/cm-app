# ADR-0004: Model Context Protocol (MCP) for AI Agent Interoperability

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Staff ML Engineer, CTO

## Context

The AI Intelligence Layer (Service 16) hosts multiple AI agents: Dispatch Agent, Customer Support Agent, Marketing Agent, Operations Copilot. These agents need to:

1. **Access live operational data** — current driver locations, shipment statuses, hub capacity — without tight coupling to each service's internal API
2. **Take actions** — reschedule deliveries, send notifications, reassign drivers — through a standardized, auditable interface
3. **Compose with external AI tools** — Claude API, future fine-tuned models — without rewriting agent infrastructure
4. **Be extended by tenants** (Enterprise tier) — merchants want to connect their own AI tools to LogisticOS data

The AI Layer currently uses direct gRPC/REST calls to each service. This creates a fragile, tightly-coupled web of service dependencies for each agent.

## Decision

Implement **Model Context Protocol (MCP)** as the standard interface between the AI Intelligence Layer and all operational services.

Each operational service exposes an **MCP Server** alongside its existing HTTP/gRPC APIs. The AI Layer (and external Enterprise integrations) consume data and invoke actions exclusively through MCP.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    AI INTELLIGENCE LAYER                    │
│  Dispatch Agent │ Support Agent │ Marketing Agent │ Copilot │
│              (Claude API + LangGraph)                       │
└───────────────────────┬─────────────────────────────────────┘
                        │ MCP Client (Rust/Python)
          ┌─────────────┴──────────────────────┐
          │         MCP Router / Registry       │
          └─────┬──────┬──────┬──────┬──────────┘
                │      │      │      │
         ┌──────┘  ┌───┘  ┌───┘  ┌───┘
         ▼         ▼      ▼      ▼
   dispatch-mcp  order  driver  engagement
   MCP Server   MCP    MCP     MCP Server
```

## MCP Servers per Service

| Service | MCP Tools Exposed |
|---------|------------------|
| **Dispatch** | `get_available_drivers`, `assign_driver`, `optimize_route`, `get_route_status` |
| **Order Intake** | `get_shipment`, `list_pending_shipments`, `reschedule_delivery`, `cancel_shipment` |
| **Driver Ops** | `get_driver_location`, `get_driver_workload`, `send_driver_instruction` |
| **Engagement** | `send_notification`, `create_campaign`, `get_customer_preferences` |
| **CDP** | `get_customer_profile`, `get_shipment_history`, `get_churn_score` |
| **Payments** | `get_cod_balance`, `generate_invoice`, `get_payment_status` |
| **Fleet** | `get_vehicle_status`, `get_fleet_availability` |
| **Analytics** | `get_delivery_metrics`, `get_zone_demand_forecast`, `get_driver_performance` |
| **Hub Ops** | `get_hub_capacity`, `get_parcel_status`, `schedule_dock` |

## MCP Resources (Read-only data feeds)

- `logisticos://shipments/{id}` — real-time shipment state
- `logisticos://drivers/active` — currently active drivers with last-known location
- `logisticos://hubs/{id}/capacity` — live hub occupancy
- `logisticos://zones/{id}/demand` — demand forecast for a delivery zone

## Implementation

- **Rust MCP SDK**: Each service embeds a lightweight MCP server using the `rmcp` crate (or Axum-based custom implementation following the MCP spec)
- **MCP Registry**: The API Gateway maintains a registry of all available MCP servers, enabling dynamic tool discovery
- **Auth**: MCP requests carry the same JWT + tenant context as standard API calls. Tools are RBAC-governed — the Support Agent cannot call `assign_driver`.
- **Audit**: All MCP tool invocations are logged as audit events with: agent_id, tool_name, input, output, timestamp, tenant_id

## Enterprise Extension (Tenant MCP)

Enterprise tenants can register external MCP servers. The AI Gateway routes tool calls to tenant-registered servers after validating schema compatibility and applying rate limits. This enables merchants to build their own AI workflows on top of LogisticOS data without needing API keys to each individual service.

## Consequences

- **Decoupled AI from operations:** Adding a new agent or swapping the AI model doesn't require changing service APIs
- **Standardized tool contracts:** All agents use the same MCP tool definitions — no bespoke glue code per agent
- **Auditable AI actions:** MCP invocation log provides a complete trail of what the AI did and why
- **Adds MCP server surface area:** Each service must maintain MCP tool definitions alongside REST/gRPC APIs — mitigated by code generation from a shared schema
- **Enterprise moat:** Tenant-extensible MCP creates a platform effect — logistics companies build their own AI on top of our data
