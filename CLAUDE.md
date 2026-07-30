# LogisticOS — AI Agentic Last Mile Delivery SaaS Platform

## Project Overview

LogisticOS is an AI Agentic SAAS, mobile-first, multi-tenant SaaS platform for logistics and last-mile delivery operations. It combines logistics operations management, customer engagement automation, marketing intelligence, and AI-driven decision making into a single unified growth platform.

**Strategic Differentiator:** Most logistics software manages operations. LogisticOS also controls customer communication, marketing automation, and revenue generation — creating a logistics growth platform, not just an operations tool.

**Audit the layout for responsiveness. Check any elements that might break on small viewports.
**enable the screenshot workflow  iterate until the design is polished across different simulated screen sizes.


---

## Technology Stack

### Primary Language
- **Rust** — all backend microservices, systems-level code, performance-critical paths

### Backend Framework & Runtime
- **Axum** — HTTP web framework for microservices
- **Tokio** — async runtime
- **Tonic** — gRPC for inter-service communication
- **SQLx** — async PostgreSQL/database access (compile-time checked queries)

### Data Infrastructure
- **PostgreSQL** — primary relational data store (per-service, schema-per-tenant)
- **Redis** — caching, session management, pub/sub, rate limiting
- **Apache Kafka** — event streaming, inter-service messaging
- **ClickHouse** — analytics warehouse, OLAP queries
- **TimescaleDB** — time-series data (GPS, telemetry, metrics)
- **PostGIS** — geospatial queries, routing, location clustering

### AI / ML Layer
- **Python** — ML model training, AI agent orchestration (sidecar services)
- **ONNX Runtime** — model serving within Rust services
- **LangChain / LangGraph** — AI agent workflows
- **Anthropic Claude API** — conversational AI, copy generation, support agents
- **OpenAI / Embeddings** — semantic search, customer intent detection
- **Model Context Protocol (MCP)** — standardized AI-to-service interface; all agents consume operational data and invoke actions exclusively via MCP tools (see ADR-0004)

### Frontend

#### Web Portals (Next.js 14+ App Router, TypeScript)
- **Merchant Portal** — shipment booking, bulk upload, billing, campaign builder
- **Admin / Ops Portal** — dispatch console, live driver map, hub operations
- **Partner Portal** — carrier performance, SLA dashboard, payout view
- **Customer Portal** — branded tracking page, reschedule, delivery feedback

#### Mobile Apps (React Native + Expo, TypeScript)
- **Driver Super App** — route navigation, task list, offline POD capture, barcode scanner
- **Customer App** — shipment tracking, booking, loyalty, push notifications

#### UI Framework — Futuristic Stack

All portals use a **dark-first glassmorphism design system** with the following libraries:

| Library | Purpose |
|---------|---------|
| **Aceternity UI** | Futuristic pre-built components: spotlight cards, moving borders, particle backgrounds, aurora effects, text reveal, beam effects |
| **shadcn/ui** | Headless accessible base components (dialog, select, tabs, toast) styled to match dark theme |
| **Framer Motion** | Micro-interactions, page transitions, staggered list animations, gesture-driven UI |
| **@react-three/fiber + @react-three/drei** | 3D live driver map globe, animated route visualization, 3D analytics dashboard elements |
| **GSAP + ScrollTrigger** | Marketing/onboarding page scroll animations, timeline sequences |
| **Lottie React** | Complex animated icons (delivery truck, package scan, checkmark, loading states) |
| **TailwindCSS** | Utility styling with custom futuristic theme tokens (neon palette, glassmorphism utilities, glow shadows) |
| **Recharts** | Delivery KPI charts — styled dark with neon fills |

#### Design Language
- **Dark-first:** Near-black base (`#050810`), not just a dark mode toggle — dark is the primary canvas
- **Glassmorphism panels:** `backdrop-blur` + translucent borders + subtle inner glow — no solid opaque cards
- **Neon accent palette:** Electric cyan (`#00E5FF`), Plasma purple (`#A855F7`), Signal green (`#00FF88`), Warning amber (`#FFAB00`)
- **Grid/mesh backgrounds:** Animated CSS grid or dot-matrix overlays on key pages
- **Typography:** `Geist` (body) + `Space Grotesk` (headings) + `JetBrains Mono` (tracking numbers, codes, data)
- **Motion:** Everything that changes state animates — no instant jumps. Easing: `cubic-bezier(0.16, 1, 0.3, 1)` (spring-out)
- **Glow effects:** Active states and alerts use `box-shadow` neon glow, not borders
- **Maps:** Mapbox Dark (`mapbox://styles/mapbox/dark-v11`) with custom neon driver markers and animated route lines

#### Design System Location
`apps/merchant-portal/src/lib/design-system/` — shared across all portals via symlink or monorepo package `@logisticos/ui`

#### Mobile (React Native + Expo)
- **NativeWind** — Tailwind for React Native
- **React Native Reanimated 3** — 60fps animations on the JS thread without bridge overhead
- **React Native Gesture Handler** — swipe-to-confirm delivery, drag-to-reorder stops
- **Expo MapView** — dark-themed maps for driver navigation

### Infrastructure
- **Kubernetes (K8s)** — container orchestration
- **Docker** — containerization
- **Istio** — service mesh, mTLS, traffic management
- **Envoy** — API gateway proxy
- **Terraform** — infrastructure as code
- **GitHub Actions** — CI/CD pipelines
- **Prometheus + Grafana** — metrics and observability
- **OpenTelemetry** — distributed tracing
- **Loki** — log aggregation

### Security
- **OAuth 2.0 / OpenID Connect** — SSO, identity federation
- **JWT + Refresh Tokens** — session management
- **Vault (HashiCorp)** — secrets management
- **RBAC** — role-based access control at API and data layer
- **Row-Level Security (RLS)** — PostgreSQL tenant isolation

---

## Roles & Stakeholders

### Executive & Business Leadership

| Role | Responsibilities |
|------|-----------------|
| **Chief Executive Officer (CEO)** | Vision, fundraising, strategic partnerships, market positioning |
| **Chief Technology Officer (CTO)** | Technical vision, architecture governance, engineering team leadership |
| **Chief Product Officer (CPO)** | Product roadmap, feature prioritization, user research oversight |
| **Chief Operations Officer (COO)** | Logistics domain expertise, operations workflows, SLA standards |
| **Chief Revenue Officer (CRO)** | Sales strategy, enterprise client acquisition, pricing models |
| **Chief Marketing Officer (CMO)** | Brand, growth marketing, engagement engine strategy |
| **Chief Financial Officer (CFO)** | Billing architecture oversight, financial compliance, investor relations |
| **Chief Information Security Officer (CISO)** | Security policy, compliance (GDPR, PCI-DSS), incident response |

