# ADR-0013: Partner Scoping and the Alliance Model

**Status:** Accepted
**Date:** 2026-04-19
**Deciders:** Principal Architect, Senior Rust Engineer — Identity & Auth, Senior Rust Engineer — Carrier Management, Database Reliability Engineer, CISO, Engineering Manager — Platform Core

> **Zero-Loss Constraint.** The rollout must preserve a hard invariant: *no shipment row becomes invisible to its legitimate operator at any point during or after the migration.* Every design choice below — two-phase handoffs, Legacy Partner backfill, fail-closed GUC policy, service-by-service canary — exists to honor that invariant. The AWB format is explicitly unchanged for the same reason: a reprint would strand physical parcels whose labels were issued before the change.

---

## Context

LogisticOS enforces tenant isolation via Row-Level Security (ADR-0008). Every operational row — shipments, drivers, routes, POD records, invoices — is scoped by `tenant_id` and visible only to users of that tenant.

The tenant model, as shipped today, collapses three very different real-world entities into one scope:

1. **The platform operator (Tenant)** — e.g. Cargomarket running LogisticOS for the region.
2. **The logistics partner (Partner)** — an independent last-mile fleet that the Tenant onboards. Partners have their own drivers, vehicles, hubs, commission rates, and SLAs.
3. **The shipping merchant (Merchant)** — a business that books shipments with a Partner (directly or brokered via the Tenant).

Today, all three see the same rows. A Partner logging into the partner portal sees *every* driver under the Tenant, not just the drivers on their own fleet. A Merchant (when merchant portal rolls out further) would see every shipment in the Tenant, not only their own bookings. This is acceptable for a single-tenant pilot but breaks the moment a second partner is onboarded or a merchant with sensitive volume data needs access.

### Evidence from in-progress work

While wiring cross-portal deep links between admin and partner portals (Tasks 4–15 on branch `feat/admin-portal/live-roster`), it became clear that:

- `driver_ops.drivers` has no `partner_id` column; the partner portal's driver list is "every driver under this tenant".
- `carrier.carriers` exists and models third-party delivery companies, but rows are **not linked to login users** — there is no way to say "user X is a representative of Carrier Y".
- JWT claims carry `tenant_id`, `user_id`, `roles` but no concept of a partner-scoped identity.
- The partner portal's Rates, SLA, Manifests, and Drivers pages implicitly assume a single scope and would silently show cross-partner data if a second partner were onboarded.

### What the industry does

| Platform | Pattern |
|----------|---------|
| **Shopify** | `shop_id` as a second scope below the platform, with M:N staff memberships |
| **Stripe Connect** | Platform has `account_id` children; each connected account is its own isolation boundary, with the platform orchestrating across them |
| **Auth0 Organizations** | Users belong to N organizations; active org is selected at login and carried in the token |
| **Salesforce Territories** | Hierarchical scoping layered over the core tenant |
| **AWS Organizations / OUs** | Root account with nested organizational units; policies inherit downward |

The common pattern: **a second (and sometimes third) scope layer below the tenant, with M:N user memberships and an active-scope claim in the session token.**

### Why this needs an ADR

This is not a feature. It is a new scope dimension that must be enforced at the database layer for every service that stores partner-owned or merchant-owned data. It affects:

- The identity schema (new tables, new JWT claims).
- Every service with partner-scoped data (driver-ops, fleet, hub-ops, carrier, POD, payments).
- The API Gateway (claim validation, context-switch endpoint).
- The AI Intelligence Layer and MCP tools (ADR-0004) — agent tool calls must respect partner scope.
- The multi-product platform topology (ADR-0009) — the Alliance model extends beyond LogisticOS into Carwash, MICE, Ride-Hailing, etc.

Getting this wrong later means a multi-service migration under live load. Getting it right now means defining the contract once.

---

## Decision

We introduce a **three-tier sovereignty model** — *Tenant → Partner → Merchant* — and call the Tenant+Partners federation an **Alliance**. Scope isolation at each tier is enforced the same way tenant isolation is enforced today: Row-Level Security at the PostgreSQL layer, session GUC parameters set per transaction, and claims propagated from a signed JWT.

### The three tiers

| Tier | Entity | Sovereignty | Examples |
|------|--------|-------------|----------|
| **Tenant** | The platform instance. Owns the Alliance. | Sees everything within the Alliance. | Cargomarket PH |
| **Partner** | An independent fleet operator inside the Alliance. Owns its drivers, vehicles, hubs, rates, SLAs. | Sees only its own operational rows + shipments routed to it. | "FastLine Couriers", "NorthLink Logistics" |
| **Merchant** | A shipping customer. Books shipments against the Alliance. | Sees only its own shipments and invoices. | "Acme E-commerce", individual Balikbayan senders |

A **Carrier** (existing `carrier.carriers` table) remains a separate concept: a *third-party, non-member* delivery company the Tenant brokers handoffs to. Partners are members of the Alliance (they log in, run drivers, have two-way visibility with the Tenant). Carriers are integration endpoints (API/EDI/manual, no user accounts in the platform).

> **Why Partner ≠ Carrier.** An earlier draft proposed unifying them behind one `participants` table. We rejected that because the lifecycle differs: Partners go through member onboarding, get seats, publish rates, and can own multi-fleet child entities over time (a Partner acquiring another fleet becomes a *Multi-Fleet Partner* — still one billing relationship, multiple operational scopes). Carriers are contract-driven allocation targets. Merging them would force every carrier row through membership plumbing it doesn't need, and every partner row through SLA-contract plumbing it doesn't own.

### Identity schema

Three new tables in the `identity` schema:

