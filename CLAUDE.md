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

1. **Feature branches** from `main` — naming: `feat/service-name/description`
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
