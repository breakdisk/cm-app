# LogisticOS — System Architecture Overview

**Version:** 1.0
**Last Updated:** 2026-03-17
**Maintained by:** Principal Software Architect

---

## Full System Diagram

```
╔══════════════════════════════════════════════════════════════════════════════════════╗
║                                  CLIENT LAYER                                        ║
╠══════════════════════════════╦═══════════════════╦══════════════╦════════════════════╣
║  Customer App                ║  Driver Super App  ║  Merchant    ║  Admin / Ops       ║
║  (React Native + Expo)       ║  (React Native)    ║  Portal      ║  Portal            ║
║  • Live tracking             ║  • Route nav       ║  (Next.js)   ║  (Next.js)         ║
║  • Booking                   ║  • Task list       ║  • Booking   ║  • Dispatch        ║
║  • Loyalty                   ║  • POD capture     ║  • Bulk ops  ║  • Live map        ║
║  • Push notifications        ║  • Offline-first   ║  • Billing   ║  • Fleet view      ║
║                              ║    (ADR-0007)      ║  • Campaigns ║  • Hub ops         ║
╠══════════════════════════════╩═══════════════════╩══════════════╬════════════════════╣
║  Partner Portal (Next.js)                                        ║  Customer Portal   ║
║  • Carrier SLA dashboard                                         ║  (Next.js)         ║
║  • Payout & settlement view                                      ║  • Branded tracking║
║  • Performance analytics                                         ║  • Reschedule      ║
║                                                                  ║  • Feedback        ║
╚══════════════════════════════════════════════════════════════════╩════════════════════╝
                                           │
                              HTTPS / WebSocket / gRPC-Web
                                           │
╔══════════════════════════════════════════╧═════════════════════════════════════════════╗
║                        API GATEWAY & INTEGRATION LAYER                                 ║
║                                                                                        ║
║   ┌────────────────────────────────────────────────────────────────────────────────┐   ║
║   │                    Envoy Proxy (L7 routing, mTLS termination)                  │   ║
║   └──────────────────────────────────┬─────────────────────────────────────────────┘   ║
║   ┌──────────────────────────────────┴─────────────────────────────────────────────┐   ║
║   │               Axum API Gateway Service (Service 17)                            │   ║
║   │  • JWT validation + tenant extraction      • Rate limiting (per tenant, per key│   ║
║   │  • API key management                      • Webhook ingestion & fanout        │   ║
║   │  • Route multiplexing (REST → gRPC)        • Tenant MCP server registry        │   ║
║   │  • OpenAPI spec enforcement                • Request/response audit log        │   ║
║   └─────────────────────────────────────────────────────────────────────────────────┘   ║
║                                                                                        ║
║   External Integrations: Shopify │ WooCommerce │ Lazada │ Shopee │ ERP/WMS webhooks   ║
╚════════════════════════════════════════════════════════════════════════════════════════╝
                                           │
                        (Istio service mesh — mTLS between all services)
                                           │
╔══════════════════════════════════════════╧═════════════════════════════════════════════╗
║                           PLATFORM CORE SERVICES                                       ║
║                                                                                        ║
║  ┌──────────────────────────────────┐   ┌──────────────────────────────────────────┐  ║
║  │  Identity & Tenant Management    │   │  Business Logic & Automation Engine       │  ║
║  │  (Service 1)                     │   │  (Service 15)                             │  ║
║  │  Rust + PostgreSQL               │   │  Rust + Redis                             │  ║
║  │  • OAuth 2.0 / OIDC              │   │  • Rules engine (delivery SLAs, triggers) │  ║
║  │  • JWT issuance + refresh        │   │  • Workflow automation                    │  ║
║  │  • RBAC policy enforcement       │   │  • Dynamic pricing rules                  │  ║
║  │  • Multi-tenant provisioning     │   │  • Routing rules evaluation               │  ║
║  │  • API key lifecycle             │   │  • Condition/action trigger evaluation    │  ║
║  └──────────────────────────────────┘   └──────────────────────────────────────────┘  ║
╚════════════════════════════════════════════════════════════════════════════════════════╝
                                           │
                ┌──────────────────────────┼──────────────────────────┐
                │                          │                          │
╔═══════════════╧═══════════╗  ╔═══════════╧════════════╗  ╔══════════╧════════════════╗
║   LOGISTICS DOMAIN         ║  ║   ENGAGEMENT DOMAIN     ║  ║   PARTNER / FINANCE       ║
╠════════════════════════════╣  ╠═════════════════════════╣  ╠═══════════════════════════╣
║                            ║  ║                         ║  ║                           ║
║  Order & Shipment Intake   ║  ║  Customer Data Platform  ║  ║  Carrier & Partner Mgmt   ║
║  (Service 4)               ║  ║  (Service 2)            ║  ║  (Service 10)             ║
║  Rust + PostgreSQL         ║  ║  Rust + PG + Redis      ║  ║  Rust + PostgreSQL        ║
║  • AWB generation          ║  ║  • Unified customer      ║  ║  • Carrier onboarding     ║
║  • Address validation      ║  ║    profile               ║  ║  • SLA enforcement        ║
║  • Merchant booking API    ║  ║  • Churn scoring         ║  ║  • Auto-allocation rules  ║
║  • Bulk CSV import         ║  ║  • Consent management    ║  ║  • Performance analytics  ║
║                            ║  ║  • Behavioral tracking   ║  ║                           ║
║  Dispatch & Routing        ║  ║                         ║  ║  Payments & Billing       ║
║  (Service 5)               ║  ║  Unified Engagement      ║  ║  (Service 12)             ║
║  Rust + PostGIS + Redis    ║  ║  Engine (Service 3)      ║  ║  Rust + PostgreSQL        ║
║  • VRP optimization        ║  ║  Rust + Kafka + Redis   ║  ║  • COD reconciliation     ║
║  • Driver assignment       ║  ║  • WhatsApp (Twilio)     ║  ║  • Invoice generation     ║
║  • Zone management         ║  ║  • SMS (Globe/PLDT)      ║  ║  • Wallet management      ║
║  • Route reoptimization    ║  ║  • Email (SES)           ║  ║  • Payment gateway hooks  ║
║                            ║  ║  • Push (Expo/FCM/APNs)  ║  ║  • COD float tracking     ║
║  Driver Operations         ║  ║  • Chat (AI-powered)     ║  ║                           ║
║  (Service 6)               ║  ║                         ║  ║  Analytics & BI            ║
║  Rust + Redis + Timescale  ║  ║  Marketing Automation    ║  ║  (Service 13)             ║
║  • Task state machine      ║  ║  Engine (Service 14)     ║  ║  Rust + ClickHouse        ║
║  • Location ingestion      ║  ║  Rust + Kafka            ║  ║  • Delivery KPI reports   ║
║  • COD collection          ║  ║  • Campaign scheduling   ║  ║  • Zone demand forecast   ║
║  • Driver availability     ║  ║  • Send-time prediction  ║  ║  • Driver performance     ║
║                            ║  ║  • A/B test execution    ║  ║  • Revenue analytics      ║
║  Customer Delivery Exp.    ║  ║  • Next-shipment upsell  ║  ║                           ║
║  (Service 7)               ║  ║                         ║  ╚═══════════════════════════╝
║  Rust + Redis              ║  ╚═════════════════════════╝
║  • Live ETA calculation    ║
║  • Tracking page API       ║
║  • Reschedule API          ║
║  • Delivery feedback       ║
║                            ║
║  Fleet Management          ║
║  (Service 8)               ║
║  Rust + TimescaleDB        ║
║  • Vehicle registry        ║
║  • GPS telemetry           ║
║  • Maintenance scheduling  ║
║  • Fuel tracking           ║
║                            ║
║  Warehouse & Hub Ops       ║
║  (Service 9)               ║
║  Rust + PostgreSQL         ║
║  • Induction scanning      ║
║  • Sort plan management    ║
║  • Cross-dock routing      ║
║  • Dock scheduling         ║
║                            ║
║  Proof of Delivery         ║
║  (Service 11)              ║
║  Rust + PostgreSQL         ║
║  • Photo storage (S3)      ║
║  • Signature storage       ║
║  • OTP validation          ║
║  • POD audit trail         ║
╚════════════════════════════╝
                    │
╔═══════════════════╧════════════════════════════════════════════════════════════════════╗
║                              AI INTELLIGENCE LAYER (Service 16)                        ║
║                                                                                        ║
║   Python + ONNX Runtime + LangGraph + Claude API                                      ║
║                                                                                        ║
║  ┌─────────────────┐  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────┐ ║
║  │  Dispatch Agent  │  │  Support Agent  │  │  Marketing Agent │  │  Ops Copilot     │ ║
║  │  • Smart driver  │  │  • WhatsApp AI  │  │  • Campaign gen  │  │  • Anomaly alert │ ║
║  │    assignment    │  │  • Order intake │  │  • Copy writing  │  │  • SLA risk pred │ ║
║  │  • Delay predict │  │  • Escalation   │  │  • Audience seg  │  │  • Recom. engine │ ║
║  └────────┬─────────┘  └────────┬────────┘  └────────┬─────────┘  └────────┬─────────┘ ║
║           └────────────────────┬┘                   ┌┘───────────────────┘           ║
║                                │                   │                                  ║
║                   ┌────────────▼───────────────────▼────────────┐                    ║
║                   │          MCP Tool Router                     │                    ║
║                   │  Resolves tool calls to service MCP servers  │                    ║
║                   │  RBAC-governed per agent role                │                    ║
║                   │  All invocations audit-logged                │                    ║
║                   └─────────────────────────────────────────────┘                    ║
╚════════════════════════════════════════════════════════════════════════════════════════╝
                                           │
                         MCP Tool Calls (to each service's MCP server)
                                           │
╔══════════════════════════════════════════╧═════════════════════════════════════════════╗
║                           MCP TOOL LAYER (per ADR-0004)                                ║
║                                                                                        ║
║  Each service exposes an MCP Server alongside its REST/gRPC API.                      ║
║  AI agents and Enterprise tenants interact with services exclusively via MCP.          ║
║                                                                                        ║
║  dispatch-mcp      order-mcp       driver-mcp       engagement-mcp   cdp-mcp          ║
║  assign_driver     get_shipment    get_location     send_notification get_profile      ║
║  optimize_route    reschedule      send_instr       create_campaign   get_churn_score  ║
║  get_drivers       cancel_shipment get_workload     get_preferences                    ║
║                                                                                        ║
║  payments-mcp      analytics-mcp   hub-mcp          fleet-mcp        pod-mcp          ║
║  get_cod_balance   get_metrics     get_capacity     get_vehicle_status get_pod         ║
║  generate_invoice  get_forecast    schedule_dock    get_availability   verify_pod      ║
╚════════════════════════════════════════════════════════════════════════════════════════╝
                                           │
╔══════════════════════════════════════════╧═════════════════════════════════════════════╗
║                        DATA & INFRASTRUCTURE LAYER                                     ║
║                                                                                        ║
║  ┌──────────────────┐  ┌──────────────┐  ┌────────────────┐  ┌──────────────────────┐ ║
║  │  PostgreSQL       │  │  Redis       │  │  Apache Kafka  │  │  ClickHouse          │ ║
║  │  (Primary store)  │  │  (Cache /    │  │  (Event bus /  │  │  (Analytics OLAP)    │ ║
║  │  RLS per ADR-0008│  │   Sessions / │  │   ADR-0006)    │  │  30-day event store  │ ║
║  │  Per-service      │  │   Pub/Sub /  │  │  22 topics     │  │  Zone forecasting    │ ║
║  │  schemas          │  │   Rate limit)│  │  DLQ pattern   │  │  BI query engine     │ ║
║  └──────────────────┘  └──────────────┘  └────────────────┘  └──────────────────────┘ ║
║                                                                                        ║
║  ┌──────────────────┐  ┌──────────────┐  ┌────────────────┐  ┌──────────────────────┐ ║
║  │  TimescaleDB      │  │  PostGIS     │  │  S3 / Object   │  │  ONNX Model Store    │ ║
║  │  (Time-series)    │  │  (Geospatial)│  │  Storage       │  │  (ML model serving)  │ ║
║  │  GPS telemetry    │  │  Zone polys  │  │  POD photos    │  │  VRP solver          │ ║
║  │  Metrics history  │  │  Route geo   │  │  Sig. captures │  │  ETA prediction      │ ║
║  │  Driver heartbeat │  │  Clustering  │  │  CSV exports   │  │  Churn models        │ ║
║  └──────────────────┘  └──────────────┘  └────────────────┘  └──────────────────────┘ ║
╚════════════════════════════════════════════════════════════════════════════════════════╝
```