---

### Product Management

| Role | Responsibilities |
|------|-----------------|
| **Principal Product Manager — Platform** | Core platform vision, roadmap coordination across all services |
| **Product Manager — Logistics Operations** | Order, dispatch, routing, driver ops, fleet, hub features |
| **Product Manager — Customer Experience** | CDP, tracking experience, delivery portal, customer-facing features |
| **Product Manager — Engagement & Marketing** | Unified Engagement Engine, campaign management, marketing automation |
| **Product Manager — AI Features** | AI agents, predictive models, automation workflows |
| **Product Manager — Payments & Billing** | COD, invoicing, wallet, payment integrations |
| **Product Manager — Partner & Carrier** | Carrier onboarding, SLA, partner portal |
| **Product Analyst** | Data-driven feature analysis, funnel metrics, A/B test design |
| **UX Researcher** | User interviews, usability testing, journey mapping |

---

### Engineering Leadership

| Role | Responsibilities |
|------|-----------------|
| **Principal Software Architect** | System design, cross-service contracts, ADRs, tech debt governance |
| **Engineering Manager — Platform Core** | Identity, tenancy, API gateway, data infrastructure teams |
| **Engineering Manager — Logistics Domain** | Order, dispatch, routing, driver, fleet, hub service teams |
| **Engineering Manager — Engagement** | Engagement engine, CDP, marketing automation teams |
| **Engineering Manager — AI/ML** | AI intelligence layer, model ops, agent development |
| **Engineering Manager — Mobile** | Driver app, customer app, offline-first architecture |
| **Engineering Manager — Frontend** | Merchant portal, admin dashboard, partner portal |
| **Engineering Manager — Platform Engineering** | CI/CD, Kubernetes, observability, developer experience |

---

### Backend Engineering (Rust)

| Role | Responsibilities |
|------|-----------------|
| **Staff Engineer — Rust Platform** | Core Rust libraries, shared crates, performance standards |
| **Senior Rust Engineer — Identity & Auth** | Identity service, OAuth/OIDC, RBAC, multi-tenancy |
| **Senior Rust Engineer — Order & Dispatch** | Order intake, dispatch engine, VRP algorithms |
| **Senior Rust Engineer — Routing Service** | Route planning, geospatial logic, traffic integration |
| **Senior Rust Engineer — Driver Operations** | Driver app backend, task management, POD service |
| **Senior Rust Engineer — Fleet & Telematics** | Vehicle tracking, telemetry ingestion, maintenance scheduling |
| **Senior Rust Engineer — Payments** | Billing engine, COD reconciliation, payment gateway integrations |
| **Senior Rust Engineer — Engagement Engine** | Channel integrations (WhatsApp, SMS, Email, Push), campaign execution |
| **Senior Rust Engineer — CDP** | Customer profile unification, behavioral tracking, consent management |
| **Senior Rust Engineer — Carrier Management** | Carrier onboarding, SLA enforcement, auto-allocation |
| **Senior Rust Engineer — Analytics** | ClickHouse ingestion, reporting APIs, BI data layer |
| **Backend Engineer (x6)** | Feature development across services under senior guidance |

---

### AI / ML Engineering

| Role | Responsibilities |
|------|-----------------|
| **Staff ML Engineer / AI Architect** | AI layer architecture, model selection, agent orchestration design |
| **Senior ML Engineer — Dispatch AI** | Smart dispatch agent, VRP optimization, delay prediction |
| **Senior ML Engineer — Customer Intelligence** | CLV prediction, churn detection, delivery pattern modeling |
| **Senior ML Engineer — Marketing AI** | Campaign optimization, send-time prediction, intent detection |
| **Senior ML Engineer — Fraud & Risk** | Payment fraud, shipment fraud, delivery authenticity scoring |
| **MLOps Engineer** | Model serving, ONNX pipeline, A/B testing of models, drift monitoring |
| **AI Agent Engineer (Python)** | LangGraph/LangChain agent workflows, Claude API integration |
| **Data Scientist** | Exploratory analysis, feature engineering, model evaluation |
| **Data Engineer** | Kafka pipelines, ETL into ClickHouse, data warehouse schema |

---

### Frontend Engineering

| Role | Responsibilities |
|------|-----------------|
| **Staff Frontend Engineer** | Architecture, component standards, design system governance |
| **Senior Frontend Engineer — Merchant Portal** | Merchant dashboard (Next.js), shipment management UI |
| **Senior Frontend Engineer — Admin & Ops Portal** | Operations dashboard, dispatch console, fleet views |
| **Senior Frontend Engineer — Partner Portal** | Carrier and partner management UI |
| **Senior Frontend Engineer — Customer Portal** | Tracking pages, branded delivery experience, customer portal |
| **Frontend Engineer (x4)** | Feature development across portals |
| **Senior React Native Engineer — Driver App** | Driver super app, offline-first, barcode scanning, POD |
| **Senior React Native Engineer — Customer App** | Customer mobile app, live tracking, booking, loyalty |
| **React Native Engineer (x2)** | Feature development across mobile apps |

---

### UX / Design

| Role | Responsibilities |
|------|-----------------|
| **Head of Design / Principal UX Designer** | Design system governance, UX standards, cross-platform consistency |
| **Senior UX Designer — Logistics Ops** | Dispatch console, fleet views, hub operations UI flows |
| **Senior UX Designer — Customer Experience** | Customer portal, tracking experience, delivery feedback |
| **Senior UX Designer — Driver App** | Driver app UX, task flows, offline-first patterns |
| **Senior UX Designer — Merchant & Partner** | Merchant portal, campaign builder, partner onboarding flows |
| **UI Designer** | Visual design, iconography, component design |
| **Motion Designer** | Micro-interactions, loading states, onboarding animations |
| **Accessibility Specialist** | WCAG compliance, screen reader support, keyboard navigation |

---

### Quality Assurance

