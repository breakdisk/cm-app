# ADR-0009: Multi-Product Platform Gateway Topology

**Status:** Accepted
**Date:** 2026-04-12
**Deciders:** Principal Architect, CTO, CPO

## Context

CargoMarket is evolving from a single-product company (LogisticOS — last-mile delivery) into a multi-product platform. Planned products on the same shared customer base:

- **LogisticOS** — last-mile delivery and logistics ops (current)
- **Carwash on-Demand** — bookings, technicians, slot management
- **Maintenance on-Demand** — work orders, parts, scheduling
- **MICE OS** — meetings/incentives/conferences/events ticketing
- **Ride-Hailing** — driver matching, trip lifecycle, fare calculation, surge pricing (planned)
- **Food Delivery** — restaurant catalog, order routing, courier dispatch, real-time tracking (planned)

The architecture must accommodate all six products without re-litigating gateway, identity, billing, or engagement decisions per product. New products should be additive: stand up a new gateway, reuse the platform.

These products share customers, identity, billing, communication channels, and AI infrastructure but have **independent operational domains**, **independent release cadences**, and likely **independent product teams**.

The current setup has a single `api-gateway` service exposed at `api.os.cargomarket.net`, which the 5 LogisticOS portals (merchant, admin, partner, customer landing page, OS landing page) all point at. When this gateway was deleted from Dokploy during recent infra cleanup, every portal broke simultaneously — the exact failure mode this ADR aims to eliminate going forward.

Two anti-patterns must be avoided:

1. **One mega-gateway for all products** — single point of failure, deploy coupling, route configuration explosion, organizational coupling (one team blocks all others), unbounded blast radius.
2. **No gateway / direct service exposure** — loses centralized auth, rate limiting, observability, API versioning, tenant isolation enforcement.

## Decision

Adopt a **two-tier gateway topology with subdomain-per-product isolation**:

1. **One Platform Gateway** at `api.cargomarket.net` — fronts cross-product *platform services* (identity, tenants, billing, CDP, engagement, AI layer, analytics).
2. **One Product Gateway per product**, on a dedicated subdomain:
   - `logistics.api.cargomarket.net` → LogisticOS services
   - `carwash.api.cargomarket.net` → Carwash services
   - `maintenance.api.cargomarket.net` → Maintenance services
   - `mice.api.cargomarket.net` → MICE OS services
   - `rides.api.cargomarket.net` → Ride-hailing services (planned)
   - `food.api.cargomarket.net` → Food delivery services (planned)

Each gateway is its own deployable unit (own Dokploy app, own image, own Traefik routes, own TLS cert, own scaling profile).

## Architecture

```
                  ┌───────── Traefik (Dokploy edge) ─────────┐
                  │   TLS termination, host-based routing    │
                  └─┬─────┬─────┬─────┬─────┬─────┬──────────┘
                    │     │     │     │     │     │
                  api.* logistics.* carwash.* maint.* mice.* admin.*
                    │     │           │         │       │       │
              ┌─────▼─┐ ┌─▼────┐  ┌───▼──┐  ┌───▼──┐  ┌─▼────┐  │
              │Platform│ │LogOS │  │Carwsh│  │Maint │  │MICE  │  │
              │Gateway │ │GW    │  │GW    │  │GW    │  │GW    │  │
              └───┬────┘ └──┬───┘  └──┬───┘  └──┬───┘  └──┬───┘  │
                  │         │         │         │         │      │
       ┌──────────▼─────────▼─────────▼─────────▼─────────▼──────▼─┐
       │      SHARED PLATFORM SERVICES (cross-product, single src) │
       │  identity · tenants · billing · cdp · engagement          │
       │  ai-layer · analytics · audit · notifications             │
       └────────────────────────────────────────────────────────────┘
                                    │
       ┌────────────────────────────▼──────────────────────────────┐
       │     PRODUCT-SPECIFIC SERVICES (isolated stacks)           │
       │                                                            │
       │  LogOS:   dispatch · driver-ops · pod · fleet · hub        │
       │           carrier · order-intake · delivery-experience     │
       │                                                            │
       │  Carwash: bookings · technicians · slots · pricing         │
       │                                                            │
       │  Maint:   work-orders · parts · schedules · sla            │
       │                                                            │
       │  MICE:    events · tickets · attendees · scanners · venues │
       └────────────────────────────────────────────────────────────┘
```