```sql
-- Partners = members of the Alliance
CREATE TABLE identity.partners (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL REFERENCES identity.tenants(id),
    legal_name         TEXT NOT NULL,
    display_name       TEXT NOT NULL,
    status             TEXT NOT NULL CHECK (status IN ('pending','active','suspended','terminated')),
    parent_partner_id  UUID REFERENCES identity.partners(id), -- multi-fleet group hierarchy
    is_legacy_sink     BOOLEAN NOT NULL DEFAULT FALSE,        -- reserved for backfill of pre-migration rows
    onboarded_at       TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_partners_tenant_id ON identity.partners (tenant_id);
CREATE INDEX idx_partners_parent ON identity.partners (parent_partner_id);
CREATE UNIQUE INDEX idx_partners_legacy_per_tenant
    ON identity.partners (tenant_id) WHERE is_legacy_sink = TRUE;

-- Merchants = shipping customers of the Alliance
CREATE TABLE identity.merchants (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL REFERENCES identity.tenants(id),
    legal_name     TEXT NOT NULL,
    display_name   TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('pending','active','suspended','terminated')),
    is_legacy_sink BOOLEAN NOT NULL DEFAULT FALSE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_merchants_tenant_id ON identity.merchants (tenant_id);
CREATE UNIQUE INDEX idx_merchants_legacy_per_tenant
    ON identity.merchants (tenant_id) WHERE is_legacy_sink = TRUE;

-- M:N user ↔ partner membership
CREATE TABLE identity.partner_memberships (
    user_id     UUID NOT NULL REFERENCES identity.users(id),
    partner_id  UUID NOT NULL REFERENCES identity.partners(id),
    tenant_id   UUID NOT NULL,  -- denormalized for RLS predicate
    role        TEXT NOT NULL CHECK (role IN ('partner_admin','dispatcher','driver_ops','viewer')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, partner_id)
);

CREATE INDEX idx_partner_memberships_partner ON identity.partner_memberships (partner_id);
CREATE INDEX idx_partner_memberships_user ON identity.partner_memberships (user_id);

-- M:N user ↔ merchant membership (same shape, different scope)
CREATE TABLE identity.merchant_memberships (
    user_id      UUID NOT NULL REFERENCES identity.users(id),
    merchant_id  UUID NOT NULL REFERENCES identity.merchants(id),
    tenant_id    UUID NOT NULL,
    role         TEXT NOT NULL CHECK (role IN ('merchant_admin','booker','billing','viewer')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, merchant_id)
);
```

A user can belong to the Tenant directly (`tenant_admin` role on `users`), or to one or more Partners, or to one or more Merchants, or any combination. The active scope is selected per session.

### JWT claims

Claims are extended with optional active-scope fields:

```json
{
  "tid":  "<tenant_id>",            // always present
  "uid":  "<user_id>",              // always present
  "roles": ["partner_admin"],       // always present
  "pid":  "<active_partner_id>",    // present when user is acting as a Partner
  "mid":  "<active_merchant_id>",   // present when user is acting as a Merchant
  "scope": "tenant | partner | merchant"
}
```

`pid` and `mid` are mutually exclusive. `scope=tenant` means the user is acting at the Alliance level (tenant_admin or platform ops) and sees everything within the tenant. Role-to-scope mapping:

| Scope | Required role on `users` / membership | RLS behavior |
|-------|---------------------------------------|-----------|
| `tenant` | `tenant_admin` or `platform_ops` on `users` | RLS predicate matches any `partner_id` / `merchant_id` within the tenant |
| `partner` | active membership in `partner_memberships` for `pid` | RLS predicate matches only rows where `partner_id = pid` |
| `merchant` | active membership in `merchant_memberships` for `mid` | RLS predicate matches only rows where `merchant_id = mid` |

### Context switching

A new endpoint on the identity service:

```
POST /v1/auth/switch-context
Headers: Authorization: Bearer <current_token>
Body:    { "scope": "partner", "partner_id": "<uuid>" }
Returns: { "access_token": "<new_jwt>", "refresh_token": "<new_refresh>" }
```

Behavior:
- Verifies the current token.
- Validates that the user has membership in the requested scope (`partner_memberships` or `merchant_memberships`) or has `tenant_admin` role.
- Issues a new JWT with `pid` / `mid` / `scope` set. Old token is not revoked — the session multiplexes.
- Rate-limited to 10 switches/minute per user to prevent churn.
- Every switch writes to `identity.scope_switch_audit (user_id, from_scope, to_scope, ip, user_agent, at)`.

### RLS extension

Every service with partner-scoped data adds a `partner_id UUID NOT NULL` column (the Legacy Partner UUID per tenant holds pre-migration and ad-hoc tenant-owned rows — see *Backfill strategy* below). Every service with merchant-scoped data adds `merchant_id UUID NOT NULL`.

The standard RLS policy gains two new scope predicates. Using driver-ops `drivers` as an example:

```sql
ALTER TABLE driver_ops.drivers ADD COLUMN partner_id UUID NOT NULL;
CREATE INDEX idx_drivers_tenant_partner ON driver_ops.drivers (tenant_id, partner_id);

ALTER TABLE driver_ops.drivers ENABLE ROW LEVEL SECURITY;
ALTER TABLE driver_ops.drivers FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_and_partner_isolation
    ON driver_ops.drivers
    AS PERMISSIVE
    FOR ALL
    TO logisticos_app
    USING (
        -- Fail closed: an unset tenant GUC or unset scope GUC denies everything.
        current_setting('app.current_tenant_id', true) IS NOT NULL
        AND current_setting('app.current_scope',     true) IS NOT NULL
        AND tenant_id = current_setting('app.current_tenant_id', true)::uuid
        AND (
            -- tenant-scope session: no partner filter
            current_setting('app.current_scope', true) = 'tenant'
            -- partner-scope session: partner must match
            OR (
                current_setting('app.current_scope', true) = 'partner'
                AND partner_id = current_setting('app.current_partner_id', true)::uuid
            )
        )
    )
    WITH CHECK (
        current_setting('app.current_tenant_id', true) IS NOT NULL
        AND current_setting('app.current_scope',     true) IS NOT NULL
        AND tenant_id = current_setting('app.current_tenant_id', true)::uuid
        AND (
            current_setting('app.current_scope', true) = 'tenant'
            OR (
                current_setting('app.current_scope', true) = 'partner'
                AND partner_id = current_setting('app.current_partner_id', true)::uuid
            )
        )
    );
```

The `tenant_admin` bypass is **not** a role-level `BYPASSRLS` — it is encoded into the policy predicate via `app.current_scope = 'tenant'`. This keeps the enforcement in one place (the policy) and audit trails remain uniform. `BYPASSRLS` continues to be held only by `logisticos_service` (migrations, ops tooling, Kafka consumers, outbox drainers — see *Service-role audit* below).

Merchant-scoped tables (shipments, invoices) layer an additional predicate with the same shape, reading `app.current_merchant_id`.

### Shipments — the special case

