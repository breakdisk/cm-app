# LogisticOS — Implementation Plan

**Architecture:** Rust microservices monorepo | Next.js / React Native frontends | Kafka + PostgreSQL + ClickHouse data layer | Kubernetes + Istio infra

---

## Phase 0 — Foundation ✅ COMPLETE
**Objective:** Every subsequent phase builds on this. Nothing ships until this is solid.

### Services
- [x] Monorepo scaffold (`Cargo.toml` workspace, `libs/`, `services/`, `apps/`, `infra/`)
- [x] `services/identity` — Multi-tenant JWT auth, RBAC, API keys, SSO/OIDC
- [x] `services/api-gateway` — Auth forwarding, rate limiting, routing, MCP registry, OpenAPI aggregation
- [x] `libs/common`, `libs/errors`, `libs/types`, `libs/auth`, `libs/events`, `libs/tracing`, `libs/geo`, `libs/ai-client`

### Infrastructure
- [x] Docker Compose local stack (all 17 services + PostgreSQL, Redis, Kafka, ClickHouse, Jaeger, Grafana)
- [x] PostgreSQL schema initialization with RLS policies (`scripts/db/init.sql`)
- [x] Kafka topic provisioning script (`scripts/kafka/create-topics.sh`)
- [x] GitHub Actions CI: `cargo clippy`, `cargo test`, `cargo build --release` (ci-rust.yml, ci-frontend.yml)
- [x] GitHub Actions CD: staging + production canary deploy (deploy-staging.yml, deploy-production.yml)
- [x] Base Helm chart template with all 17 service overrides (`infra/kubernetes/helm/`)
- [x] Istio mTLS baseline, VirtualServices, DestinationRules, Gateway (`infra/istio/`)
- [x] Terraform modules: vpc, eks, rds, elasticache, msk, iam, s3, monitoring
- [x] Terraform environments: dev, staging, production
- [x] Kubernetes namespaces, RBAC, network policies, External Secrets

### Deliverables
- Developer can `docker compose up` and have the full local stack running in < 5 minutes
- Identity service handles 1,000 concurrent auth requests at p99 < 50ms
- All `libs/` crates compile with zero clippy warnings

### Success Criteria
- `cargo build --workspace` passes clean
- Tenant creation, user invite, JWT issuance, RBAC enforcement all working and tested
- API Gateway proxies requests with auth validation

---

## Phase 1 — Logistics MVP ✅ COMPLETE
**Objective:** End-to-end shipment lifecycle: book → dispatch → deliver → POD.

### Services to Build
- [x] `services/order-intake` — Order validation, address normalization, waybill generation, CSV bulk upload
- [x] `services/dispatch` — Driver assignment, nearest-neighbor routing, VRP optimization, route management
- [x] `services/driver-ops` — Task queue, location tracking, status updates, COD collection
- [x] `services/pod` — Signature capture, photo upload (S3 presigned), GPS verification, OTP, dispute handling
- [x] `services/fleet` — Vehicle registration, GPS telematics (TimescaleDB), maintenance scheduling, fuel tracking

### Frontends
- [x] `apps/merchant-portal` — Shipment list, bulk actions, analytics, billing, campaigns, fleet, settings (dark glassmorphism)
- [x] `apps/driver-app` — Route view, task list, POD capture, barcode scanner, offline-first SQLite sync

### Business Logic Implemented
- Shipment status state machine (Pending → Confirmed → PickupAssigned → PickedUp → InTransit → Delivered/Failed)
- COD amount ≤ declared value validation
- Driver cannot be assigned to a route if vehicle is at capacity
- Shipment can only be cancelled before pickup
- POD required before marking delivery as completed
- Failed delivery auto-queues reschedule attempt (up to 3 times)

### Kafka Events Flowing
- `shipment.created` → dispatch subscribes
- `driver.assigned` → driver-ops + engagement subscribe
- `delivery.completed` → payments + cdp + analytics subscribe
- `delivery.failed` → business-logic + engagement subscribe

### Infrastructure
- [ ] Per-service Dockerfiles and Helm charts deployed to staging K8s
- [ ] `services/identity` → `services/order-intake` gRPC contract finalized
- [ ] S3/MinIO bucket for POD photos