| Role | Responsibilities |
|------|-----------------|
| **QA Lead** | Test strategy, coverage standards, release gates |
| **Senior QA Engineer — Backend** | API testing, integration testing, contract testing |
| **Senior QA Engineer — Mobile** | Driver app and customer app testing, device matrix |
| **Senior QA Engineer — Frontend** | Portal testing, cross-browser, responsive |
| **Performance Engineer** | Load testing, stress testing, latency benchmarking |
| **Security QA Engineer** | Penetration testing, OWASP compliance, vulnerability scanning |
| **QA Automation Engineer (x2)** | E2E test automation (Playwright, Appium) |

---

### Platform & Infrastructure Engineering

| Role | Responsibilities |
|------|-----------------|
| **Staff Platform Engineer / SRE Lead** | SLA ownership, incident management, reliability standards |
| **Senior SRE / DevOps Engineer** | Kubernetes cluster management, Istio, CI/CD pipelines |
| **Infrastructure Engineer (Terraform)** | Cloud infra as code, multi-region provisioning |
| **Senior Security Engineer** | Secrets management (Vault), network security, compliance automation |
| **Database Reliability Engineer** | PostgreSQL performance, replication, backup strategy, RLS |
| **Observability Engineer** | Prometheus, Grafana, Loki, OpenTelemetry tracing setup |

---

### Business Domain Experts (Internal Consultants)

| Role | Responsibilities |
|------|-----------------|
| **Logistics Domain Expert** | Last-mile delivery workflows, routing standards, hub operations |
| **E-commerce Integration Expert** | Shopify/WooCommerce/Lazada/Shopee integration patterns |
| **Compliance & Legal Counsel** | Data privacy (GDPR, PDPA), PCI-DSS, logistics regulations |
| **Finance & Billing SME** | COD workflows, invoicing rules, payment reconciliation |
| **Customer Success Lead** | Onboarding flows, merchant success, churn reduction |

---

### External Stakeholders & Integration Partners

| Stakeholder | Relationship |
|-------------|-------------|
| **Enterprise Merchants** | Primary B2B clients; drive shipment volume and revenue |
| **SME Merchants** | Self-service clients; highest volume, lower ACV |
| **End Customers (Recipients)** | Delivery experience consumers; drive NPS and repeat merchant usage |
| **Drivers / Couriers** | Field operators; data producers for tracking and AI training |
| **Third-Party Carriers** | Outsourced delivery partners; SLA contracts, API integrations |
| **Payment Gateways** (Stripe, PayMongo, etc.) | Financial transaction processing |
| **Telecom Providers** (Twilio, Globe, PLDT) | SMS, WhatsApp, voice channel delivery |
| **Map Providers** (Google, Mapbox, HERE) | Routing, geocoding, live traffic |
| **E-commerce Platforms** (Shopify, WooCommerce, Lazada) | Order intake integrations |
| **ERP / WMS Vendors** | Warehouse and inventory system integrations |
| **GPS / Telematics Vendors** | Fleet tracking hardware integrations |
| **Cloud Providers** (AWS / GCP / Azure) | Infrastructure hosting |
| **Investors / Board** | Funding, governance, growth KPIs |
| **Regulatory Authorities** | Data protection, transport, financial regulations |

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    CLIENT LAYER                             │
│  Customer App  │  Driver App  │  Merchant Portal  │  Admin  │
│  (React Native)│ (React Native)│    (Next.js)     │(Next.js)│
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│              API GATEWAY & INTEGRATION LAYER                │
│          Envoy / Axum Gateway — Auth, Rate Limit,           │
│          Routing, API Keys, Webhook Management              │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│           UNIFIED ENGAGEMENT ENGINE                         │
│  CDP  │  Campaign Mgmt  │  WhatsApp/SMS/Email/Push  │  Chat │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│               LOGISTICS OPERATIONS LAYER                    │
│  Order Intake │ Dispatch & Routing │ Driver Ops │ Fleet     │
│  Hub Ops      │ Carrier Mgmt       │ POD        │ Payments  │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│              BUSINESS LOGIC ENGINE                          │
│  Rules Engine │ Workflow Automation │ Dynamic Pricing       │
│  SLA Enforcement │ Routing Rules │ Trigger Conditions       │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│               AI INTELLIGENCE LAYER                         │
│  Dispatch Agent │ Logistics Planner │ Support Agent         │
│  Marketing Agent │ Operations Copilot │ Fraud Detection      │
└─────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────┐
│            DATA & EVENT INFRASTRUCTURE                      │
│  PostgreSQL │ Kafka │ Redis │ ClickHouse │ TimescaleDB      │
│  PostGIS    │ ONNX Model Serving │ Analytics Warehouse      │
└─────────────────────────────────────────────────────────────┘
```

---

## Microservices Inventory

| # | Service | Domain | Primary Tech |
|---|---------|--------|-------------|
| 1 | **Identity & Tenant Management** | Platform Core | Rust + PostgreSQL |
| 2 | **Customer Data Platform (CDP)** | Engagement | Rust + PostgreSQL + Redis |
| 3 | **Unified Engagement Engine** | Engagement | Rust + Kafka + Redis |
| 4 | **Order & Shipment Intake** | Logistics | Rust + PostgreSQL |
| 5 | **Dispatch & Routing** | Logistics | Rust + PostGIS + Redis |
| 6 | **Driver Operations** | Logistics | Rust + Redis + TimescaleDB |
| 7 | **Customer Delivery Experience** | Customer | Rust + Redis |
| 8 | **Fleet Management** | Logistics | Rust + TimescaleDB |
| 9 | **Warehouse & Hub Operations** | Logistics | Rust + PostgreSQL |
| 10 | **Carrier & Partner Management** | Partner | Rust + PostgreSQL |
| 11 | **Proof of Delivery** | Logistics | Rust + PostgreSQL |
| 12 | **Payments & Billing** | Finance | Rust + PostgreSQL |
| 13 | **Analytics & BI** | Intelligence | Rust + ClickHouse |
| 14 | **Marketing Automation Engine** | Engagement | Rust + Kafka |
| 15 | **Business Logic & Automation Engine** | Platform Core | Rust + Redis |
| 16 | **AI Intelligence Layer** | AI | Python + ONNX + Rust FFI |
| 17 | **API Gateway & Integration Layer** | Platform Core | Rust (Axum + Envoy) |
| 18 | **Data & Infrastructure Layer** | Infrastructure | Kafka + ClickHouse + PostGIS |

---

## MCP Integration Layer

Each operational service exposes an **MCP Server** alongside its HTTP/gRPC APIs. The AI Intelligence Layer and Enterprise tenants consume all operational data and invoke actions exclusively through MCP — no direct AI-to-service calls.

| Service | Key MCP Tools |
|---------|--------------|
| Dispatch | `assign_driver`, `optimize_route`, `get_available_drivers` |
| Order Intake | `reschedule_delivery`, `cancel_shipment`, `get_shipment` |
| Driver Ops | `get_driver_location`, `send_driver_instruction` |
| Engagement | `send_notification`, `get_customer_preferences` |
| CDP | `get_customer_profile`, `get_churn_score` |
| Payments | `generate_invoice`, `get_cod_balance` |
| Analytics | `get_delivery_metrics`, `get_zone_demand_forecast` |
| Hub Ops | `get_hub_capacity`, `schedule_dock` |
| Fleet | `get_vehicle_status`, `get_fleet_availability` |

**Enterprise Extension:** Enterprise-tier tenants may register their own external MCP servers via the API Gateway. This creates a platform effect — merchants build their own AI workflows on LogisticOS data without direct service API access.

See [docs/adr/0004-mcp-for-ai-interoperability.md](docs/adr/0004-mcp-for-ai-interoperability.md) for the full decision record.

---

## Engineering Principles

### Code Quality
- All Rust code must pass `clippy` with `#![deny(clippy::all)]`
- Zero `unwrap()` in production paths — use proper error propagation with `thiserror`/`anyhow`
- All public APIs must have integration tests
- Service contracts defined as protobuf (gRPC) or OpenAPI 3.1 specs before implementation
- Every service exposes `/health`, `/metrics`, `/ready` endpoints