## Platform vs Product Boundary

The platform/product split is the most important decision in this ADR. A service is **platform** if and only if (a) more than one product needs it and (b) it can be evolved without product-specific knowledge.

### Platform services (shared, behind `api.cargomarket.net`)

| Service | Why platform |
|---------|-------------|
| **identity** | Every product authenticates the same human/tenant |
| **tenants** | Tenant lifecycle is product-agnostic |
| **billing & payments** | One invoice can span products; one wallet per customer |
| **cdp** | Unified customer profile across product touchpoints |
| **engagement** | WhatsApp/SMS/Email/Push are channels, not domain logic |
| **ai-layer** | Agents reach into any product via MCP (see ADR-0004) |
| **analytics** | Cross-product BI and exec dashboards |
| **audit** | Regulatory audit log spans all products |
| **notifications** | Push token registry, preference center |

### Product services (isolated, behind `<product>.api.cargomarket.net`)

| Product | Services |
|---------|----------|
| **LogisticOS** | order-intake · dispatch · routing · driver-ops · fleet · hub-ops · carrier · pod · delivery-experience · marketing-automation (logistics-specific) · business-logic |
| **Carwash** | bookings · technicians · slots · service-catalog · pricing-engine |
| **Maintenance** | work-orders · parts-inventory · technician-scheduling · sla-engine |
| **MICE OS** | events · ticket-types · attendees · check-in-scanners · venue-mgmt · seating |
| **Ride-Hailing** | trip-lifecycle · driver-matching · fare-calculator · surge-pricing · trip-tracking · driver-earnings |
| **Food Delivery** | restaurant-catalog · menu-mgmt · order-routing · courier-dispatch · live-tracking · merchant-payouts |

### Boundary rules

1. **Product services may call platform services**, never the reverse. Platform services must not import any product domain types.
2. **Product services may not call other products' services directly.** Cross-product workflows go through Kafka events (see ADR-0002, ADR-0006) or through the AI Layer via MCP.
3. **A service starts as a product service.** It earns "platform" status only when a second product needs it. Premature platformization is the second-worst trap after the mega-gateway.
4. **Watch the field-ops cluster.** LogisticOS, Ride-Hailing, and Food Delivery all share underlying primitives: driver/courier identity, real-time GPS ingestion, geospatial dispatch, ETA prediction, in-app navigation, earnings/payouts. When the second of these products goes live, extract these into a **`field-ops` platform tier** rather than copying them. Candidates for extraction: `gps-ingest`, `geospatial-dispatch`, `eta-engine`, `earnings-ledger`, `worker-identity` (the human operating in the field, distinct from the customer). This extraction needs its own ADR when the time comes.

## Role Separation: Platform / Partner / Merchant / Customer

The gateway topology only makes sense once the actor model is locked down. The platform is multi-tenant white-label SaaS — the people using the system at every layer have distinct roles, distinct authority, distinct money flows, and distinct branding. Conflating any two of them breaks multi-tenancy, billing, or RLS.

### The five-actor model