Shipments are *multi-scope*: they have a `merchant_id` (who booked it), a `partner_id` (who's fulfilling it), and a `tenant_id` (which Alliance). All three scopes need read access, each to their own rows. The policy OR-chains the three predicates:

```sql
USING (
    current_setting('app.current_tenant_id', true) IS NOT NULL
    AND current_setting('app.current_scope',     true) IS NOT NULL
    AND tenant_id = current_setting('app.current_tenant_id', true)::uuid
    AND (
        current_setting('app.current_scope', true) = 'tenant'
        OR (current_setting('app.current_scope', true) = 'partner'
            AND partner_id = current_setting('app.current_partner_id', true)::uuid)
        OR (current_setting('app.current_scope', true) = 'merchant'
            AND merchant_id = current_setting('app.current_merchant_id', true)::uuid)
        OR EXISTS (
            -- Time-bounded cross-partner read grant (prior fulfillment partner post-handoff).
            SELECT 1 FROM order_intake.shipment_scope_grants g
            WHERE g.shipment_id = shipments.id
              AND g.grantee_partner_id = current_setting('app.current_partner_id', true)::uuid
              AND g.expires_at > now()
        )
    )
)
```

### Cross-partner handoffs — two-phase, outbox-atomic

A shipment's `partner_id` can change during its lifecycle (Partner A runs first mile, Partner B runs last mile — common in multi-hub inter-island routes). The transition is **two-phase** and **transactionally coupled to the outbox**, so the shipment never enters a state where neither partner can see it.

```
partner_id=A, pending_partner_id=NULL           (stable)
  │
  │ shipment.handoff_requested (A proposes → B)
  ▼
partner_id=A, pending_partner_id=B              (A still sees it; B sees it via pending-partner predicate)
  │
  ├── handoff_accepted by B  → partner_id=B, pending_partner_id=NULL
  │                             + shipment_scope_grants(A, +30d read)
  │
  └── handoff_rejected by B  → pending_partner_id=NULL
                                (partner_id stays A)
```

The policy's OR-chain is extended to include `pending_partner_id = current_partner_id` so Partner B can see the shipment while the handoff is pending. There is no window in which the shipment is orphaned — at every instant at least one partner (and often both) has RLS visibility.

**Atomicity.** The state transition and the Kafka event are written in a single PostgreSQL transaction via `kafka_outbox`:

```sql
BEGIN;
  UPDATE order_intake.shipments
     SET partner_id = $new, pending_partner_id = NULL, updated_at = now()
   WHERE id = $shipment AND pending_partner_id = $new;   -- CAS on pending

  INSERT INTO order_intake.kafka_outbox (topic, key, payload)
    VALUES ('shipment.handoffs', $shipment, $event_json);

  INSERT INTO order_intake.shipment_scope_grants (shipment_id, grantee_partner_id, expires_at)
    VALUES ($shipment, $old_partner, now() + interval '30 days');
COMMIT;
```

The outbox drainer publishes to Kafka at-least-once. Consumers dedupe on event id. No commit → no event; no event → no visibility flip. A crashed drainer recovers by re-reading the outbox.

Handoff events live on Kafka topic `shipment.handoffs` with tenant_id, both partner ids, AWB, and accept/reject metadata.

### Backfill strategy — Legacy Partner, not tenant-default

For each existing tenant, migration creates one `identity.partners` row with `is_legacy_sink = TRUE` and `display_name = '<tenant> — Legacy'`. All pre-migration operational rows are backfilled with `partner_id = <Legacy Partner id>`. The same pattern applies to Merchants.

Visibility rules for the Legacy Partner:

- `tenant_admin` sees Legacy rows (via `scope=tenant` tier bypass) — always.
- All pre-existing `partner_memberships` for the tenant are seeded with a membership in the Legacy Partner at `role=viewer`. Historical visibility for real-world operators is preserved.
- New operational writes never target the Legacy Partner; APIs reject `partner_id=<legacy>` for fresh inserts. The sink is append-only-from-migration and read-many thereafter.
- Once a tenant has completed its partner onboarding, an operator-run `reassign_legacy_rows` job attributes Legacy rows to their real owners in bulk (audit-logged, idempotent).

Rationale: auto-assigning every legacy row to one *real* Partner would silently grant that Partner rights (commissions, visibility, liability) over rows it did not actually fulfill. The Legacy sink keeps provenance honest.

### Tables that gain partner_id

The migration touches every table where a Partner's operational sovereignty applies:

| Service | Tables gaining `partner_id` |
|---------|----------------------------|
| driver-ops | `drivers`, `driver_tasks`, `driver_locations`, `driver_shifts` |
| fleet | `vehicles`, `vehicle_telemetry`, `maintenance_records` |
| hub-ops | `hubs`, `hub_inbound`, `hub_outbound`, `dock_schedules` |
| carrier | (no change — Partners ≠ Carriers; the join table `partner_carrier_contracts` is new) |
| pod | `pod_records` (follows shipment's current partner_id) |
| payments | `commission_ledger`, `payout_schedules` |
| order-intake | `shipments` (partner_id = current fulfillment partner, plus `pending_partner_id`) |
| dispatch | `dispatch_assignments`, `route_plans` |

Tables that gain `merchant_id`: `shipments`, `invoices`, `cod_ledger`, `merchant_billing_accounts`, `marketing_campaigns`.

Tables explicitly exempt from partner scoping: `tenants`, `partners`, `merchants`, `users`, `kafka_outbox`, `schema_migrations`, `system_config`, and the new `identity.*_memberships` (which *are* the scoping mechanism and use their own visibility rules).

### Rust integration — libs/common

A new module `libs/common/src/db/scope.rs` extends the tenant context pattern:

```rust
// libs/common/src/db/scope.rs

pub enum SessionScope {
    Tenant,
    Partner(Uuid),
    Merchant(Uuid),
}

pub async fn set_session_scope(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    scope: SessionScope,
) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL app.current_tenant_id = $1")
        .bind(tenant_id.to_string())
        .execute(&mut *conn).await?;

    let (scope_str, pid, mid) = match scope {
        SessionScope::Tenant       => ("tenant",   None,        None),
        SessionScope::Partner(id)  => ("partner",  Some(id),    None),
        SessionScope::Merchant(id) => ("merchant", None,        Some(id)),
    };

    sqlx::query("SET LOCAL app.current_scope = $1")
        .bind(scope_str).execute(&mut *conn).await?;

    if let Some(p) = pid {
        sqlx::query("SET LOCAL app.current_partner_id = $1")
            .bind(p.to_string()).execute(&mut *conn).await?;
    }
    if let Some(m) = mid {
        sqlx::query("SET LOCAL app.current_merchant_id = $1")
            .bind(m.to_string()).execute(&mut *conn).await?;
    }
    Ok(())
}
```

The `TenantScopedTx` helper from ADR-0008 is superseded by `ScopedTx::begin(pool, tenant_id, scope)`. All existing repository code migrates by one line per repo. `ScopedTx::begin` is the *only* sanctioned path for `logisticos_app` to acquire a connection; direct `pool.acquire()` is linted out via `clippy-disallowed-methods`.

### MCP tools (ADR-0004) integration

Every MCP tool that reads or mutates partner-scoped data must accept the session scope. The AI Intelligence Layer, when invoking tools, passes the current user's active scope through — a dispatch agent operating on behalf of Partner A cannot assign drivers from Partner B even if the LLM "decides" to try. Enforcement is at the MCP server boundary (the Rust service), not at the LLM prompt layer.

Tool signatures gain a scope context parameter:

```
dispatch.assign_driver(shipment_id, driver_id, [scope: SessionScope])
driver_ops.get_driver_location(driver_id, [scope: SessionScope])
```

The scope is populated from the invoking user's JWT, not from tool input — the LLM cannot spoof it.

### Multi-product platform (ADR-0009) integration

The Alliance model generalizes. Each CargoMarket product has a different second-tier scope:

| Product | Tier-2 scope |
|---------|--------------|
| LogisticOS | Partner (fleet) |
| Carwash | Branch |
| Maintenance | Workshop |
| MICE | Venue |
| Ride-Hailing | Fleet Operator |
| Food Delivery | Restaurant |

The `identity.partners` table is product-specific conceptually but can be modeled as `identity.operating_units` with a `product_type` column shared across products. This ADR scopes the initial implementation to LogisticOS; a follow-up ADR-0014 generalizes to the multi-product platform.

---

## Tracking Number (AWB) Impact

**Decision: the AWB format does not change.**

Current format (from the Tracking Number Architecture): `CM-{TTT}-{S}{NNNNNNN}{C}` — platform prefix, 3-char tenant code, service code (S/E/D/B/I), 7-digit tenant-scoped sequence, Luhn mod-34 checksum.

### Why no partner segment

| Property | Implication |
|----------|-------------|
| **AWB identifies a parcel, not a scope.** | A parcel's partner changes during lifecycle (handoffs). A parcel's AWB cannot — the label is physical. Encoding partner into the AWB would require label reprint at every handoff. Impossible in the field. |
| **Uniqueness is tenant-scoped.** | The `NNNNNNN` sequence uses a tenant-wide generator. Partitioning by partner would require either (a) a partner segment (rejected above), (b) cross-partner coordination on sequence issuance (introduces a write hot-spot across Alliance), or (c) accepting collision risk (rejected). Keep the tenant-scoped generator as-is. |
| **Luhn checksum is partner-agnostic.** | The mod-34 computation reads only AWB characters; no entropy derives from tenant or partner. No math change. |
| **Service codes (S/E/D/B/I) stay.** | These encode shipment *type*, not ownership. ADR-0014 may add product-level codes when Carwash/MICE/etc. need AWBs; that is out of scope for partner scoping. |
| **Backward compatibility.** | Every pre-existing label must remain scan-valid. Format change on a live operation is a shipment-loss event, not a rollout. |

### What DOES change — AWB lookup path

AWB lookups must resolve a shipment regardless of the scanner's partner scope, because:

- A customer tracks by AWB on a public page (no partner context).
- A driver at Hub B scans a parcel handed off from Partner A before the handoff event has fully propagated.
- A carrier integration webhook posts a status update keyed by AWB alone.

Implementation:

1. `GET /v1/shipments/by-awb/:awb` runs under `scope=tenant` inside the service (via an explicit `ScopedTx::begin(pool, tenant_id, SessionScope::Tenant)`), *not* the caller's partner scope.
2. Every such lookup writes to `order_intake.awb_lookup_audit (awb, caller_user_id, caller_scope, resolved_partner_id, ip, at)`. Lookup volume is expected but auditable.
3. Response is filtered before return: if the caller is `scope=partner(P)` and the shipment's current `partner_id ≠ P` and there is no active `shipment_scope_grants` or `pending_partner_id` match, the response strips commercial fields (COD amount, merchant commission, shipper contact details) and returns only operational fields (status, hub, next-leg ETA). This preserves the zero-loss handoff flow without leaking business data.
4. `tenant_admin` gets the unfiltered response.

### What this gives us

The AWB remains the single stable identifier for a parcel across its entire lifecycle, across partner handoffs, across tenant_admin oversight, across carrier integrations, and across customer tracking — which is exactly what a waybill must be.

---

## Addendum: Marketplace Discovery for Carrier Vehicles

### Rationale

Carriers and Partners own idle vehicles between scheduled jobs. Exposing that idle capacity to consumer bookings creates a new revenue stream for Alliance members and converts the Tenant from "logistics software" to "logistics marketplace". A consumer (individual or small business) with goods to move submits pickup/dropoff, cargo weight, and volume; the platform matches against available vehicles whose size class, capacity, service area, and price fit the request.

### Participant model — no new scope tier

Rather than inventing a fourth scope, we extend the existing Partner and Merchant tiers with **type discriminators**:

| Role | Tier | Discriminator |
|------|------|---------------|
| Alliance Partner (full fleet operator) | Partner | `partner_type = 'alliance'` |
| Marketplace Partner (vehicle-listing only; formerly a non-member Carrier that opts in) | Partner | `partner_type = 'marketplace'` |
| Non-member Carrier (integration endpoint, no login) | Carrier | unchanged — still separate `carrier.carriers` table |
| Business Merchant (regular shipper) | Merchant | `merchant_type = 'business'` |
| Consumer (ad-hoc marketplace booker) | Merchant | `merchant_type = 'consumer'` |

**One RLS policy, one JWT claim set.** A Marketplace Partner logs in under `scope=partner` and sees only their own listings and bookings — same policy that isolates an Alliance Partner's drivers. A Consumer logs in under `scope=merchant` and sees only their own bookings — same policy that isolates a Business Merchant's invoices. The three-tier sovereignty model absorbs the marketplace without structural change.

Carriers that prefer to remain pure integration endpoints (API/EDI, no login, no listings) stay in `carrier.carriers` unchanged. "Carrier" becomes a role, not an exclusive entity: a given third-party delivery company may appear in both tables if they do both (integration handoffs *and* vehicle listings).

### Schema

```sql
-- Type discriminators on existing identity tables
ALTER TABLE identity.partners
    ADD COLUMN partner_type TEXT NOT NULL DEFAULT 'alliance'
    CHECK (partner_type IN ('alliance', 'marketplace'));

ALTER TABLE identity.merchants
    ADD COLUMN merchant_type TEXT NOT NULL DEFAULT 'business'
    CHECK (merchant_type IN ('business', 'consumer'));

-- New schema for marketplace
CREATE SCHEMA IF NOT EXISTS marketplace;

CREATE TABLE marketplace.vehicle_listings (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id               UUID NOT NULL REFERENCES identity.tenants(id),
    partner_id              UUID NOT NULL REFERENCES identity.partners(id),
    vehicle_id              UUID NOT NULL,                                  -- fleet.vehicles
    size_class              TEXT NOT NULL CHECK (size_class IN (
                                'motorcycle','sedan','van','l300',
                                '6wheeler','10wheeler','trailer')),
    max_weight_kg           NUMERIC(10,2) NOT NULL,
    max_volume_m3           NUMERIC(10,2),
    base_price_cents        BIGINT NOT NULL,
    per_km_cents            BIGINT NOT NULL,
    per_kg_cents            BIGINT,
    service_area            GEOGRAPHY(POLYGON, 4326),                       -- PostGIS
    idle_from               TIMESTAMPTZ NOT NULL,
    idle_until              TIMESTAMPTZ NOT NULL,
    status                  TEXT NOT NULL CHECK (status IN (
                                'active','paused','booked','expired')),
    carrier_response_window INTERVAL NOT NULL DEFAULT '15 minutes',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_listings_tenant_partner ON marketplace.vehicle_listings (tenant_id, partner_id);
CREATE INDEX idx_listings_idle_window    ON marketplace.vehicle_listings (tenant_id, status, idle_from, idle_until);
CREATE INDEX idx_listings_service_area   ON marketplace.vehicle_listings USING GIST (service_area);

CREATE TABLE marketplace.bookings (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id          UUID NOT NULL REFERENCES identity.tenants(id),
    listing_id         UUID NOT NULL REFERENCES marketplace.vehicle_listings(id),
    partner_id         UUID NOT NULL,                     -- denormalized from listing (RLS)
    merchant_id        UUID NOT NULL,                     -- consumer merchant (RLS)
    shipment_id        UUID NOT NULL UNIQUE,              -- 1:1 with order_intake.shipments
    pickup_at          TIMESTAMPTZ NOT NULL,
    pickup_point       GEOGRAPHY(POINT, 4326) NOT NULL,
    dropoff_point      GEOGRAPHY(POINT, 4326) NOT NULL,
    cargo_weight_kg    NUMERIC(10,2) NOT NULL,
    cargo_volume_m3    NUMERIC(10,2),
    quoted_price_cents BIGINT NOT NULL,
    status             TEXT NOT NULL CHECK (status IN (
                           'pending','accepted','rejected','in_transit','delivered','cancelled','disputed')),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_bookings_tenant_partner  ON marketplace.bookings (tenant_id, partner_id, status);
CREATE INDEX idx_bookings_tenant_merchant ON marketplace.bookings (tenant_id, merchant_id);
```

Both tables receive the same three-tier RLS policy shape defined above, with one augmentation: `vehicle_listings` has a **public-discovery read path** that runs under `scope=tenant` via a dedicated service endpoint; raw table access remains partner-scoped. Public-discovery reads return a projected `PublicListing` DTO (display name, size class, quoted-price estimator, coarse service-area polygon) — cost basis, driver identity, and vehicle plate are stripped at the type layer.

### Matching algorithm

Consumer submits a booking intent:
`{ pickup_point, dropoff_point, cargo_weight_kg, cargo_volume_m3, pickup_at, size_class_preference? }`

The matcher (executed in `dispatch` service, reusing existing PostGIS infrastructure):

```
1. Tenant filter         — listings are surfaced only within the consumer's Alliance.
2. Availability filter   — idle_from <= pickup_at <= idle_until AND status = 'active'.
3. Capacity filter       — max_weight_kg >= cargo_weight_kg
                            AND (max_volume_m3 IS NULL OR max_volume_m3 >= cargo_volume_m3).
4. Size class filter     — exact match if consumer specified; else ranked preference.
5. Service-area filter   — ST_Covers(service_area, pickup_point)
                            AND ST_Covers(service_area, dropoff_point).
6. Price compute         — quoted = base_price_cents
                            + per_km_cents * ST_Distance(pickup, dropoff)/1000
                            + COALESCE(per_kg_cents,0) * cargo_weight_kg.
7. Rank                  — primary: distance from carrier's last known position to pickup_point
                            secondary: quoted price ASC
                            tertiary:  carrier rating DESC.
8. Return top N (default 10) as PublicListing DTOs.
```

Distance uses PostGIS `geography` (accurate over long hauls). The vehicle's last-known position reuses the `driver_latest_locations` view pattern already built for dispatch proximity.

### Booking flow — every booking is a first-class shipment

```
Consumer (scope=merchant) → POST /v1/marketplace/bookings
  │
  ├─ order-intake mints a shipment row
  │     - AWB generated via existing CM-{TTT}-{S}{NNNNNNN}{C} generator
  │     - partner_id = listing.partner_id
  │     - merchant_id = consumer's merchant_id
  │     - service code 'S' (Standard) or 'E' (Express) based on pickup_at delta
  │
  ├─ marketplace.bookings row created (partner_id + merchant_id denormalized for RLS)
  │
  ├─ listing.status → 'booked' atomically in the same tx (row lock)
  │
  └─ Kafka outbox: marketplace.booking_created
         → Carrier notified via partner portal + mobile push
         → Consumer notified via customer app + SMS + AWB link
         
Carrier (scope=partner) responds within carrier_response_window:
  accept  → booking.status='accepted', shipment enters dispatch flow,
            driver = listing.vehicle's assigned driver, POD flow as usual
  reject  → booking.status='rejected', listing.status='active',
            matcher auto-runs fallback against remaining listings
  timeout → treated as reject (see above)
```

The booking-to-shipment path reuses order-intake end-to-end. There is **no parallel marketplace shipment pipeline, no parallel AWB generator, no parallel POD path**. This preserves the zero-loss invariant: a marketplace booking IS a shipment, and every guarantee that applies to shipments (outbox-atomic writes, RLS enforcement, handoff safety) applies to marketplace bookings by construction.

### Tracking Number — still unchanged

A marketplace booking produces a shipment row identical in shape to a merchant-booked shipment. The AWB is issued by the same tenant-scoped generator using `CM-{TTT}-{S}{NNNNNNN}{C}`. Service code `S` (Standard) or `E` (Express) applies by default based on urgency; no new service code needed. A future ADR may introduce code `M` for marketplace-sourced analytics disambiguation, but the format itself is stable — labels printed before the marketplace launch remain scan-valid, labels printed for marketplace bookings remain scan-valid if the booking is later re-handed-off to an Alliance Partner.

### Non-negotiable constraints for Marketplace

- **No marketplace booking bypasses order-intake.** Every booking mints a proper shipment.
- **Matching is tenant-scoped.** Consumers never see cross-tenant listings. Cross-Alliance federation is a future ADR.
- **Carriers see only their own bookings.** RLS partner-scope isolation applies unchanged.
- **Consumers see only their own bookings.** RLS merchant-scope isolation applies unchanged.
- **Public discovery returns only projected DTOs.** Internal fields (cost basis, driver, plate) are never serialized to consumer responses.

---

## Risk Register and Mitigations

The zero-loss constraint requires each risk below to have a concrete, testable mitigation landed before the enforcement flip.

| # | Risk | Likelihood | Blast radius | Mitigation |
|---|------|------------|--------------|------------|
| R1 | **Backfill attribution error.** Auto-assigning legacy rows to a real Partner creates silent cross-partner ownership of historical data. | High during pilot (already two partners per tenant) | Shipments invisible to their true operator; commissions paid to wrong Partner. | **Legacy Partner sink** (`is_legacy_sink=TRUE`). No real Partner receives legacy rows unless explicitly reassigned via the audited `reassign_legacy_rows` job. Tenant-admin and every pre-existing membership retains read access. |
| R2 | **Enforcement flip silently drops rows.** A background consumer or cron that doesn't set GUCs succeeds with zero-row SELECTs and silently no-op INSERTs. Tasks fail to be created; outbound events fail to be emitted. | Near-certain without gating. | Catastrophic — physical parcels without digital state. | **Fail-closed policy** (explicit `IS NOT NULL` checks in the predicate). **Service-role audit**: every consumer / cron / drainer / migration must declare whether it runs as `logisticos_service` (BYPASSRLS) or `logisticos_app` with an explicit `ScopedTx::begin`. Gating the enforcement flip requires the audit doc signed off. CI test: start a pool with no GUC, assert every read returns 0 rows and every write errors — proves fail-closed. |
| R3 | **Handoff atomicity failure.** A bare UPDATE flips `partner_id A→B`, then Kafka publish fails. A no longer sees it; B never learns. | Medium (occurs under network partition). | Single shipment orphaned per event loss. | **Two-phase handoff with `pending_partner_id`.** State machine guarantees at least one partner always has RLS visibility. **Outbox-atomic commit**: state transition and event emission in one tx. At-least-once delivery with dedupe by event id. |
| R4 | **Tri-predicate policy bug.** A missing `OR` branch on `shipments` makes multi-scope rows invisible to everyone except tenant_admin. | Medium during rollout. | Merchant tracking pages empty; customers assume parcel lost. | **14-day shadow-mode** (not 7): policies exist, `logisticos_app` retains BYPASSRLS, pgaudit logs what *would* have been blocked. **Diff report** by (tenant, partner, merchant, endpoint) compares would-be-blocked to actually-returned. **Cutover gate**: 72h of zero-diff required. |
| R5 | **GRANT window expiry mid-dispute.** Default 30-day prior-partner visibility expires before a dispute is investigated. | Medium (disputes often land day 45+). | Audit loss, not physical loss. Still unacceptable for COD disputes. | **Default 180 days**, configurable per tenant. **Tenant-scope retains permanent historical visibility** — tenant_admin always sees all past partner states. `shipment_scope_grants` is append-only; rows are archived, never deleted. |
| R6 | **Context-switch abuse.** A compromised JWT or a stale `pid` claim causes a user to see another Partner's data. | Low. | Data leak across partners. | Membership re-verification on every `/switch-context` call. Rate limit 10/min/user. Audit every switch to `identity.scope_switch_audit`. Quarterly pen-test adds partner↔partner and merchant↔merchant crossing probes. |
| R7 | **AI agent crosses scope.** An LLM-driven MCP tool call receives a shipment id from a different partner and acts on it. | Low at boundary; medium across many tools. | Action on wrong parcel (e.g. reassigning a driver belonging to Partner B). | Scope is injected server-side from JWT, *never* from tool input. Every MCP tool server applies `ScopedTx::begin` before handling the call. Per-tool unit test asserts cross-scope calls return `403 scope_violation`. |
| R8 | **Missed GUC set under high concurrency.** A pgbouncer session-pool reuse leaks GUC values between requests. | High under pgbouncer transaction mode without `SET LOCAL` discipline. | Cross-tenant/cross-partner leak. | **All GUCs use `SET LOCAL`** (tx-scoped, auto-reset on COMMIT/ROLLBACK). **pgbouncer mode fixed at `transaction`**; session mode is banned in prod config. **CI check**: static analysis fails any `SET app.current_*` without `LOCAL`. |
| R9 | **Service-by-service migration races.** Some services are enforcing, others aren't; a row written by a non-enforcing service with `partner_id=NULL` poisons the enforcing service's joins. | Medium. | Join returns no rows; task creation fails; parcel state stranded. | **`partner_id` added as `NOT NULL` with default = Legacy Partner** in shadow phase, before any service flips. Every insert path explicitly sets `partner_id` before the flip. **Service-by-service canary order**: driver-ops first (shallowest), order-intake / shipments last (deepest). |
| R10 | **AWB lookup breaks post-handoff.** Partner B receives a parcel and scans it before the `shipment.handoff_accepted` event is consumed by every service. | Medium during operations. | Scan rejected; parcel stuck at hub. | AWB lookup elevates to `scope=tenant` with audit log (see *Tracking Number Impact*). Lookup does not depend on scope resolution for resolvability — only for field filtering. |
| R11 | **Marketplace listing-field leak.** Discovery endpoint accidentally returns carrier cost basis, driver identity, or vehicle plate to a consumer browse request. | Medium at first launch. | Commercial data exposure; competitive harm to carriers. | Discovery endpoint returns a projected `PublicListing` DTO. Internal struct has no `#[derive(Serialize)]`; only the DTO does. Contract test: snapshot the DTO shape and fail CI on any new public field unless explicitly whitelisted. |
| R12 | **Consumer PII leaks to wrong carrier.** Carrier A sees consumer phone/address for a booking actually routed to Carrier B due to a matcher bug. | Low structurally, high if RLS misconfigured on `bookings`. | PII breach; PDPA/GDPR exposure. | `marketplace.bookings` runs under the standard three-tier RLS policy with `partner_id` match required. Consumer contact fields are encrypted-at-rest and decrypted only when the requesting partner matches the booking's `partner_id`. Quarterly pen-test adds a marketplace cross-partner probe. |
| R13 | **Size/weight mismatch at pickup.** Consumer underreports cargo; vehicle can't carry. Parcel stranded mid-marketplace. | Medium operationally. | Single booking stuck; consumer experience broken. Not shipment-lost in the RLS sense — the shipment row exists — but physically unfulfilled. | At booking, consumer attests to weight/volume with an explicit confirmation. At pickup, driver runs an in-app checklist; on mismatch, booking → `disputed` state and the matcher re-runs against remaining idle listings in the area. The original shipment row remains visible to tenant_admin throughout; no row becomes orphaned. |
| R14 | **Carrier cancels an accepted booking to chase a higher bid.** | Medium at market launch. | Consumer experience degraded; shipment needs rematch. | Cancellation after acceptance is penalty-gated (configurable, default 20% of quoted price, deducted from carrier payout). Cancellation automatically triggers the fallback matcher; consumer sees a seamless re-routing experience. Repeat-offender carriers have their `partner.status` auto-flipped to `suspended` and require tenant_admin reinstatement. |

All fourteen mitigations are deliverables, not commentary. Each has an acceptance test in the `testing strategy` section below.

---

## Alternatives Considered

| Alternative | Reason Rejected |
|-------------|----------------|
| **Unify Partners and Carriers into one table** | Different lifecycles, different ownership models, different UX. Unification leaks concerns in both directions. |
| **Per-partner PostgreSQL schema** | Same reasons ADR-0008 rejected per-tenant schemas — operational overhead, connection pool explosion, DDL sprawl across N partners × 17 services. |
| **Application-level partner filtering (no RLS)** | Would repeat the class of bug ADR-0008 exists to prevent. First missed `WHERE partner_id = ?` becomes a cross-partner data leak in production. |
| **Single scope per session, no switching** | A user legitimately works for multiple partners (dispatcher moonlighting for two fleets) or wears two hats (tenant_admin who owns one partner). Forcing re-login every time kills UX. |
| **Embed partner_id in tenant_id (composite key)** | Breaks every existing query, every Kafka event envelope, every MCP tool signature. Not backward compatible. |
| **Separate JWT per scope (no multiplexing)** | Users need simultaneous views (tenant_admin auditing Partner A's roster while Partner A is logged in). Multiplexing via `scope` claim + context-switch endpoint is cleaner. |
| **Partner segment in the AWB** | Forces label reprint on every handoff — physically impossible. Partner ownership is row-state, not parcel-identity. |
| **Auto-attribute legacy rows to one real Partner on backfill (R1 original)** | Silently grants that Partner commission and visibility rights on rows it did not fulfill. Legacy Partner sink preserves provenance. |
| **Bare UPDATE for handoffs with post-commit event (R3 original)** | Loses the event on network partition and orphans the shipment. Transactional outbox + two-phase state is the only way to honor zero-loss. |

---

## Consequences

### Positive

- **Scope isolation at the database layer.** A Partner cannot read another Partner's drivers even with a bug in application code. RLS provides the same guarantee for the new tier that it already does for tenants.
- **Zero-loss guaranteed by construction.** Two-phase handoffs + outbox atomicity + fail-closed policy + legacy sink means there is no state in which a shipment row is invisible to its legitimate operator.
- **UX matches reality.** Partners log into the partner portal and see *their* fleet. Merchants (future) see *their* shipments. No more "hide it in the frontend" anti-patterns.
- **Context switching enables power users.** A tenant_admin can drop into Partner A's view to debug a dispatch issue, then switch back — with a clear audit trail of which scope each action was taken in.
- **AI agents are scope-safe by construction.** MCP tool enforcement means an LLM cannot accidentally (or adversarially) cross partner boundaries.
- **Backward compatible at the event layer.** Kafka event envelopes gain `partner_id` / `merchant_id` as optional fields. Existing consumers keep working; new consumers opt into scope-aware behavior.
- **AWB remains stable.** Every label printed before this ADR remains valid. Every label printed after this ADR survives arbitrary partner handoffs.
- **Opens the Alliance platform model.** Third-party Partners can be onboarded in minutes, not architected-around. This is the foundation for the CargoMarket multi-product vision.
- **Marketplace Discovery as a natural extension.** Carriers with idle vehicles and consumers with one-off cargo become first-class Alliance participants without a new scope tier. Every marketplace booking is a proper shipment, protected by the same zero-loss guarantees as any other shipment.

### Negative

- **Migration load.** ~25 tables across 8 services need `partner_id` / `merchant_id` columns + backfill. Backfill uses the Legacy Partner sink. Staged over 3 sprints (was 2) with feature flag `partner_scope_enforcement` per-service starting as shadow-read.
- **RLS policy complexity.** The three-scope predicate with grant-table EXISTS is more expensive to evaluate than a single-column match. Mitigated by composite indexes `(tenant_id, partner_id)` and `(tenant_id, merchant_id)` on every scoped table, and `(shipment_id, grantee_partner_id) WHERE expires_at > now()` on `shipment_scope_grants`. Benchmark target: < 1ms overhead per query at P99.
- **Four session GUCs per transaction.** `SET LOCAL app.current_tenant_id`, `app.current_scope`, plus optional `app.current_partner_id` / `current_merchant_id`. Measured overhead: ~0.4ms added per transaction start. Acceptable within the 200ms P99 budget.
- **JWT size grows.** `pid` / `mid` / `scope` claims add ~80 bytes. Negligible.
- **Context-switch endpoint is a new attack surface.** Requires careful rate-limiting, membership re-verification on each switch, and audit logging of every switch. Security QA adds cross-scope access tests to the quarterly pen test scope.
- **AI tool plumbing changes.** Every MCP tool signature gains a scope context. One-time refactor, then uniform. No LLM prompt changes — the scope is injected at the server, invisible to the model.
- **AWB lookup elevation path.** Two code paths for shipment resolution (`by-id + scope` vs `by-awb + tenant-elevated`). The latter needs field-filtering logic kept in sync with commercial-data exposure policy. Single responsibility: `ShipmentQueryService::resolve_awb` is the *only* entry point; duplicated implementations are banned.

---

## Rollout Plan

| Week | Step | Gate to next step |
|------|------|------------------|
| 1 | **Schema.** Land `identity.partners`, `identity.merchants`, `partner_memberships`, `merchant_memberships`, `shipment_scope_grants`, `scope_switch_audit`, `awb_lookup_audit` migrations. Seed one **Legacy Partner** and one **Legacy Merchant** per tenant. Seed `partner_memberships` for every existing partner portal user into the Legacy Partner. No RLS changes. | Migration succeeds on staging mirror of production data. |
| 2 | **Claims.** Extend JWT issuance with `pid` / `mid` / `scope`. Ship `POST /v1/auth/switch-context`. Claims optional; absence = tenant scope. | All existing sessions continue to work; switch endpoint passes cross-scope pen test. |
| 3a | **Columns.** Add `partner_id` / `merchant_id` / `pending_partner_id` columns to target tables, backfilled to Legacy Partner / Legacy Merchant. `NOT NULL` enforced with Legacy as default. | Every insert path in every service has been audited to set `partner_id` explicitly. |
| 3b | **Shadow RLS (14 days).** Add RLS policies in log-only mode (`logisticos_app` retains BYPASSRLS, pgaudit logs what would have been blocked). Ship the would-be-blocked diff report. | 72 consecutive hours of zero-diff across all services and endpoints. |
| 3c | **Service-role audit.** Every Kafka consumer, cron, drainer, migration, backfill job proven to run under `logisticos_service` with BYPASSRLS, or under `logisticos_app` with explicit `ScopedTx::begin`. | Audit document signed off by Principal Architect, Database Reliability Engineer, and CISO. |
| 4 | **Enforce, service-by-service, 48h per service.** Canary order: driver-ops → fleet → hub-ops → payments → dispatch → pod → order-intake → shipments. Each service's BYPASSRLS is removed only after 48h of zero-incident on the prior service. A single shipment-invisibility incident rolls back the flag and blocks progression until root cause is eliminated. | Zero shipment-invisibility incidents across all eight services. |
| 5 | **UX.** Partner portal honors partner scope. Admin portal gains a scope switcher UI. | Partner portal pages show only scope-matched rows in regression tests across two-partner test tenant. |
| 6 | **MCP.** All MCP tool servers validate scope context on entry. AI Intelligence Layer propagates active scope from the invoking user's session. | Per-tool unit test: cross-scope invocation returns `403 scope_violation`. |
| 7 | **Cross-partner handoffs.** Ship `shipment.handoffs` Kafka topic, two-phase `pending_partner_id` state machine, outbox-atomic commit path, and `shipment_scope_grants` (180-day default). | End-to-end chaos test: handoff with induced Kafka outage reconverges without shipment loss. |

Feature flag: `partner_scope_enforcement` (Unleash) — service-scoped, flipped individually per service at step 4. Rollback path is a single flag flip per service; flipping back re-enables BYPASSRLS in under 10s.

---

## Testing Strategy

- **Unit**: JWT claim extraction; `SessionScope` serde; RLS predicate construction; Luhn checksum unchanged; AWB lookup service field-filtering.
- **Integration**: Per service, spin a test database with two tenants × two partners × two merchants. For every repository method, run:
  - `scope=tenant` (tenant_admin) — should see all rows in tenant.
  - `scope=partner(A)` — should see only Partner A's rows.
  - `scope=partner(B)` — should see only Partner B's rows.
  - Cross-tenant — should see zero rows regardless of scope.
  - `scope=partner(A)` on a shipment currently at `partner_id=B, pending_partner_id=A` — should see the shipment (pending visibility).
  - `scope=partner(A)` on a shipment handed off A→B with active 180-day grant — should see the shipment (grant visibility).
- **Fail-closed assertion**: start a pool that never calls `ScopedTx::begin`. Assert every SELECT returns 0 rows and every INSERT errors with RLS violation. Mitigation R2/R8.
- **Handoff atomicity chaos test**: induce Kafka broker outage between handoff-COMMIT and outbox-drain. Assert shipment state reconverges and no shipment is invisible to both partners at any instant. Mitigation R3.
- **Backfill correctness**: post-backfill, per table, count rows per `(tenant_id, partner_id)`. Assert sum equals total and Legacy sink contains exactly the pre-migration count. Mitigation R1/R9.
- **Contract**: `scripts/db/check-rls-coverage.sh` extended to assert every new `partner_id` / `merchant_id` column has a corresponding RLS policy with `current_setting('app.current_scope', true) IS NOT NULL` in the predicate. Mitigation R4.
- **Shadow-diff**: the log-only diff report is itself a test artifact. Cutover gate blocks on 72h zero-diff. Mitigation R4.
- **Pen test**: Quarterly security review adds six new test cases: partner↔partner, merchant↔merchant, partner-sees-other-merchants-shipments-via-handoff-window, GUC leak across pgbouncer connection reuse, MCP tool cross-scope invocation, AWB lookup data leak via field filtering. Mitigations R6/R7/R8/R10.
- **Load**: Benchmark the four-predicate policy (tenant + scope + partner + grant-EXISTS) against the current single-predicate policy. Fail build if regression exceeds 1ms P99 on the `shipments` table hot path.

---

## Spec-First Deliverables (per CLAUDE.md)

Before implementation begins, these specs must land:

1. **OpenAPI 3.1** — `docs/api/openapi/identity.v1.yaml` adds `POST /v1/auth/switch-context`, `GET /v1/me/memberships`, `GET /v1/partners`, `POST /v1/partners`, `GET /v1/merchants`, `POST /v1/merchants`.
2. **OpenAPI 3.1** — `docs/api/openapi/order-intake.v1.yaml` adds `GET /v1/shipments/by-awb/:awb` with tenant-scope-elevation semantics and field-filtering spec.
3. **OpenAPI 3.1** — `docs/api/openapi/order-intake.v1.yaml` adds `POST /v1/shipments/:id/handoff/request`, `POST /v1/shipments/:id/handoff/accept`, `POST /v1/shipments/:id/handoff/reject`.
4. **Protobuf** — `libs/proto/logisticos/v1/scope.proto` defines `SessionScope`, `PartnerRef`, `MerchantRef` messages reused by every MCP tool contract.
5. **Event envelope update** — `libs/proto/logisticos/v1/events.proto` adds optional `partner_id` and `merchant_id` to `EventEnvelope`. New topic `shipment.handoffs` with `HandoffRequested`, `HandoffAccepted`, `HandoffRejected` payloads.
6. **MCP tool manifest** — each MCP server's published tool schema declares which tools are partner-scoped vs tenant-scoped vs merchant-scoped.
7. **Runbook** — `docs/runbooks/partner-scope-rollback.md` documents the per-service flag flip, BYPASSRLS restoration, and post-incident triage path.
8. **OpenAPI 3.1** — `docs/api/openapi/marketplace.v1.yaml` adds:
   - `GET /v1/marketplace/discovery` (consumer browse, returns `PublicListing[]`)
   - `POST /v1/marketplace/listings` / `PATCH /v1/marketplace/listings/:id` / `DELETE` (partner scope)
   - `POST /v1/marketplace/bookings` (consumer merchant scope, mints shipment + AWB)
   - `POST /v1/marketplace/bookings/:id/accept` / `:id/reject` (partner scope)
9. **MCP tool manifest** — `dispatch.match_marketplace_listings` declared as `scope=tenant`-or-`scope=merchant` (consumer-facing matcher); `marketplace.accept_booking` declared as `scope=partner`.

---

## Related ADRs

- [ADR-0003](0003-row-level-security-for-multi-tenancy.md) — original RLS introduction.
- [ADR-0004](0004-mcp-for-ai-interoperability.md) — MCP tool contracts; this ADR extends tool signatures with scope context.
- [ADR-0005](0005-hexagonal-architecture-for-microservices.md) — scope flows through application commands the same way tenant_id does; RLS remains an infrastructure concern.
- [ADR-0006](0006-kafka-event-streaming-topology.md) — event envelopes gain optional `partner_id` / `merchant_id`; new topic `shipment.handoffs`.
- [ADR-0008](0008-multi-tenancy-rls-strategy.md) — this ADR extends the RLS strategy with additional scope tiers; the session-GUC mechanism and `logisticos_app` / `logisticos_service` role split are reused unchanged.
- [ADR-0009](0009-multi-product-platform-gateway-topology.md) — the Alliance model generalizes to all six CargoMarket products; ADR-0014 (future) will define the cross-product `operating_unit` generalization.
- [ADR-0010](0010-operator-agent-collaboration-model.md) — agent operators gain scope; a Partner's operator agent cannot act outside Partner scope.
- [ADR-0011](0011-firebase-logisticos-jwt-bridge.md) — the LoS JWT exchange mints the extended claims; the Firebase→LoS bridge gains a `default_scope` hint for post-login routing.
- **Tracking Number Architecture** (implementation doc) — AWB format `CM-{TTT}-{S}{NNNNNNN}{C}` is explicitly unchanged by this ADR; lookup path is extended with tenant-scope elevation and audit.