---

## Service Mesh (Istio)

All inter-service communication within the cluster passes through the Istio service mesh:

```
Service A  →  [Envoy Sidecar A]  ===(mTLS)=== [Envoy Sidecar B]  →  Service B
                      ↑                               ↑
               Telemetry (OpenTelemetry)       Policy enforcement
               Trace propagation               Circuit breaking
               Metrics export                  Traffic splitting
               (→ Prometheus/Grafana)          (canary deploys)
```

- **mTLS**: All service-to-service traffic is mutually authenticated. No plain HTTP between services in production.
- **Circuit breaking**: Istio enforces circuit breaker policies on all service-to-service connections (max pending requests, retry budgets).
- **Traffic splitting**: Canary deployments use Istio `VirtualService` weighted routing (95/5 → 80/20 → 100/0).
- **Observability**: Distributed traces are propagated via `traceparent` headers. Envoy emits spans to the OpenTelemetry collector. Traces are stored in Tempo and visualized in Grafana.

---

## Data Flow Example: Shipment Created

```
Merchant Portal (POST /shipments)
         │
         ▼
[API Gateway — validates JWT, extracts tenant_id]
         │
         ▼
[Order Intake Service — validates address, assigns AWB]
         │
         ├─── PostgreSQL: INSERT INTO shipments (RLS tenant-scoped)
         │
         └─── Kafka PUBLISH: logisticos.order.shipment.created
                    │
          ┌─────────┼────────────────────────────────────┐
          │         │                                    │
          ▼         ▼                                    ▼
  [Dispatch       [Engagement                       [Analytics
   Service]        Service]                          Service]
   • Creates        • Sends booking confirmation      • Records
     route plan       WhatsApp to merchant              event in
   • Assigns         (logisticos.notification          ClickHouse
     driver           .outbound)
   • PUBLISH:       • PUBLISH:
     route.created   notification.sent
```