| Actor | Identity in code | Tenant relationship | Money flow | Branding seen |
|-------|------------------|---------------------|------------|---------------|
| **Platform** (CargoMarket) | n/a — owns the system | Owns all tenants | Receives subscription + per-shipment fees from Partners | n/a (internal only) |
| **Partner** (logistics company) | `Tenant` | **Is a tenant** | Pays Platform; charges Merchants and Customers | Their own brand (white-label) |
| **Operator** (Partner staff) | `User` with `operator` role, scoped to one Tenant | Belongs to one Partner | Salary from Partner (out of band) | Partner brand |
| **Merchant** (business shipping goods) | `Merchant` entity scoped to one Tenant | Customer of one Partner | Pays Partner monthly | Their own brand → falls back to Partner brand |
| **Driver** (field worker) | `User` with `driver` role, scoped to one Tenant | Employee/contractor of one Partner | Paid per task by Partner | Partner brand (driver app rarely white-labeled) |
| **Customer** (end recipient or self-booking sender) | `Customer` profile, **platform-tier** (CDP) | Cross-tenant identity, per-shipment scoped | Pays via card/COD; money flows to Partner | Merchant brand → Partner brand → neutral |

### Critical rules

1. **Partner = Tenant.** All multi-tenancy lives at the Partner level. RLS isolates Partner A's data from Partner B's data. There is no concept of a "Partner inside a Partner."
2. **Operator is tenant-scoped.** An operator at Partner X cannot see Partner Y's data, ever. Geo/hub scoping layers on top of tenant scoping for sub-roles (dispatcher for region R, hub supervisor for hub H, finance clerk).
3. **Merchant is tenant-scoped.** When Partner X onboards Merchant M, that merchant exists *only* in tenant X. Same business entity using two Partners = two separate merchant records (they are commercial competitors, not unified).
4. **Driver is tenant-scoped.** Each driver belongs to exactly one Partner. Sub-carrier subcontracting moves *shipments* between Partners, not drivers.
5. **Customer is platform-tier.** Customer profile lives in the platform CDP, identified by phone number. A customer who ships with Partner A and later receives a package from Partner B is the same human in the CDP. **Each Partner only sees their own slice of the customer's history** — RLS at the CDP read API enforces this. The platform tier owns the unified profile; the Partner tier sees a tenant-filtered view.
6. **Platform is invisible to everyone except Partners.** CargoMarket logo never appears in Merchant, Driver, Operator, or Customer surfaces. The white-label is total.

### Anti-pattern: "give Partners a Merchant account for self-booking"

**Rejected.** A Partner is not a customer of itself. Giving Partners a merchant account collapses three distinct concepts:

| Concept | What it actually is |
|---------|---------------------|
| **Walk-in customer (operator-booked)** | Partner operator books a one-off shipment on behalf of a customer who walked into a hub or called the office. Created with `actor=operator_id`, `merchant=null`, lightweight customer profile attached. Billing rolls into the Partner's "direct sales" ledger. |
| **Sub-carrier marketplace** | Partner A subcontracts overflow to Partner B. This is a B2B carrier contract with negotiated lane rates, SLA, periodic settlement. Lives in the Carrier & Partner Management service. Different schema, different invoice type, different UI. Partner A is not a "merchant of B" — they are a `carrier_partner` entity with its own settlement flow. |
| **Real merchant** | Self-service business that signs up with one Partner via the Partner's branded merchant portal. Postpaid monthly invoice. Negotiated rates per merchant. Brand cascade: merchant brand → Partner brand. |

These three are different concepts with different billing, different RLS, different audit trails. Collapsing them into "everyone has a merchant account" loses the ability to charge them differently, brand them differently, audit them differently, and enforce contracts differently.

### Customer invoice and tracking delivery

The end customer receives invoices and tracking via **three channels in parallel**, dispatched by the Engagement Engine on every state change:

| Channel | When it fires | Required state | Owner |
|---------|--------------|----------------|-------|
| **Customer app** (push + in-app screen) | Real-time on every state change | Customer has installed app and linked phone via OTP | Engagement Engine → push token in CDP |
| **WhatsApp / SMS fallback** | Real-time on every state change | Customer phone number captured at booking — **always available** | Engagement Engine → channel adapter |
| **Email** | Issued for invoice/receipt + status digests | Customer provided email (optional at booking) | Engagement Engine → SES adapter |

**Decision logic at POD time:**