### Architecture Decisions
- **Service Isolation:** Each microservice owns its database schema. No cross-service DB joins.
- **Event-First:** State changes emit Kafka events. Downstream services react; no synchronous coupling for non-critical paths.
- **Multi-Tenancy:** Row-level security (RLS) enforced at PostgreSQL layer. Tenant ID propagated via JWT claims and request context.
- **Offline-First Mobile:** Driver app functions without connectivity. Sync on reconnection.
- **API Contracts First:** OpenAPI/protobuf spec reviewed before any implementation begins.
- **ADR Required:** All architectural decisions documented as Architecture Decision Records in `/docs/adr/`.

### Security Standards
- No secrets in code or environment files — all via Vault
- mTLS between all internal services via Istio
- Input validation at API boundary using Rust type system + validator crate
- PCI-DSS scope minimization — payment data never stored in non-payment services
- GDPR/PDPA compliance: consent required before behavioral tracking; right-to-erasure implemented
- All API keys and webhooks scoped to minimum permissions

### Performance Standards
- P99 API latency < 200ms for operational endpoints
- P99 dispatch assignment < 500ms
- Live tracking updates < 2s end-to-end
- Notification delivery (WhatsApp/SMS) < 5s from trigger event
- All DB queries analyzed with EXPLAIN; no unbounded full table scans in production paths

### AI Integration Standards
- AI features are additive enhancements — all operations must have a non-AI fallback
- Model predictions logged for retraining pipelines
- AI agent actions are audited and reversible where possible
- Bias monitoring on dispatch and routing models

---

## Domain Glossary

| Term | Definition |
|------|-----------|
| **Tenant** | A logistics company using LogisticOS |
| **Merchant** | A business that ships goods (client of the Tenant) |
| **Shipper** | Synonym for Merchant in some contexts |
| **Consignee / Customer** | The end recipient of a shipment |
| **Driver / Courier** | Field agent who performs pickups and deliveries |
| **Hub** | A sorting/distribution center in the logistics network |
| **POD** | Proof of Delivery (signature, photo, OTP) |
| **COD** | Cash on Delivery — payment collected at doorstep |
| **VRP** | Vehicle Routing Problem — algorithm for optimizing multi-stop routes |
| **ETA** | Estimated Time of Arrival |
| **SLA** | Service Level Agreement — delivery time commitments |
| **AWB** | Airway Bill / tracking number assigned to a shipment |
| **First Mile** | Pickup from merchant to hub |
| **Last Mile** | Delivery from hub to end customer |
| **Cross-dock** | Transferring parcels between vehicles/hubs without storage |
| **Balikbayan Box** | Large freight box used by overseas workers sending goods home (PH context) |
| **CDP** | Customer Data Platform — unified profile store |
| **Engagement Engine** | Unified system for all customer communications |

---

## Key Use Case: Balikbayan Box (Fully Automated Flow)

```
1. Customer sends WhatsApp message
        ↓
2. AI Support Agent captures shipment details
        ↓
3. Order Intake Service validates & normalizes address
        ↓
4. Dispatch Engine assigns pickup driver (AI-optimized)
        ↓
5. Driver App notifies courier with route
        ↓
6. Engagement Engine sends pickup confirmation (WhatsApp + SMS)
        ↓
7. Driver completes pickup → POD recorded
        ↓
8. Hub Operations receives & sorts parcel
        ↓
9. Carrier Management selects optimal outbound carrier (AI)
        ↓
10. Live tracking link sent to customer
        ↓
11. Delivery attempted → POD (photo + signature + GPS)
        ↓
12. Delivery confirmation sent (WhatsApp)
        ↓
13. Marketing Automation triggers next-shipment campaign (AI-generated)
        ↓
14. Analytics records full shipment lifecycle for BI
```

---

## Repository Structure (Target)

```
logisticos/
├── CLAUDE.md                          # This file
├── docs/
│   ├── adr/                           # Architecture Decision Records
│   ├── api/                           # OpenAPI + Protobuf specs
│   ├── runbooks/                      # Operational runbooks
│   └── architecture/                  # Architecture diagrams
├── services/
│   ├── identity/                      # Service 1: Identity & Tenant Mgmt
│   ├── cdp/                           # Service 2: Customer Data Platform
│   ├── engagement/                    # Service 3: Unified Engagement Engine
│   ├── order-intake/                  # Service 4: Order & Shipment Intake
│   ├── dispatch/                      # Service 5: Dispatch & Routing
│   ├── driver-ops/                    # Service 6: Driver Operations
│   ├── delivery-experience/           # Service 7: Customer Delivery Experience
│   ├── fleet/                         # Service 8: Fleet Management
│   ├── hub-ops/                       # Service 9: Warehouse & Hub Ops
│   ├── carrier/                       # Service 10: Carrier & Partner Mgmt
│   ├── pod/                           # Service 11: Proof of Delivery
│   ├── payments/                      # Service 12: Payments & Billing
│   ├── analytics/                     # Service 13: Analytics & BI
│   ├── marketing/                     # Service 14: Marketing Automation
│   ├── business-logic/                # Service 15: Business Logic Engine
│   ├── ai-layer/                      # Service 16: AI Intelligence Layer
│   └── api-gateway/                   # Service 17: API Gateway
├── libs/
│   ├── common/                        # Shared Rust crates (errors, types, auth)
│   ├── proto/                         # Shared protobuf definitions
│   └── sdk/                           # Client SDKs (generated)
├── apps/
│   ├── merchant-portal/               # Next.js merchant dashboard
│   ├── admin-portal/                  # Next.js admin/ops dashboard
│   ├── partner-portal/                # Next.js partner portal
│   ├── customer-portal/               # Next.js customer tracking portal
│   ├── driver-app/                    # React Native driver super app
│   └── customer-app/                  # React Native customer app
├── infra/
│   ├── terraform/                     # Infrastructure as code
│   ├── kubernetes/                    # K8s manifests and Helm charts
│   ├── istio/                         # Service mesh configuration
│   └── monitoring/                    # Grafana dashboards, alerts
└── scripts/                           # Developer tooling, migration scripts
```

