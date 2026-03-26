# LogisticOS

**AI Agentic SaaS Platform for Last-Mile Delivery Operations**

[![Build Status](https://img.shields.io/github/actions/workflow/status/logisticos/logisticos/ci.yml?branch=main&label=CI)](https://github.com/logisticos/logisticos/actions)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![Node.js](https://img.shields.io/badge/Node.js-20%2B-green?logo=node.js)](https://nodejs.org/)
[![License](https://img.shields.io/badge/license-UNLICENSED-red)](LICENSE)
[![ADRs](https://img.shields.io/badge/ADRs-8-blue)](docs/adr/)

LogisticOS is a mobile-first, multi-tenant SaaS platform that unifies logistics operations management, customer engagement automation, marketing intelligence, and AI-driven decision making into a single growth platform.

**Strategic differentiator:** Most logistics software manages operations. LogisticOS also controls customer communication, marketing automation, and revenue generation — making it a logistics *growth* platform, not just an operations tool.

---

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Microservices](#microservices)
- [MCP Integration Layer](#mcp-integration-layer)
- [Tech Stack](#tech-stack)
- [Repository Structure](#repository-structure)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Clone and Setup](#clone-and-setup)
  - [Run the Local Stack](#run-the-local-stack)
  - [Run Individual Services](#run-individual-services)
  - [Run Frontend Apps](#run-frontend-apps)
- [Service Port Reference](#service-port-reference)
- [Development Workflow](#development-workflow)
- [Engineering Principles](#engineering-principles)
- [Documentation](#documentation)
- [Contributing](#contributing)
- [License](#license)

---

## Architecture Overview

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

## Microservices

LogisticOS is composed of 17 independently deployable Rust microservices, each owning its own database schema.

| # | Service | Domain | Port | Primary Data Store |
|---|---------|--------|------|--------------------|
| 1 | **API Gateway** | Platform Core | 8000 | Redis |
| 2 | **Identity & Tenant Management** | Platform Core | 8001 | PostgreSQL |
| 3 | **Customer Data Platform (CDP)** | Engagement | 8002 | PostgreSQL + Redis |
| 4 | **Unified Engagement Engine** | Engagement | 8003 | Kafka + Redis |
| 5 | **Order & Shipment Intake** | Logistics | 8004 | PostgreSQL |
| 6 | **Dispatch & Routing** | Logistics | 8005 | PostGIS + Redis |
| 7 | **Driver Operations** | Logistics | 8006 | Redis + TimescaleDB |
| 8 | **Customer Delivery Experience** | Customer | 8007 | Redis |
| 9 | **Fleet Management** | Logistics | 8008 | TimescaleDB |
| 10 | **Warehouse & Hub Operations** | Logistics | 8009 | PostgreSQL |
| 11 | **Carrier & Partner Management** | Partner | 8010 | PostgreSQL |
| 12 | **Proof of Delivery** | Logistics | 8011 | PostgreSQL + MinIO |
| 13 | **Payments & Billing** | Finance | 8012 | PostgreSQL |
| 14 | **Analytics & BI** | Intelligence | 8013 | ClickHouse |
| 15 | **Marketing Automation Engine** | Engagement | 8014 | Kafka |
| 16 | **Business Logic & Automation Engine** | Platform Core | 8015 | Redis |
| 17 | **AI Intelligence Layer** | AI | 8016 | Python + ONNX + Rust FFI |

Every service exposes `/health`, `/metrics`, and `/ready` endpoints, plus an `/mcp` endpoint for AI agent interoperability.

---

## MCP Integration Layer

AI agents are first-class operators in LogisticOS. Every service exposes an **MCP (Model Context Protocol) server** alongside its HTTP/gRPC API. The AI Intelligence Layer and enterprise tenants consume all operational data and invoke actions exclusively through MCP — no direct AI-to-service database calls.

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

Enterprise-tier tenants may also register their own external MCP servers via the API Gateway, enabling custom AI workflows built on LogisticOS data. See [ADR-0004](docs/adr/0004-mcp-for-ai-interoperability.md).

---

## Tech Stack

### Backend

| Technology | Purpose |
|-----------|---------|
| **Rust 1.80+** | All microservices — safety, performance, zero-cost abstractions |
| **Axum 0.7** | HTTP web framework |
| **Tokio** | Async runtime |
| **Tonic / gRPC** | Inter-service communication |
| **SQLx 0.7** | Async PostgreSQL access with compile-time query checks |
| **rdkafka** | Kafka producer/consumer |
| **jsonwebtoken / argon2** | Auth and password hashing |

### Data Infrastructure

| Technology | Role |
|-----------|------|
| **PostgreSQL 16 + PostGIS** | Primary relational store, geospatial queries, Row-Level Security for multi-tenancy |
| **Redis 7** | Caching, session management, pub/sub, rate limiting |
| **Apache Kafka 3.7** | Event streaming, inter-service messaging, audit log |
| **ClickHouse 24.3** | Analytics warehouse, OLAP queries |
| **TimescaleDB** | Time-series data: GPS telemetry, driver metrics |
| **MinIO** | S3-compatible object storage (POD photos, documents) |

### AI / ML Layer

| Technology | Purpose |
|-----------|---------|
| **Anthropic Claude API** | Conversational AI, copy generation, support agents |
| **LangChain / LangGraph** | AI agent workflows and planning |
| **ONNX Runtime** | Model serving within Rust services |
| **Model Context Protocol (MCP)** | Standardized AI-to-service interface |
| **Python** | ML model training, AI agent orchestration (sidecar services) |
| **OpenAI Embeddings** | Semantic search, customer intent detection |

### Frontend — Web Portals

All portals use Next.js 14+ App Router with TypeScript and a **dark-first glassmorphism design system**.

| Technology | Purpose |
|-----------|---------|
| **Next.js 14+ (App Router)** | SSR/SSG web portals |
| **TypeScript** | Type-safe frontend code |
| **TailwindCSS** | Utility styling with custom neon/glassmorphism theme tokens |
| **Aceternity UI** | Futuristic pre-built components: spotlight cards, aurora effects, beam animations |
| **shadcn/ui** | Headless accessible base components (dialogs, selects, tabs, toasts) |
| **Framer Motion** | Micro-interactions, page transitions, staggered animations |
| **@react-three/fiber + drei** | 3D driver map globe, route visualization, 3D analytics elements |
| **GSAP + ScrollTrigger** | Marketing and onboarding page scroll animations |
| **Recharts** | Delivery KPI charts styled with neon fills |
| **Lottie React** | Complex animated icons (delivery truck, package scan, checkmark) |

### Frontend — Mobile Apps

| Technology | Purpose |
|-----------|---------|
| **React Native + Expo** | Cross-platform driver and customer apps |
| **NativeWind** | Tailwind CSS for React Native |
| **React Native Reanimated 3** | 60fps animations without bridge overhead |
| **React Native Gesture Handler** | Swipe-to-confirm delivery, drag-to-reorder stops |
| **Expo MapView** | Dark-themed maps for driver navigation |

### Infrastructure

| Technology | Purpose |
|-----------|---------|
| **Kubernetes** | Container orchestration |
| **Docker + Docker Compose** | Containerization and local dev stack |
| **Istio** | Service mesh, mTLS, canary traffic splitting |
| **Envoy** | API gateway proxy |
| **Terraform** | Infrastructure as code |
| **GitHub Actions** | CI/CD pipelines |
| **Prometheus + Grafana** | Metrics and dashboards |
| **OpenTelemetry + Jaeger** | Distributed tracing |
| **Loki** | Log aggregation |
| **HashiCorp Vault** | Secrets management |

---

## Repository Structure

```
logisticos/
├── CLAUDE.md                          # Project instructions and architecture guide
├── Cargo.toml                         # Rust workspace root
├── docker-compose.yml                 # Full local development stack
│
├── services/                          # Rust microservices
│   ├── api-gateway/                   # Service 1:  API Gateway & Integration Layer
│   ├── identity/                      # Service 2:  Identity & Tenant Management
│   ├── cdp/                           # Service 3:  Customer Data Platform
│   ├── engagement/                    # Service 4:  Unified Engagement Engine
│   ├── order-intake/                  # Service 5:  Order & Shipment Intake
│   ├── dispatch/                      # Service 6:  Dispatch & Routing
│   ├── driver-ops/                    # Service 7:  Driver Operations
│   ├── delivery-experience/           # Service 8:  Customer Delivery Experience
│   ├── fleet/                         # Service 9:  Fleet Management
│   ├── hub-ops/                       # Service 10: Warehouse & Hub Operations
│   ├── carrier/                       # Service 11: Carrier & Partner Management
│   ├── pod/                           # Service 12: Proof of Delivery
│   ├── payments/                      # Service 13: Payments & Billing
│   ├── analytics/                     # Service 14: Analytics & BI
│   ├── marketing/                     # Service 15: Marketing Automation Engine
│   ├── business-logic/                # Service 16: Business Logic & Automation Engine
│   └── ai-layer/                      # Service 17: AI Intelligence Layer
│
├── apps/                              # Frontend applications
│   ├── merchant-portal/               # Next.js — shipment booking, bulk upload, billing, campaigns
│   ├── admin-portal/                  # Next.js — dispatch console, live driver map, hub operations
│   ├── partner-portal/                # Next.js — carrier performance, SLA dashboard, payouts
│   ├── customer-portal/               # Next.js — branded tracking, reschedule, delivery feedback
│   ├── driver-app/                    # React Native — routes, task list, offline POD, barcode scan
│   └── customer-app/                  # React Native — shipment tracking, booking, loyalty, push
│
├── libs/                              # Shared Rust workspace crates
│   ├── common/                        # Shared utilities and types
│   ├── config/                        # Configuration loading
│   ├── errors/                        # Error types (thiserror-based)
│   ├── auth/                          # JWT validation, RBAC helpers
│   ├── tracing/                       # OpenTelemetry setup
│   ├── types/                         # Domain value types (TenantId, ShipmentId, etc.)
│   ├── events/                        # Kafka event schemas
│   ├── geo/                           # Geospatial utilities
│   ├── ai-client/                     # Anthropic Claude API client
│   ├── proto/                         # Shared protobuf definitions
│   └── sdk/                           # Generated client SDKs
│
├── infra/
│   ├── terraform/                     # Cloud infrastructure as code
│   ├── kubernetes/                    # K8s manifests and Helm charts
│   ├── istio/                         # Service mesh configuration
│   └── monitoring/                    # Grafana dashboards, Prometheus config, alerts
│
├── docs/
│   ├── adr/                           # Architecture Decision Records (8 ADRs)
│   ├── api/                           # OpenAPI + Protobuf specs
│   ├── runbooks/                      # Operational runbooks
│   └── architecture/                  # Architecture diagrams
│
└── scripts/
    ├── dev-setup.sh                   # First-time developer setup
    ├── migrate.sh                     # Run database migrations
    ├── seed-dev.sh                    # Seed local development data
    ├── ci/                            # CI helper scripts
    ├── db/                            # Database init scripts
    ├── dev/                           # Developer tooling
    └── kafka/                         # Kafka topic setup scripts
```

---

## Getting Started

### Prerequisites

| Requirement | Version | Install |
|-------------|---------|---------|
| Rust | 1.80+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | [nvm](https://github.com/nvm-sh/nvm) recommended |
| Docker | Latest | [Docker Desktop](https://www.docker.com/products/docker-desktop/) |
| Docker Compose | v2+ | Included with Docker Desktop |

Optional CLI tools:

```bash
cargo install sqlx-cli --features postgres
cargo install cargo-watch
npm install -g pnpm
```

### Clone and Setup

```bash
git clone https://github.com/logisticos/logisticos.git
cd logisticos

# Run the first-time developer setup script
bash scripts/dev-setup.sh
```

The setup script will:
- Check prerequisites
- Copy `.env.example` to `.env.local`
- Pull required Docker images
- Initialize the database schema
- Seed development data

### Run the Local Stack

The `docker-compose.yml` at the repository root starts the complete infrastructure and all 17 microservices:

```bash
# Start all infrastructure and services
docker compose up -d

# Start only the data infrastructure (no application services)
docker compose up -d postgres redis kafka zookeeper clickhouse

# Start infrastructure and enable optional developer tools
docker compose --profile tools up -d
```

**Infrastructure service endpoints after startup:**

| Service | URL |
|---------|-----|
| API Gateway | http://localhost:8000 |
| Grafana | http://localhost:3100 (admin / admin) |
| Prometheus | http://localhost:9090 |
| Jaeger (tracing) | http://localhost:16686 |
| MailHog (email) | http://localhost:8025 |
| MinIO console | http://localhost:9002 (minioadmin / minioadmin) |

**Optional developer tools** (requires `--profile tools`):

| Tool | URL |
|------|-----|
| Kafka UI | http://localhost:9093 |
| pgAdmin | http://localhost:5050 (admin@logisticos.dev / admin) |
| RedisInsight | http://localhost:5540 |

### Run Individual Services

All Rust services are members of the Cargo workspace. To run a service locally against the Dockerized infrastructure:

```bash
# Run a specific service with live reload
cargo watch -x 'run --bin logisticos-identity'

# Run with debug logging
RUST_LOG=debug cargo run --bin logisticos-dispatch

# Run all tests for a service
cargo test -p logisticos-order-intake

# Run workspace-wide checks
cargo clippy --all-targets --all-features
cargo test --workspace
```

**Required environment variables** (set in `.env.local` by the setup script):

```bash
DATABASE_URL=postgres://logisticos:password@localhost:5432/logisticos
REDIS_URL=redis://localhost:6379
KAFKA_BROKERS=localhost:9092
ANTHROPIC_API_KEY=<your-key>          # Required only for ai-layer service
RUST_LOG=info
```

### Run Frontend Apps

Each portal is a standalone Next.js application:

```bash
# Merchant Portal
cd apps/merchant-portal
npm install
npm run dev                            # http://localhost:3000

# Admin / Ops Portal
cd apps/admin-portal
npm install
npm run dev                            # http://localhost:3001

# Partner Portal
cd apps/partner-portal
npm install
npm run dev                            # http://localhost:3002

# Customer Portal
cd apps/customer-portal
npm install
npm run dev                            # http://localhost:3003
```

For the React Native apps:

```bash
# Driver App
cd apps/driver-app
npm install
npx expo start

# Customer App
cd apps/customer-app
npm install
npx expo start
```

### Database Migrations

```bash
# Run all pending migrations
bash scripts/migrate.sh

# Or use sqlx-cli directly
sqlx migrate run --database-url postgres://logisticos:password@localhost:5432/logisticos

# Seed development data
bash scripts/seed-dev.sh
```

---

## Service Port Reference

| Port | Service |
|------|---------|
| 8000 | API Gateway |
| 8001 | Identity & Tenant Management |
| 8002 | Customer Data Platform (CDP) |
| 8003 | Unified Engagement Engine |
| 8004 | Order & Shipment Intake |
| 8005 | Dispatch & Routing |
| 8006 | Driver Operations |
| 8007 | Customer Delivery Experience |
| 8008 | Fleet Management |
| 8009 | Warehouse & Hub Operations |
| 8010 | Carrier & Partner Management |
| 8011 | Proof of Delivery |
| 8012 | Payments & Billing |
| 8013 | Analytics & BI |
| 8014 | Marketing Automation Engine |
| 8015 | Business Logic & Automation Engine |
| 8016 | AI Intelligence Layer |
| 5432 | PostgreSQL |
| 6379 | Redis |
| 9092 | Kafka |
| 8123 | ClickHouse HTTP |
| 9090 | Prometheus |
| 3100 | Grafana |
| 16686 | Jaeger |

---

## Development Workflow

### Branch Strategy

```
main                    ← production-ready, protected
feat/<service>/<desc>   ← feature work (e.g., feat/dispatch/driver-priority-scoring)
fix/<service>/<desc>    ← bug fixes
chore/<desc>            ← tooling, deps, infrastructure
```

### Feature Development Process

1. **Open an issue** describing the feature or bug.
2. **Create a feature branch** from `main` using the naming convention above.
3. **Write the API contract first** — OpenAPI 3.1 spec (HTTP) or protobuf (gRPC) reviewed and merged before any implementation begins. Specs live in `docs/api/`.
4. **Create an ADR** for any architectural decision in `docs/adr/`. Use the existing ADRs as templates.
5. **Implement** with unit and integration tests. All services require tests for public API endpoints.
6. **Open a Pull Request** — minimum 2 approvals required; at least one from a senior engineer or architect.
7. **CI gates must pass:** `cargo clippy`, `cargo test`, security scan, performance regression check.
8. **Staging deploy** — E2E tests run against the staging environment before promotion.
9. **Production deploy** — canary rollout via Istio traffic splitting (10% → 50% → 100%).

### Code Quality Gates

```bash
# Must pass before committing
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace
cargo fmt --check

# For frontend
npm run lint
npm run typecheck
npm run test
```

### Commit Style

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(dispatch): add AI-based driver priority scoring
fix(payments): correct COD reconciliation rounding error
chore(deps): update axum to 0.7.5
docs(adr): add ADR-0009 for caching strategy
```

---

## Engineering Principles

### Architecture Constraints

- **Service Isolation** — each service owns its database schema; no cross-service database joins.
- **Event-First** — all state changes emit Kafka events; downstream services react asynchronously.
- **Multi-Tenancy** — Row-Level Security enforced at the PostgreSQL layer; tenant ID propagated via JWT claims.
- **Offline-First Mobile** — the driver app functions without connectivity and syncs on reconnect.
- **AI Fallbacks** — all AI-enhanced features have a deterministic non-AI fallback path.
- **ADR Required** — architectural decisions are documented in `/docs/adr/` before implementation.

### Rust Standards

- `#![deny(clippy::all)]` enforced on all crates.
- No `unwrap()` in production code paths — use `thiserror` / `anyhow` for error propagation.
- All database queries analyzed with `EXPLAIN`; no unbounded full table scans in production paths.

### Performance Targets

| Metric | Target |
|--------|--------|
| Operational API P99 latency | < 200ms |
| Dispatch assignment P99 | < 500ms |
| Live tracking end-to-end | < 2s |
| Notification delivery (WhatsApp/SMS) | < 5s from trigger |

### Security Requirements

- No secrets in code or `.env` files — all via HashiCorp Vault.
- mTLS between all internal services via Istio.
- Input validation at API boundary using Rust type system + `validator` crate.
- PCI-DSS scope minimization — payment data never stored outside the payments service.
- GDPR/PDPA compliance: consent required before behavioral tracking; right-to-erasure implemented.

---

## Documentation

| Resource | Location |
|----------|----------|
| Architecture Decision Records | [`docs/adr/`](docs/adr/) |
| OpenAPI + Protobuf Specs | [`docs/api/`](docs/api/) |
| Operational Runbooks | [`docs/runbooks/`](docs/runbooks/) |
| Architecture Diagrams | [`docs/architecture/`](docs/architecture/) |
| Project Instructions (AI context) | [`CLAUDE.md`](CLAUDE.md) |

### Key ADRs

| ADR | Title |
|-----|-------|
| [ADR-0001](docs/adr/0001-rust-for-all-backend-services.md) | Rust for All Backend Services |
| [ADR-0002](docs/adr/0002-event-driven-inter-service-communication.md) | Event-Driven Inter-Service Communication |
| [ADR-0003](docs/adr/0003-row-level-security-for-multi-tenancy.md) | Row-Level Security for Multi-Tenancy |
| [ADR-0004](docs/adr/0004-mcp-for-ai-interoperability.md) | MCP for AI Interoperability |
| [ADR-0005](docs/adr/0005-hexagonal-architecture-for-microservices.md) | Hexagonal Architecture for Microservices |
| [ADR-0006](docs/adr/0006-kafka-event-streaming-topology.md) | Kafka Event Streaming Topology |
| [ADR-0007](docs/adr/0007-offline-first-driver-app.md) | Offline-First Driver App |
| [ADR-0008](docs/adr/0008-multi-tenancy-rls-strategy.md) | Multi-Tenancy RLS Strategy |

---

## Contributing

1. Read [`CLAUDE.md`](CLAUDE.md) for architecture context and non-negotiable constraints.
2. Review open issues on GitHub before starting work.
3. Follow the [Development Workflow](#development-workflow) — spec-first, ADR for architecture decisions.
4. All PRs require 2 approvals and passing CI gates.
5. For significant changes, open a discussion issue before writing code.

### Non-Negotiables

- Zero-downtime deployments — all services must support rolling updates.
- Audit logging — all mutations logged with actor, timestamp, tenant, and IP.
- Rate limiting — all public-facing APIs rate-limited per tenant and per API key.
- i18n from day one — UI must support internationalization (EN and PH priority).
- WCAG 2.1 AA minimum for all web portals.
- Mobile-first design for all customer and driver interfaces.

---

## License

Copyright (c) LogisticOS Team. All rights reserved.

This software is proprietary and unlicensed for distribution. See [LICENSE](LICENSE) for details.