```
on pod.captured event:
  payments.issue_shipment_invoice(shipment_id) → publishes InvoiceGenerated
  engagement.invoice_consumer:
    1. Look up customer channel preferences in CDP
    2. Fan out to ALL available channels:
       - has_app_token        → push notification + in-app Invoices screen
       - has_whatsapp_consent → WhatsApp message with PDF link
       - has_email            → email with PDF attachment
       - fallback             → SMS with short tracking URL
    3. Respect per-customer mute/preference settings from CDP
```

**Critical separation of responsibility:**
- **Payments service** owns invoice *generation*. Publishes `InvoiceGenerated` to Kafka.
- **Engagement Engine** owns invoice *delivery*. Consumes `InvoiceGenerated`, fans out across channels.
- **CDP** owns customer channel preferences and contact details.
- **No service owns end-to-end "send invoice."** This is by design — it makes channel addition (e.g. Viber, Telegram) a one-service change.

**Branding cascade on the customer-facing surfaces (tracking page, invoice PDF, email template):**

```
1. Merchant brand        (if merchant has paid for branding tier and configured logo/colors)
       ↓ fall back to
2. Partner brand         (Partner's logo, colors, domain — always configured)
       ↓ fall back to
3. Neutral platform skin (only if Partner has not configured branding — should not happen in production)
```

CargoMarket branding **never** appears on customer-facing surfaces.

## Gateway Responsibilities

Every gateway (platform or product) is a thin Rust + Axum binary (reuse `services/api-gateway`) responsible for:

- **TLS termination** (delegated to Traefik in Dokploy; gateway speaks plain HTTP inside the cluster)
- **JWT validation** against the platform identity service
- **Tenant context propagation** — extract `tenant_id` from JWT, inject into request context for downstream services and RLS (see ADR-0008)
- **Rate limiting** per tenant + per API key (Redis-backed)
- **Request/response logging** with trace ID propagation (OpenTelemetry, see ADR linked from CLAUDE.md)
- **API versioning** (`/v1/...`, `/v2/...` prefixes routed to different upstream versions)
- **Routing** to the correct upstream service via service discovery (Docker DNS in Dokploy; Kubernetes Service in K8s)

A gateway never embeds business logic. If you find yourself adding domain logic to a gateway, that logic belongs in a service.

## Frontend Configuration

Each portal/app needs **two** API base URLs, not one:

```env
NEXT_PUBLIC_PLATFORM_API=https://api.cargomarket.net
NEXT_PUBLIC_PRODUCT_API=https://logistics.api.cargomarket.net
```

For mobile apps (Expo):

```env
EXPO_PUBLIC_PLATFORM_API=https://api.cargomarket.net
EXPO_PUBLIC_PRODUCT_API=https://logistics.api.cargomarket.net
```

The HTTP client picks the right base URL by domain (auth → platform; shipments → product). New products copy this two-base pattern; the platform base never changes.

## Migration Plan

### Phase 1 — Recover the broken state (immediate)

1. Recreate the LogisticOS API gateway as a Dokploy app **at `logistics.api.cargomarket.net`** (not `api.*`). Use the existing `ghcr.io/breakdisk/logisticos-service-api-gateway:latest` image.
2. Update the 5 LogisticOS portals' env vars to point at the new subdomain. Redeploy.
3. Verify all portals authenticate, list shipments, render maps.

### Phase 2 — Stand up the platform gateway (this sprint)

1. Create a new Dokploy app `platform-gateway` at `api.cargomarket.net`.
2. Configure it to route to identity, billing, cdp, engagement, ai-layer.
3. Update portals to use **two** base URLs (platform + product).
4. Migrate auth flows to call `api.cargomarket.net/v1/auth/*`.

### Phase 3 — Document the boundary (this sprint)

1. Add a `services/PLATFORM.md` listing every service and its tier (platform / product / which product).
2. Add a CI check that fails the build if a platform service imports from a product crate.

### Phase 4 — Onboard new products (per product)

For each new product (Carwash, Maintenance, MICE, Ride-Hailing, Food Delivery):