---

## Development Workflow

1. **Feature branches** from `master` — naming: `feat/service-name/description`
2. **OpenAPI/Protobuf spec** reviewed and merged before implementation
3. **ADR created** for any architectural decision
4. **Implementation** with unit + integration tests required
5. **PR review** — minimum 2 approvals, one must be a senior engineer or architect
6. **CI gates:** clippy, tests, security scan, performance regression check
7. **Staging deploy** — E2E tests run against staging environment
8. **Production deploy** — canary rollout via Istio traffic splitting

---

## Non-Negotiables

- **Zero downtime deployments** — all services must support rolling updates
- **Data residency** — tenant data must remain in configured region
- **Audit logging** — all mutations logged with actor, timestamp, tenant, IP
- **Rate limiting** — all public-facing APIs rate-limited per tenant and per API key
- **Multi-language support** — UI must support i18n from day one (EN, PH priority)
- **Mobile-first** — all customer and driver interfaces designed mobile-first
- **Accessibility** — WCAG 2.1 AA minimum for all web portals

---

## Session Continuity Notes (PR #126 — branch `claude/quirky-wright-3apidc`)

Work completed in this session — all committed and pushed:

| Change | File |
|--------|------|
| Magic link `returnTo` propagation end-to-end | `apps/landing/src/app/login/page.tsx` |
| `/verify-email` page (was 404) | `apps/landing/src/app/verify-email/page.tsx` |
| `/reset-password` page (was 404) | `apps/landing/src/app/reset-password/page.tsx` |
| Session route: extract `USER_NOT_INVITED` / `ROLE_MISMATCH` reason codes | `apps/landing/src/app/api/auth/session/route.ts` |
| Login page: actionable 403 error messages for admin/partner | `apps/landing/src/app/login/page.tsx` |
| Carrier detail: **Partner Portal Access** card with Invite Portal User form | `apps/admin-portal/src/app/(dashboard)/carriers/[id]/page.tsx` |
| Carrier detail: **Review & Approve** flow for pending_verification carriers | `apps/admin-portal/src/app/(dashboard)/carriers/[id]/page.tsx` |
| Fix: `inviteUser` result extraction bug (was using `ApiResponse` envelope, now correctly uses `.data`) | same |
| Carrier detail: **full UI readability redesign** — KPI strip (2xl values, accent glow), Carrier Profile (GlassCard padding=none, icon header, 2-col border-grid, text-sm values), SLA (divide-y rows, font-bold color-coded values), Compliance (inline colored pill badge replacing invisible NeonBadge) | `apps/admin-portal/src/app/(dashboard)/carriers/[id]/page.tsx` |

### Auth / Identity Notes
- `exchangeFirebaseToken` → `link_invited_user` → `find_by_email_global` uses **case-sensitive exact SQL match**. Identity service stores emails as-entered; Firebase returns lowercase. Always normalize emails to lowercase when creating users via `inviteUser` (already done in the invite handlers).
- Partner portal carrier lookup: `GET /v1/carriers/me` matches authenticated user's email against `carrier.contact_email`. Carrier must be created with the same lowercase email the partner uses to sign in.
- Dev seed users: `admin@demo.com`, `merchant@demo.com`, `driver@demo.com` (password: `LogisticOS1!`). Real tenant: `demo` / `00000000-0000-0000-0000-000000000001`.

### How to Invite a Partner User for an Existing Carrier
Admin Portal → Carriers → click carrier row → scroll to **Partner Portal Access** card → **Invite Portal User** button.

---

## Session Continuity Notes — Remote MCP Server (`ai-layer` `/mcp`)

**Status: implemented, `cargo check` clean, committed (`95725f60` + follow-up client-IP commit), pushed to `origin/master`, deploy to VPS `75.119.138.135` (Dokploy, compose app `oscargomarketnet-logisticosbackend-pqfh0u`) in progress via GHCR rebuild. Not yet tested end-to-end with a live MCP client.**

Built out the "Expose a LogisticOS service as a remote MCP server" direction of ADR-0004's Enterprise Extension — wraps the AI Layer's existing `ToolRegistry` (`services/ai-layer/src/infrastructure/tools/mod.rs`, 21 tools) behind a real MCP-protocol transport, rather than the old bespoke `/internal/tools/execute` contract the Python sidecar uses (that internal route is untouched — both surfaces now share the one tool registry).

| Change | File |
|---|---|
| Added `rmcp = "3.0.1"` (features `server`, `transport-streamable-http-server`) to workspace deps | [Cargo.toml](Cargo.toml) |
| Wired `rmcp.workspace = true` | [services/ai-layer/Cargo.toml](services/ai-layer/Cargo.toml) |
| New `LogisticOsMcpServer` implementing `rmcp::ServerHandler` (`list_tools`/`call_tool`) over the existing `ToolRegistry` | [services/ai-layer/src/api/mcp/mod.rs](services/ai-layer/src/api/mcp/mod.rs) (new file) |
| `pub mod mcp;` added | [services/ai-layer/src/api/mod.rs](services/ai-layer/src/api/mod.rs) |
| `.nest_service("/mcp", mcp::streamable_http_service(tools.clone()))` mounted **before** the `require_auth` middleware layer, so JWT auth covers it for free | [services/ai-layer/src/bootstrap.rs](services/ai-layer/src/bootstrap.rs) |
| `/mcp` path added to `resolve_upstream()` → `ai_layer_url` | [services/api-gateway/src/proxy/mod.rs](services/api-gateway/src/proxy/mod.rs) |