### Success Criteria
- Full balikbayan box flow works end-to-end in staging
- Driver app works offline (queue actions, sync on reconnect)
- Delivery success rate trackable in Grafana

---

## Phase 2 — Engagement + Customer Experience ✅ COMPLETE
**Objective:** Customer communication automated. Merchant has visibility. Engagement engine live.

### Services to Build
- [x] `services/cdp` — Unified customer profile, behavioral events, CLV/churn scoring, segments, consent
- [x] `services/engagement` — WhatsApp (Twilio), SMS, email (SendGrid), push; template management, Kafka consumer
- [x] `services/delivery-experience` — Public tracking API, branded tracking page, ETA, reschedule, feedback, preferences
- [x] `services/hub-ops` — Parcel induction, sort scanning, dock scheduling, hub manifest, capacity management
- [x] `services/carrier` — Carrier onboarding, SLA tracking, AI allocation log, rate cards

### Frontends
- [x] `apps/customer-portal` — Tracking page (public), reschedule, feedback (glassmorphism, dark theme)
- [x] `apps/admin-portal` — Dispatch console, live driver map, hubs, carriers, drivers, fleet, AI agents, alerts

### Business Logic Implemented
- Engagement trigger rules: `delivery.completed` → send confirmation WhatsApp
- Engagement trigger rules: `delivery.failed` → send reschedule notification
- ETA calculation based on driver location + remaining stops (recomputed every 2 min)
- Carrier SLA breach detection (missed pickup/delivery windows)
- Customer consent checked before any marketing message
- Hub capacity enforcement (reject transfers above threshold)

### MCP Layer (Phase 2 addition per ADR-0004)
- [ ] `services/engagement` exposes MCP server: `send_notification`, `get_customer_preferences`
- [ ] `services/cdp` exposes MCP server: `get_customer_profile`, `get_shipment_history`
- [ ] MCP Registry initialized in `services/api-gateway`

### Success Criteria
- WhatsApp delivery confirmation sent within 5s of `delivery.completed` event
- Live tracking page loads branded with merchant logo, shows real-time driver location
- Customer can reschedule a failed delivery via tracking link (no login required)

---

## Phase 3 — Payments + Marketing + Growth ✅ COMPLETE
**Objective:** Revenue engine live. Merchant gets invoiced. Growth loops running.

### Services to Build
- [x] `services/payments` — Invoicing, COD reconciliation, wallet, Stripe/PayMongo integration
- [x] `services/marketing` — Campaign management, A/B testing, send log, segmentation
- [x] `services/analytics` — ClickHouse event ingestion, delivery KPIs, daily aggregates, driver performance
- [x] `services/business-logic` — ECA rules engine, Kafka consumer, HTTP action executor (notify/reschedule/alert)

### Frontends
- [x] `apps/merchant-portal` — Billing dashboard (invoices, COD, wallet), campaigns, analytics (charts, zone breakdown)
- [x] `apps/partner-portal` — SLA dashboard, payout view, rate cards, manifests, settings (dark glassmorphism)

### Business Logic Implemented
- Net-15 invoice generation triggered by weekly shipment settlement
- 12% VAT applied on all Philippines-based invoices
- COD reconciliation: driver cash collected vs. system expected, flagging discrepancies > 5%
- Dynamic pricing: surcharge for same-day orders placed after 3PM
- Failed delivery rules engine: IF attempts >= 3 THEN return to sender AND notify merchant
- Referral reward: PHP 50 wallet credit per referred merchant's first 10 shipments
- Campaign eligibility: only customers with active consent in CDP

### MCP Layer (Phase 3 additions)
- [ ] `services/dispatch` MCP server: `assign_driver`, `optimize_route`, `get_route_status`
- [ ] `services/order-intake` MCP server: `reschedule_delivery`, `cancel_shipment`
- [ ] `services/payments` MCP server: `get_cod_balance`, `generate_invoice`
- [ ] `services/analytics` MCP server: `get_delivery_metrics`, `get_zone_demand_forecast`