1. Create `services/<product>/` directory with its own Cargo workspace member.
2. Create `<product>-gateway` Dokploy app at `<product>.api.cargomarket.net`.
3. Reuse the existing platform gateway — do not duplicate identity, billing, etc.
4. New product portals get the same two-base-URL pattern.

## Consequences

### Positive

- **Independent blast radius.** A bug in Carwash cannot break LogisticOS. The April 2026 incident (deleted gateway took down all 5 portals) becomes structurally impossible across products.
- **Independent deploy cadence.** Each product team owns one stack end-to-end.
- **Clean URL semantics.** `logistics.api.cargomarket.net` tells you exactly which product handles the request — useful for logs, billing, support, customer trust.
- **TLS isolation.** Each subdomain has its own cert; one rotation failure does not cascade.
- **Org scaling.** Teams map onto subdomains. New product = new gateway, no contention with existing teams.
- **Caching/CDN per product.** Different products have different cacheability profiles; per-subdomain CDN config is straightforward.

### Negative

- **More gateway instances to operate.** N+1 gateways instead of 1. Mitigated by templating: every gateway is the same Rust binary with different routing config.
- **DNS and cert management overhead.** Each new product needs a new subdomain provisioned. Mitigated by Dokploy's automatic cert management.
- **Frontend complexity.** Apps need two base URLs instead of one. Acceptable cost for the isolation gained.
- **Risk of premature platformization.** Teams will be tempted to call something "platform" the moment two products consume it. Counter this with the rule: a service is product-tier until proven otherwise, and platformization requires an ADR.

### Neutral

- **Cross-product workflows still need design.** Example: a customer who has both LogisticOS shipments and a MICE event ticket gets one notification feed. This is solved by the platform engagement service consuming product events from Kafka (see ADR-0006). This ADR does not change that pattern, but it does enforce that the integration happens via events, not direct calls.

## Alternatives Considered

### Alternative 1: One mega-gateway for all products

Single `api.cargomarket.net` routing all paths to all products via path prefixes (`/logistics/`, `/carwash/`, `/mice/`).

**Rejected** because:
- It is exactly the failure mode that broke us in April 2026 (single gateway = single point of failure).
- Route config grows unbounded as products are added.
- Deploy coupling: every product team waits on one gateway release pipeline.
- One team's misconfigured route can break every product.

### Alternative 2: Path-based routing per product (`api.cargomarket.net/logistics/...`)

Single gateway, but paths are namespaced.

**Rejected** because:
- Still couples all products to one gateway and one TLS cert.
- Harder to give product teams autonomy over their gateway config.
- No independent rate limiting policies per product without significant gateway logic.
- Worse for caching (one cache invalidation namespace) and observability (one metrics namespace).

### Alternative 3: Direct service exposure (no gateway)

Each service exposes its own domain.

**Rejected** because:
- Loses centralized auth, rate limiting, tenant context propagation.
- Cert management explodes (one cert per service, dozens of services).
- Frontend has to know dozens of base URLs.
- Tenant isolation enforcement becomes per-service instead of centralized.
- Acceptable for 3–4 services; suicide at 20+.

### Alternative 4: Service mesh only (Istio/Linkerd, no gateways)

Use a service mesh for everything, expose services via mesh ingress.

**Rejected** because:
- Service mesh handles east-west traffic well but is not a substitute for a north-south gateway with auth and tenant context.
- We are not on Kubernetes yet (Dokploy is Docker Compose). Service mesh is a future option layered on top of this ADR, not a replacement for it.

## References

- ADR-0002: Event-Driven Inter-Service Communication
- ADR-0004: MCP for AI Interoperability (cross-product AI access)
- ADR-0006: Kafka Event Streaming Topology (cross-product async integration)
- ADR-0008: Multi-Tenancy RLS Strategy (tenant context propagation through gateways)
- April 2026 incident: deletion of `api.os.cargomarket.net` Dokploy app caused simultaneous outage of 5 portals (root cause for this ADR)