**Key design decisions:**
- **Access gate:** `Claims::has_feature("enterprise_mcp")`. This pricing-feature-matrix key already existed (`identity` migration `0016_pricing_feature_matrix.sql` + `libs/auth/src/claims.rs`) but had **zero consumers anywhere in the codebase** before this — this is its first real use.
- **Tenant scoping:** `tenant_id` is always overwritten server-side from the validated JWT inside `call_tool`, never trusted from the MCP client's arguments — prevents a remote caller from passing an arbitrary `tenant_id` to read/act on another tenant's data.
- **Auth plumbing:** no new auth code written. `RequestContext::extensions` (rmcp) carries the raw `axum::http::request::Parts` for every call; the pre-existing `require_auth` middleware already inserts `Claims` into request extensions before the request reaches the nested MCP service, so the handler just reads them back out.
- **Transport:** stateless Streamable HTTP (no session pinning) — matches the platform's Istio rolling-deploy requirement.

**Audit trail — wired (was a gap, now closed):** `LogisticOsMcpServer::record_audit` (in `mcp/mod.rs`) persists every remote MCP tool call as an `AgentAction` on a single-action `AgentSession` (`AgentType::OnDemand`, `trigger: {"source": "remote_mcp", "user_id", "email"}`), saved via the same `SessionRepository` the internal LangGraph agent path uses. This means remote MCP calls now show up in the existing AI Agents dashboard / session history (`GET /v1/agents/sessions`) alongside autonomous agent runs — no new table, no new admin UI needed. `LogisticOsMcpServer::new()` and `mcp::streamable_http_service()` both now take `session_repo: Arc<dyn SessionRepository>` in addition to `tools`; `bootstrap.rs` passes `session_repo.clone()` before it's moved into `AppState`.
Captured: actor (`user_id`/`email`), tenant (`session.tenant_id`), timestamp (`executed_at`/`started_at`/`completed_at`).

**Client IP — wired (was a gap, now closed):** api-gateway's `proxy_handler` is now served via `into_make_service_with_connect_info::<SocketAddr>()` ([bootstrap.rs](services/api-gateway/src/bootstrap.rs)), appends the connecting peer to `X-Forwarded-For` (preserving any upstream value rather than overwriting), and explicitly skips forwarding the original `x-forwarded-for` header as-is in the generic header-copy loop so there's exactly one, correctly-appended header. `mcp/mod.rs::client_ip_from` reads it back out of `RequestContext::extensions` and stores it in `AgentSession.trigger.client_ip`. Caveat: this is the gateway's own peer address — if a CDN/LB sits in front of the gateway in a given environment, that hop's address is what lands here unless it already stamped its own XFF (in which case we append, not overwrite).