### Success Criteria
- Monthly merchant invoice auto-generated and emailed (zero manual intervention)
- COD reconciliation variance < 0.5% of total collected
- Campaign builder can send a WhatsApp campaign to a segment of 10,000 customers in < 60s
- At least 3 automation rules running in production (reschedule, notify, escalate)

---

## Phase 4 — AI Intelligence Layer + Advanced Optimization ✅ COMPLETE
**Objective:** The platform runs itself. AI agents handle dispatch, support, and marketing automatically.

### Services to Build
- [x] `services/ai-layer` — Full agentic runtime: ClaudeClient (claude-opus-4-6), AgentRunner (20-turn loop), ToolRegistry (9 MCP tools), Kafka trigger consumer, HTTP session API

### AI Agents Implemented
- [x] **AI Dispatch Agent** — auto-assigns driver via assign_driver + optimize_route MCP tools
- [x] **AI Recovery Agent** — handles delivery.failed events, reschedules, notifies customer
- [x] **AI Reconciliation Agent** — processes cod.collected, updates wallet via MCP
- [x] **AI Support Agent** — WhatsApp inquiry handling with escalate_to_human fallback
- [x] **AI Marketing Agent** — campaign trigger via get_customer_profile + send_notification
- [x] **AI Operations Copilot** — metrics analysis via get_delivery_metrics MCP tool

### MCP Layer (Complete)
- [x] All 9 MCP tools implemented in ai-layer: get_available_drivers, assign_driver, get_shipment, reschedule_delivery, send_notification, get_delivery_metrics, reconcile_cod, get_driver_performance, escalate_to_human
- [x] API Gateway MCP registry with enterprise tenant MCP server registration
- [x] Agent session audit log (PostgreSQL), escalated sessions endpoint
- [x] Kafka trigger consumer: shipment.created → Dispatch, delivery.failed → Recovery, cod.collected → Reconciliation

### Success Criteria
- AI Dispatch Agent handles ≥ 80% of assignments without human intervention
- Support Agent resolves ≥ 60% of WhatsApp inquiries without escalation
- Delivery delay prediction model achieves ≥ 75% precision at 2-hour horizon
- Operations Copilot surfaces at least 1 actionable recommendation per day with > 70% acceptance rate by dispatchers

---

## Cross-Cutting Requirements (All Phases)

### Performance SLAs
| Endpoint | p99 Target |
|----------|-----------|
| Auth / Token validation | < 30ms |
| Shipment booking | < 300ms |
| Driver assignment | < 500ms |
| Live tracking update | < 2s end-to-end |
| WhatsApp notification delivery | < 5s from trigger |
| Analytics dashboard load | < 3s |

### Security (Non-negotiable every phase)
- All inter-service traffic: mTLS via Istio
- All secrets: HashiCorp Vault (no .env files in production)
- All public APIs: rate limited (per tenant + per API key)
- All mutations: audit logged (actor, tenant, IP, timestamp, before/after state)
- PCI-DSS scope: payment card data never leaves `services/payments`
- GDPR/PDPA: consent gate enforced in `services/engagement` before any message

### Code Quality (CI gate — every PR)
- `cargo clippy -- -D warnings` passes
- `cargo test --workspace` passes
- Service integration tests run against real PostgreSQL (no mocks)
- `cargo audit` — no known vulnerabilities in dependencies
- OpenAPI spec diff — breaking changes require major version bump

---

## Team Resourcing by Phase

| Phase | Duration | Critical Path Roles |
|-------|----------|---------------------|
| 0 — Foundation | 6-8 weeks | Staff Rust Engineer, SRE, DB Engineer |
| 1 — Logistics MVP | 10-12 weeks | 2x Senior Rust (dispatch + driver), RN Engineer (driver app), Senior Frontend (merchant portal) |
| 2 — Engagement | 8-10 weeks | Senior Rust (engagement + cdp), Frontend (customer portal), UX Designer |
| 3 — Payments + Growth | 10-12 weeks | Senior Rust (payments), ML Engineer (models), PM (marketing) |
| 4 — AI Layer | 12-16 weeks | Staff ML Engineer, AI Agent Engineer, MLOps Engineer |

**Total estimated timeline to Phase 4 GA: ~12-14 months**
(Phases 1–3 run partially parallel with adequate team size)