---

## Tenant Isolation Summary

All data access enforces tenant boundaries at three layers:

```
Request
   │
   ├─ [Layer 1] JWT middleware — validates tenant_id claim
   │
   ├─ [Layer 2] Application layer — tenant_id in every command/query
   │
   └─ [Layer 3] PostgreSQL RLS — SET LOCAL app.current_tenant_id before every transaction
                                  (ADR-0008 — enforced even if application bugs exist)
```

---

## Technology Quick Reference

| Concern | Technology |
|---------|-----------|
| API framework | Axum (Rust) |
| Async runtime | Tokio |
| Inter-service RPC | Tonic (gRPC) |
| DB access | SQLx (compile-time checked) |
| Primary store | PostgreSQL 16 + RLS |
| Cache / sessions | Redis 7 |
| Event streaming | Apache Kafka 3.7 |
| Analytics OLAP | ClickHouse 24 |
| Time-series | TimescaleDB 2.x |
| Geospatial | PostGIS 3.x |
| ML serving | ONNX Runtime (in-process Rust) |
| AI agents | Python + LangGraph + Claude API |
| Agent interface | MCP (Model Context Protocol) |
| Service mesh | Istio 1.22 |
| Proxy | Envoy 1.30 |
| Container orchestration | Kubernetes 1.30 |
| Infrastructure as code | Terraform |
| CI/CD | GitHub Actions |
| Secrets | HashiCorp Vault |
| Observability | Prometheus + Grafana + Loki + Tempo |
| Tracing | OpenTelemetry |
| Web portals | Next.js 14+ (App Router, TypeScript) |
| Mobile apps | React Native + Expo (TypeScript) |
| Maps | Mapbox Dark (web) + Expo MapView (mobile) |

---

## Related Documents

- [ADR-0001](../../adr/0001-rust-for-all-backend-services.md) — Rust for all backend services
- [ADR-0002](../../adr/0002-event-driven-inter-service-communication.md) — Event-driven communication
- [ADR-0003](../../adr/0003-row-level-security-for-multi-tenancy.md) — Row-level security
- [ADR-0004](../../adr/0004-mcp-for-ai-interoperability.md) — MCP for AI interoperability
- [ADR-0005](../../adr/0005-hexagonal-architecture-for-microservices.md) — Hexagonal architecture
- [ADR-0006](../../adr/0006-kafka-event-streaming-topology.md) — Kafka event streaming topology
- [ADR-0007](../../adr/0007-offline-first-driver-app.md) — Offline-first driver app
- [ADR-0008](../../adr/0008-multi-tenancy-rls-strategy.md) — Multi-tenancy RLS strategy
- [data-flow.md](data-flow.md) — Detailed data flow diagrams for key scenarios