**Deploy in progress:** pushed to `origin/master` (had to merge a divergent remote first — PR #124 "wire pending state for Gig Workers" had landed upstream; clean merge, no conflicts). `Cargo.lock`/`Cargo.toml` changed → `build-images.yml` rebuilds **all** 20 services + 5 portals, not just ai-layer/api-gateway. Confirmed SSH access to the VPS and that Dokploy's compose app pulls `ghcr.io/breakdisk/logisticos-service-<name>:latest` — **once the GHCR build finishes, still need to run `docker compose pull && docker compose up -d` (at least for `api-gateway` and `ai-layer`) inside `/etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/` on the VPS** to actually pick up the new images; pushing to GHCR alone does not redeploy the running containers.

**Known gaps — do not treat as production-ready without addressing:**
- **No per-tool RBAC.** ADR-0004's "Support Agent can't call `assign_driver`" is not enforced — every tool is reachable to any caller that clears the `enterprise_mcp` gate. This is the significant one: any caller who clears the Enterprise-tier gate can invoke financial/destructive tools (`assign_driver`, `cancel_shipment`, `generate_invoice`, `reconcile_cod`) with no further authorization check.
- **No OAuth 2.1 discovery metadata.** A caller needs an existing LogisticOS Bearer JWT already; Claude Desktop's automatic remote-MCP OAuth connector won't self-provision a session. A merchant's own agent framework configured with a static token works today.

**Environment note:** C: drive was at **~1.6 GB free** after this build (down from the usual state) — clear `C:\cargo-target-logisticos\debug\incremental` before the next full build/link session.

**Next steps:** decide on per-tool RBAC model, decide whether OAuth 2.1 discovery is worth the lift vs. the static-token bridge, optionally wire XFF + `ConnectInfo` for IP capture, then commit + test against a running ai-layer instance with an `enterprise_mcp`-enabled tenant JWT.

---

## Development Environment & Gotchas

### Git
- **Default branch is `master`**, not `main` — use `origin/master` in all log/diff comparisons
- **Commit messages with `•` bullets break PowerShell heredocs** — use `git commit -F /tmp/msg.txt` instead of `-m` with special characters
- **`settings.local.json` is tracked** — scrub any secrets (tokens, API keys) from the tool-allowlist before staging; the file accumulates literal command strings from prior sessions

### Rust / Cargo (dev machine)
- **C: drive fills to 0 bytes during long build sessions** — clear `C:\cargo-target-logisticos\debug\incremental` (~10 GB, safe to delete; Cargo regenerates) to recover ~9 GB
- **Set `CARGO_INCREMENTAL=0`** on every `cargo build/check` to prevent incremental cache regrowth
- **`link.exe` exit code 1318 is a disk-full linker error**, not a code error — `cargo check` (skips linking) is sufficient for type verification during development
- **Multi-crate check:** `cargo check -p crate-a -p crate-b` validates several crates in one invocation

### Axum routing
- **Duplicate `.route()` calls on the same path panic at startup** — combine HTTP methods on one call: `.route("/v1/foo", get(list).post(create))`

### Engagement service — notification templates
- **`event_consumer.rs` uses inline Rust `match` arms, not the DB registry** — `engagement.notification_templates` is only read by the HTTP `/v1/send` path. Kafka-driven consumer notifications need a match arm in `event_consumer.rs`; a DB seed alone has no effect on consumer-triggered messages

### hub-ops — repository layers
- **Two separate container repo structs exist** — `PgContainerRepository` (in `bootstrap.rs`, used directly by HTTP handlers) and `PgPalletContainerRepository` (in `infrastructure/db/mod.rs`, implements `ContainerRepository` trait used by `HubTransferService`). Add new trait methods to `PgPalletContainerRepository`

### Android driver app
- **Cannot run Gradle locally** — Android changes follow the existing feature-module pattern and are validated by the GitHub Actions Android CI workflow; use `cargo check` / `tsc --noEmit` for backend/portal verification within sessions
- **`device_timestamp` discipline** — capture `System.currentTimeMillis()` at the physical event (scan callback, shutter click) and convert immediately with `HubRepository.isoFromMillis()`; never re-sample at coroutine launch or network-send time

---

## Proof of Pickup (POP) & Real-Time Telemetry — Implementation Directive

> **Alignment verdict:** The POP architecture is fully coherent with the existing three-track billing domain, `DriverLedger`, billing clearance gate, and `OutboundSyncWorker` patterns already in the codebase. The items below are the precise delta between the spec and current state.

### What Is Already Built (Do Not Rebuild)

| Spec Requirement | Existing Implementation |
|---|---|
| Three-track domain (A/B/C) | `services/payments/src/domain/strategies/` — `balikbayan.rs`, `standard_parcel.rs`, `cod.rs` |
| Billing clearance blocking container assignment | `InvoiceRepository::get_billing_clearance` + `BillingClearance` struct in `services/payments/src/domain/repositories/mod.rs` |
| Driver cash debit on pickup | `DriverLedger::record_cash_collected` + `DriverLedgerRepository` in `services/payments/src/domain/entities/driver_ledger/` |
| `workflow_metadata` JSONB on invoices | `InvoiceRepository::save_with_domain` — implemented in payments |
| Geofence hard block (200 m) at delivery | `POD_GEOFENCE_METERS = 200.0` in `services/pod/src/domain/value_objects/mod.rs` |
| Offline-resilient photo sync | `OutboundSyncWorker` in `apps/driver-app-android/core/database/worker/OutboundSyncWorker.kt` |
| Append-only audit entries | `DriverLedger.entries: Vec<LedgerEntry>` — immutable, never updated |
| TIMESTAMPTZ everywhere | `captured_at: TIMESTAMPTZ` on `pod.proofs`; `TIMESTAMPTZ` on all ledger and invoice tables |

---

### Modifications Required to Existing Code

#### 1. Track B — 5 % Weight Tolerance Band
**File:** `services/payments/src/domain/strategies/standard_parcel.rs` — line ~166

Current code triggers an overage invoice on ANY `actual > declared`. The spec requires a 5 % tolerance band before triggering `InvoiceState::HeldForPaymentOverage`.

```rust
// BEFORE
let has_overage = actual_g > declared_g && declared_g > 0;

// AFTER
let has_overage = declared_g > 0
    && (actual_g as f64 - declared_g as f64) / declared_g as f64 > 0.05;
```

#### 2. 50 m `OUT_OF_BOUNDS_HANDOVER` Geospatial Flag
**File:** `services/pod/src/application/services/pod_service.rs` — inside the `submit` handler, after geofence passes.

The existing 200 m block (hard gate at initiation) is **not replaced** — this is a separate soft annotation. When the distance between `(capture_lat, capture_lng)` and the registered delivery coordinates exceeds 50 m, write the flag into the invoice `workflow_metadata` via `InvoiceRepository::save_with_domain`:

```rust
// After successful POD submit — compute haversine distance against delivery coords
// If distance_m > 50.0 and geofence_verified == true (GPS drift edge case):
//   invoice.workflow_metadata["telemetry_exception"] = "OUT_OF_BOUNDS_HANDOVER"
// Do NOT reject the request — write and continue.
```

#### 3. Pickup Payload — POP Metadata per Track
**File:** `apps/driver-app-android/feature/pickup/data/PickupRepository.kt` — `confirmPickup()`

Currently sends `CompleteTaskRequest()` (empty body). Must be extended to carry track-specific POP metadata:

- **Track A (Balikbayan):** `verified_box_size: String` (enum: JUMBO | XL | LARGE | MEDIUM), `outer_packaging_integrity: Boolean`, `cash_collected_amount: Long` (cents)
- **Track B (Standard Parcel / Hub Inbound):** `verified_weight_grams: Int`, `scale_node_id: String`
- **Track C (COD Last-Mile):** `barcode_scan_hash: String`, `gps_lat: Double`, `gps_lng: Double`, `driver_device_id: String`

All tracks must include `device_timestamp: String` (ISO 8601 captured via `System.currentTimeMillis()` converted to UTC at the physical barcode scan moment — not at network send time).

---

### New Implementations Required

#### 4. `ProofOfPickup` Domain Entity
Create `services/pod/src/domain/entities/pop.rs` (or a dedicated `services/pop/` service if scope justifies it).

`ProofOfPickup` is the custody-open bookend symmetric to `ProofOfDelivery`. Minimum schema:

```rust
pub struct ProofOfPickup {
    pub id:               Uuid,
    pub tenant_id:        Uuid,
    pub shipment_id:      Uuid,
    pub task_id:          Uuid,
    pub driver_id:        Uuid,
    pub billing_track:    BillingTrack,   // A | B | C
    pub status:           PopStatus,      // Draft | Completed | Disputed
    pub device_timestamp: DateTime<Utc>,  // hardware clock at scan
    pub server_timestamp: DateTime<Utc>,  // backend processing time
    pub capture_lat:      f64,
    pub capture_lng:      f64,
    // Track-A-specific
    pub verified_box_size:            Option<String>,
    pub outer_packaging_integrity:    Option<bool>,
    pub cash_collected_amount_cents:  Option<i64>,
    // Track-B-specific
    pub verified_weight_grams:        Option<i64>,
    pub scale_node_id:                Option<String>,
    // Track-C-specific
    pub barcode_scan_hash:            Option<String>,
    pub driver_device_id:             Option<String>,
    pub created_at:                   DateTime<Utc>,
}
```

Migration: `services/pod/migrations/0003_create_pop_table.sql`. Table name: `pod.proofs_of_pickup`.

**Hard guard:** `pop_status != 'completed'` must block `sea_container_id` assignment at the carrier service level (see item 7 below).

#### 5. `shipment_telemetry_logs` Table (TimescaleDB Hypertable)
Every shipment state mutation — triggered by API request OR edge sync event — MUST append a record. Never overwrite.

```sql
-- Migration: services/order-intake/migrations/XXXX_create_telemetry_logs.sql
CREATE TABLE IF NOT EXISTS shipments.telemetry_logs (
    id               UUID        NOT NULL DEFAULT gen_random_uuid(),
    shipment_id      UUID        NOT NULL,
    tenant_id        UUID        NOT NULL,
    event_type       TEXT        NOT NULL,  -- e.g. 'PICKUP_COMPLETE', 'HUB_INBOUND', 'POD_SUBMITTED'
    device_timestamp TIMESTAMPTZ,           -- from Kotlin hardware layer; NULL for server-side events
    server_timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    actor_id         UUID,                  -- driver_id, hub_agent_id, system
    payload          JSONB       NOT NULL DEFAULT '{}',
    PRIMARY KEY (id, server_timestamp)
);
SELECT create_hypertable('shipments.telemetry_logs', 'server_timestamp');
CREATE INDEX ON shipments.telemetry_logs (shipment_id, server_timestamp DESC);
```

Rust repository: `TelemetryLogRepository` with a single `append(event: TelemetryEvent) -> AppResult<()>` method. All services inject and call it at every milestone transition.

**Analytical rule:** SLA and transit-velocity queries MUST use `device_timestamp` as the primary time basis where non-null, falling back to `server_timestamp` only when device_timestamp is absent (server-generated events). Never use `server_timestamp` alone for SLA calculations.

#### 6. POP → Driver Ledger Debit
Wire the POP completion handler to call `DriverLedger::record_cash_collected` when `cash_collected_amount_cents > 0`.

- **Track A:** On `MilestoneEvent::PickupComplete` (Balikbayan doorstep) — debit `cash_collected_amount_cents` to driver ledger as `LedgerEntryType::CodCollected`.
- **Track C:** On `MilestoneEvent::PickupComplete` (first-mile merchant handover) — if COD shipment, draft invoice is created; ledger debit is deferred to `MilestoneEvent::CodDeliveryCompleted` (existing `CodStrategy` handles this correctly).
- The `DriverLedger` repository and `find_or_create_for_shift` are already implemented. Wire the POP service to them identically to `CodStrategy` in `services/payments/src/domain/strategies/cod.rs:140–153`.

#### 7. `sea_container_id` Hard Guard at Carrier Service
Enforce the billing clearance check before any database mutation that assigns a `shipment_id` to a `sea_container_id` or customs manifest. Implementation pattern:

```rust
// In the carrier/container assignment handler:
let clearance = invoice_repo
    .get_billing_clearance(&tenant_id, shipment_id).await?;

if let Some(c) = clearance {
    if !c.is_cleared {
        return Err(AppError::BusinessRule(format!(
            "Shipment {} has {} unpaid invoice(s) — cannot assign to container until billing is cleared.",
            shipment_id, c.unpaid_invoice_count
        )));
    }
}
// Also check POP status once the POP service is live:
// let pop = pop_repo.find_completed_by_shipment(shipment_id).await?;
// if pop.is_none() { return Err(AppError::BusinessRule("POP not completed".into())); }
```

#### 8. `OutboundSyncWorker` — Image Compression + GPS Bundle
**File:** `apps/driver-app-android/core/database/worker/OutboundSyncWorker.kt`

Before uploading a photo to R2:
1. Read the `File` from disk.
2. Decode as `Bitmap` and recompress: `Bitmap.compress(Bitmap.CompressFormat.JPEG, 75, outputStream)` — target ≤ 800 KB per image.
3. Add GPS coordinates (`gps_lat`, `gps_lng`) and `device_timestamp` (ISO 8601, captured at camera shutter — stored in the `SyncQueueEntity.payloadJson`, NOT at worker-execution time) to the multipart metadata alongside the compressed bytes.
4. The camera/capture screen must NOT allow the driver to proceed to the next screen until the compressed payload is successfully enqueued in `SyncQueueEntity` — not until it is uploaded (offline scenarios must still allow flow-through).

---

### Cross-Cutting Implementation Rules

These rules apply to every engineer implementing any POP, telemetry, or pickup-related feature:

1. **Dual timestamp contract:** Every API payload that originates from a physical device action (barcode scan, photo capture, signature) MUST carry both `device_timestamp` (ISO 8601, hardware clock at action moment) and allow the server to record `server_timestamp` (backend receipt time). Store both in `shipment_telemetry_logs`. SLA calculations use `device_timestamp`.

2. **`shipment_telemetry_logs` is append-only:** No `UPDATE` or `DELETE` ever touches this table. Every state transition is a new row. This is enforced at the application layer; a `REVOKE UPDATE, DELETE ON shipments.telemetry_logs FROM app_role;` grant should be applied in the migration.

3. **POP gates POD:** A `ProofOfPickup` with `status = Completed` is a prerequisite for downstream manifest eligibility. The billing clearance check (`get_billing_clearance`) must be extended to also validate POP status when the shipment is in the Balikbayan or Standard Parcel billing track.

4. **`workflow_metadata` JSONB is the audit scratch-pad:** Any telemetry exception flag (`OUT_OF_BOUNDS_HANDOVER`, weight discrepancy, packaging integrity failure) is written into the invoice or POP record's `workflow_metadata` field. It is never a blocking error by itself — it is an ops visibility tag.

5. **Kotlin `device_timestamp` discipline:** `System.currentTimeMillis()` must be called **at the instant of the physical event** (scan, shutter click, confirmation tap) and serialized immediately into the `SyncQueueEntity.payloadJson`. It must never be read later at worker-execution time. Use `Instant.ofEpochMilli(deviceTs).atOffset(ZoneOffset.UTC).format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)` for ISO 8601 serialization.

---

### Glossary Additions

| Term | Definition |
|---|---|
| **POP** | Proof of Pickup — cryptographic custody-open bookend. Opens the platform liability loop at first physical asset contact. Symmetric to POD. |
| **Chain of Custody** | The unbroken sequence POP → telemetry milestones → POD. Every leg is bound by these two bookends. |
| **`device_timestamp`** | Hardware clock reading taken on the Kotlin driver app at the physical moment of the scan/capture event. Primary time basis for SLA calculations. |
| **`server_timestamp`** | Backend cluster time recorded when the event payload is processed. Used as fallback when `device_timestamp` is absent. |
| **`OUT_OF_BOUNDS_HANDOVER`** | Telemetry exception flag written to `workflow_metadata` when delivery GPS is > 50 m from the registered address. Non-blocking — ops audit use only. |
| **`shipment_telemetry_logs`** | Append-only TimescaleDB hypertable recording every shipment state transition as a sequential timeline block. |
