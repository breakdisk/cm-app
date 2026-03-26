# Changelog

All notable changes to LogisticOS will be documented in this file.

This file follows the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.
LogisticOS adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- Merchant Portal responsive redesign — glassmorphism dashboard with KPI sparklines, hero banner, AI insights panel
- Mobile-first responsive layout with hamburger navigation (< 768px breakpoint)
- PostCSS + Tailwind CSS pipeline configured for all four web portals
- Driver app offline sync service with SQLite queue and NetInfo connectivity listener
- Driver app secure token store via `expo-secure-store` (Keychain/Keystore)
- Admin Portal API clients for fleet, drivers, analytics, hubs, and carriers
- Customer Portal reschedule and feedback pages wired to live API endpoints
- Business Logic service infrastructure: Redis rule cache, reqwest HTTP client, Kafka event publisher
- Fleet Management page with real-time vehicle telemetry and fuel-level indicators
- Drivers Management page with KPI summary and live status grid
- Analytics page with delivery trend charts and zone performance metrics
- Architecture Decision Record: ADR-0008 — Multi-Tenancy RLS Strategy

### Fixed
- `next.config.ts` replaced with `next.config.mjs` across all portals (Next.js 14 requires `.mjs`)
- `tsconfig.json` path aliases (`@/*`) added to all portal configs
- `getBusinessDays` function corrected to use `new Date()` instead of hardcoded date
- Unused `useRef` import removed from driver app root layout

### Infrastructure
- Tailwind plugin dependencies installed: `tailwindcss-animate`, `@tailwindcss/typography`, `@tailwindcss/container-queries`
- `postcss.config.js` created for all four Next.js portals

---

## [0.2.0] — 2026-01-15

### Added
- AI Intelligence Layer: dispatch agent, logistics planner, support agent scaffolding
- MCP server interface for all 17 microservices (see ADR-0004)
- Engagement Engine: WhatsApp, SMS, Email, and Push channel integrations
- Customer Delivery Experience service with live tracking and ETA updates
- Partner Portal: carrier performance dashboard, SLA tracking, payout views
- Customer Portal: branded tracking page, delivery feedback, reschedule flow
- Driver Super App: route navigation, task list, barcode scanner, offline POD capture
- Customer App: shipment tracking, booking, loyalty, push notifications
- Architecture Decision Records: ADR-0005 through ADR-0008
- Operational runbooks: deployment, incident response, Kafka operations
- Architecture diagrams: system overview, data flow

### Changed
- Dispatch service upgraded to support AI-assisted VRP optimization
- Order Intake service now publishes `order.created` Kafka events consumed by Engagement Engine
- Identity service extended with tenant-scoped RBAC enforcement

### Fixed
- Race condition in driver location update handler under high-concurrency load
- Kafka consumer group rebalancing causing duplicate POD submissions
- TimescaleDB hypertable chunk interval tuned for GPS telemetry ingestion rate

---

## [0.1.0] — 2025-10-01

### Added
- Initial monorepo scaffold with 17 Rust microservices
- Identity & Tenant Management service with OAuth 2.0 / OIDC, JWT, multi-tenancy
- Order & Shipment Intake service with address validation and AWB generation
- Dispatch & Routing service with PostGIS-backed geospatial queries
- Fleet Management service with TimescaleDB telemetry ingestion
- Payments & Billing service with COD reconciliation and invoice generation
- Analytics & BI service with ClickHouse OLAP queries
- Merchant Portal (Next.js 14): shipment booking, bulk upload, billing dashboard
- Admin / Ops Portal (Next.js 14): dispatch console, live driver map, hub operations
- Shared Rust crates: `common`, `config`, `errors`, `auth`, `tracing`, `types`, `events`, `geo`, `ai-client`
- Protobuf definitions for all inter-service gRPC contracts
- Docker Compose stack: PostgreSQL 16, Redis 7, Kafka 3.7, ClickHouse, TimescaleDB
- Kubernetes manifests and Helm charts for all services
- Terraform infrastructure modules (AWS/GCP/Azure)
- GitHub Actions CI/CD pipelines: lint, test, security scan, build, deploy
- Prometheus + Grafana observability stack
- Architecture Decision Records: ADR-0001 through ADR-0004
- CLAUDE.md project specification and engineering standards

---

[Unreleased]: https://github.com/your-org/logisticos/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/your-org/logisticos/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/your-org/logisticos/releases/tag/v0.1.0
