# Order → Dispatch → POD Flow — Design Spec

**Date:** 2026-03-26
**Status:** Approved
**Scope:** Wire the core last-mile delivery journey end-to-end using real Rust services, PostgreSQL, Redis, and Kafka. Includes real JWT authentication across all portals.

---

## Goals

- Merchant creates a shipment → stored in DB, AWB generated
- Admin sees the shipment queue → assigns to a driver
- Driver receives the task → advances through status states → captures POD
- Customer tracks the shipment in real time via public tracking page
- All portals authenticate with real JWT tokens issued by the identity service

## Out of Scope (this phase)

- Auto-dispatch / AI routing (manual assignment only)
- Notifications (WhatsApp/SMS)
- COD reconciliation and billing
- Failed delivery retry workflows
- Multi-hub routing
- Production Kubernetes deployment

---

## Architecture

### Approach

Sequential service wiring in dependency order. Each service is fully working and tested before the next one is built. Docker Compose provides infrastructure only (Postgres, Redis, Kafka). Rust services run natively on the host via `cargo run` for fast iteration.

### Infrastructure (Docker Compose)

File: `infra/docker-compose.dev.yml`

| Service | Image | Port |
|---------|-------|------|
| PostgreSQL 16 | `postgres:16-alpine` | 5432 |
| Redis 7 | `redis:7-alpine` | 6379 |
| Kafka | `confluentinc/cp-kafka:7.6` | 9092 (internal), 29092 (host) |
| Zookeeper | `confluentinc/cp-zookeeper:7.6` | 2181 |

One PostgreSQL database per service, all on the same instance:

- `logisticos_identity`
- `logisticos_order_intake`
- `logisticos_dispatch`
- `logisticos_driver_ops`
- `logisticos_pod`
- `logisticos_delivery_experience`

An init script (`infra/postgres/init.sql`) creates all databases, seed tenant, seed users, and one seed shipment on first `docker compose up`.

### Kafka Topics

All events flow through a single topic per event type. Auto-create is disabled; topics are created by the Docker Compose init script using `kafka-topics.sh`.

| Topic | Partitions | Retention | Producer | Consumer(s) |
|-------|-----------|-----------|----------|-------------|
| `logisticos.shipment.created` | 3 | 7 days | order-intake | dispatch, delivery-exp |
| `logisticos.shipment.cancelled` | 3 | 7 days | order-intake | dispatch, delivery-exp |
| `logisticos.shipment.status_updated` | 3 | 7 days | order-intake | — (merchant portal polls) |
| `logisticos.task.assigned` | 3 | 7 days | dispatch | driver-ops, delivery-exp |
| `logisticos.task.status_changed` | 3 | 7 days | driver-ops | delivery-exp |
| `logisticos.task.completed` | 3 | 7 days | driver-ops | pod, delivery-exp |
| `logisticos.pod.captured` | 3 | 7 days | pod | delivery-exp |
| `logisticos.user.created` | 3 | 7 days | identity | dispatch |

All Kafka messages share this envelope:
```json
{
  "event_type": "string",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": { }
}
```

### Service Ports

| Service | Port |
|---------|------|
| Identity | 8000 |
| Order-intake | 8001 |
| Dispatch | 8002 |
| Driver-ops | 8003 |
| POD | 8004 |
| Delivery-experience | 8005 |
| Compliance (existing) | 8006 |

### Frontend Ports

| App | Port |
|-----|------|
| Admin portal (Next.js dev) | 3001 |
| Merchant portal (Next.js dev) | 3002 |
| Partner portal (Next.js dev) | 3003 |
| Customer portal (Next.js dev) | 3004 |
| Driver app (Expo web export) | 8083 |

---

## Identity Service (`services/identity`, port 8000)

### JWT Strategy

Two-token pattern:

- **Access token** — 15 min TTL, HS256, validated locally by every downstream service using the shared `JWT_SECRET` env var. No network call required for validation.
- **Refresh token** — 7 day TTL, stored hashed in `refresh_tokens` table, rotated on every use (old token revoked, new token issued).

### JWT Claims

```json
{
  "sub": "user_uuid",
  "tenant_id": "tenant_uuid",
  "role": "tenant_admin | merchant | driver | partner",
  "name": "string",
  "exp": 1234567890,
  "iat": 1234567890
}
```

`name` is included in the JWT so downstream services (e.g. dispatch) can denormalize `driver_name` from the token without querying identity.

### UUID Namespace Contract

**`driver_id` throughout the entire system is always `users.id` from the identity service.** There is no separate driver profile UUID. When dispatch assigns a task, the `driver_id` in the assignment is the identity `users.id` of the driver user. When driver-ops filters tasks by `WHERE driver_id = ?`, the value is the JWT `sub` claim.

### Roles

| Role | Permissions |
|------|-------------|
| `tenant_admin` | Full ops access: dispatch queue, driver assignment, all shipments |
| `merchant` | Create and view own shipments only |
| `driver` | View own tasks, advance task status, submit POD |
| `partner` | View driver roster and compliance status |

### API Endpoints

```
POST /api/v1/auth/login     body: { email, password }
                            → { access_token, refresh_token, user: { id, role, tenant_id, name } }

POST /api/v1/auth/refresh   body: { refresh_token }
                            → { access_token }

POST /api/v1/auth/logout    body: { refresh_token } → 204

GET  /api/v1/auth/me        → { id, email, role, tenant_id, name }

POST /api/v1/users          tenant_admin only
                            body: { email, password, role, name }
                            → { id, email, role, name }
                            side-effect: emits logisticos.user.created

GET  /api/v1/users          tenant_admin only
                            query: ?role=driver|merchant|...
                            → [{ id, email, role, name }]
```

### Kafka Event — `logisticos.user.created`

Emitted after `POST /api/v1/users` succeeds:

```json
{
  "event_type": "user.created",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "user_id": "uuid",
    "name": "string",
    "role": "string",
    "email": "string"
  }
}
```

### Database Schema

```sql
-- tenants
id          UUID PRIMARY KEY DEFAULT gen_random_uuid()
name        TEXT NOT NULL
slug        TEXT NOT NULL UNIQUE
plan        TEXT NOT NULL DEFAULT 'starter'
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()

-- users
id             UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id      UUID NOT NULL REFERENCES tenants(id)
email          TEXT NOT NULL UNIQUE
password_hash  TEXT NOT NULL
role           TEXT NOT NULL   -- tenant_admin | merchant | driver | partner
name           TEXT NOT NULL
is_active      BOOLEAN NOT NULL DEFAULT true
created_at     TIMESTAMPTZ NOT NULL DEFAULT now()

-- refresh_tokens
id          UUID PRIMARY KEY DEFAULT gen_random_uuid()
user_id     UUID NOT NULL REFERENCES users(id)
token_hash  TEXT NOT NULL UNIQUE
expires_at  TIMESTAMPTZ NOT NULL
revoked_at  TIMESTAMPTZ
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
```

### Seed Data

The Docker init script seeds one tenant and four users plus one seed driver profile in dispatch (see Dispatch seed below) so all portals can log in immediately on first `docker compose up`.

| Email | Password | Role | UUID (fixed for seed) |
|-------|----------|------|----------------------|
| `admin@demo.logisticos.dev` | `Demo1234!` | `tenant_admin` | `00000000-0000-0000-0000-000000000001` |
| `merchant@demo.logisticos.dev` | `Demo1234!` | `merchant` | `00000000-0000-0000-0000-000000000002` |
| `driver@demo.logisticos.dev` | `Demo1234!` | `driver` | `00000000-0000-0000-0000-000000000003` |
| `partner@demo.logisticos.dev` | `Demo1234!` | `partner` | `00000000-0000-0000-0000-000000000004` |

Fixed UUIDs are used so the seed shipment and seed task (below) can reference them by ID without dynamic lookups.

---

## Order-Intake Service (`services/order-intake`, port 8001)

### API Endpoints

```
POST /api/v1/shipments      merchant role
                            body: { recipient_name, recipient_phone, recipient_address,
                                    origin_city, destination_city, cod_amount?, weight_kg?, notes? }
                            → { id, tracking_number, status: "pending" }

GET  /api/v1/shipments      merchant: own shipments | tenant_admin: all
                            query: ?status=&page=&limit=
                            → { data: [shipment], total, page }

GET  /api/v1/shipments/:id  → shipment detail

PATCH /api/v1/shipments/:id/cancel
                            merchant | tenant_admin → 200
                            side-effect: emits logisticos.shipment.cancelled
```

### AWB Generation

Format: `LS-{8 uppercase alphanumeric}` — e.g. `LS-A1B2C3D4`.
Generated using a cryptographically random 8-char string in Rust, checked for uniqueness against the DB with a retry on collision.

### Shipment Status Lifecycle

Order-intake owns the canonical shipment status. Status is updated by consuming downstream Kafka events:

| Event consumed | Status transition |
|----------------|-------------------|
| — (on create) | `pending` |
| `task.assigned` | `assigned` |
| `task.status_changed` (navigating) | `out_for_delivery` |
| `task.completed` | `delivered` |
| `task.status_changed` (failed) | `failed` |
| `shipment.cancelled` (self) | `cancelled` |

The merchant portal polls `GET /api/v1/shipments` at 30s and will see live status updates.

### Kafka Events

**`logisticos.shipment.created`** — emitted after successful DB insert:

```json
{
  "event_type": "shipment.created",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX",
    "merchant_id": "uuid",
    "recipient_name": "string",
    "recipient_address": "string",
    "origin_city": "string",
    "destination_city": "string",
    "cod_amount": 0.0,
    "weight_kg": 0.0
  }
}
```

**`logisticos.shipment.cancelled`** — emitted on `PATCH /cancel`:

```json
{
  "event_type": "shipment.cancelled",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX"
  }
}
```

### Database Schema

```sql
-- shipments
id                  UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id           UUID NOT NULL
merchant_id         UUID NOT NULL
tracking_number     TEXT NOT NULL UNIQUE
recipient_name      TEXT NOT NULL
recipient_phone     TEXT NOT NULL
recipient_address   TEXT NOT NULL
origin_city         TEXT NOT NULL
destination_city    TEXT NOT NULL
status              TEXT NOT NULL DEFAULT 'pending'
  -- pending | assigned | out_for_delivery | delivered | failed | cancelled
cod_amount          NUMERIC(12,2)
weight_kg           NUMERIC(8,3)
notes               TEXT
created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
updated_at          TIMESTAMPTZ NOT NULL DEFAULT now()
```

### Seed Data

One seed shipment pre-inserted so the customer tracking demo works on first boot:

```
tracking_number: LS-DEMO0001
merchant_id:     00000000-0000-0000-0000-000000000002  (seed merchant)
recipient_name:  Maria Santos
status:          pending
origin_city:     Manila
destination_city: Makati City
cod_amount:      1500.00
```

---

## Dispatch Service (`services/dispatch`, port 8002)

### Driver Data Source

Dispatch maintains its own `driver_profiles` table (a local cache of driver users), populated by consuming `logisticos.user.created` where `role = "driver"`. This avoids synchronous cross-service calls.

`driver_id` in all dispatch records is the identity `users.id` UUID — the same value as the JWT `sub` claim for driver tokens.

### API Endpoints

```
GET  /api/v1/dispatch/queue
     tenant_admin → [{ shipment_id, tracking_number, recipient_name,
                       destination_city, cod_amount, queued_at }]

GET  /api/v1/dispatch/drivers
     tenant_admin → [{ driver_id, name, compliance_status, is_available }]

POST /api/v1/dispatch/assign
     tenant_admin
     body: { shipment_id, driver_id }
     → { assignment_id }

GET  /api/v1/dispatch/assignments
     tenant_admin → [active assignments with driver + shipment info]
```

### Kafka Consumers

| Topic | Action |
|-------|--------|
| `logisticos.shipment.created` | Insert into `dispatch_queue` with `status = "pending"` |
| `logisticos.shipment.cancelled` | Update `dispatch_queue` entry to `status = "cancelled"` |
| `logisticos.user.created` (role=driver) | Upsert into `driver_profiles` |

### Assignment Logic

On `POST /api/v1/dispatch/assign`:
1. Check Redis compliance cache key `driver:{driver_id}:compliance` — must be `compliant` or `expiring_soon`. Cache is populated by the compliance service (port 8006, existing).
2. Resolve `driver_name` from `driver_profiles` table.
3. Create `assignments` record.
4. Update `dispatch_queue` entry to `status = "assigned"`.
5. Emit `logisticos.task.assigned`.

### Kafka Event — `logisticos.task.assigned`

```json
{
  "event_type": "task.assigned",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "assignment_id": "uuid",
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX",
    "driver_id": "uuid",
    "driver_name": "string",
    "recipient_name": "string",
    "recipient_address": "string",
    "origin_city": "string",
    "destination_city": "string",
    "cod_amount": 0.0
  }
}
```

(`origin_city` included for idempotent replay by delivery-experience.)

### Database Schema

```sql
-- driver_profiles  (local cache populated from logisticos.user.created)
id          UUID PRIMARY KEY   -- same as identity users.id
tenant_id   UUID NOT NULL
name        TEXT NOT NULL
email       TEXT NOT NULL
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()

-- dispatch_queue
id               UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id        UUID NOT NULL
shipment_id      UUID NOT NULL UNIQUE
tracking_number  TEXT NOT NULL
recipient_name   TEXT NOT NULL
origin_city      TEXT NOT NULL
destination_city TEXT NOT NULL
cod_amount       NUMERIC(12,2)
status           TEXT NOT NULL DEFAULT 'pending'
  -- pending | assigned | cancelled
queued_at        TIMESTAMPTZ NOT NULL DEFAULT now()

-- assignments
id            UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id     UUID NOT NULL
shipment_id   UUID NOT NULL
driver_id     UUID NOT NULL   -- identity users.id
driver_name   TEXT NOT NULL
assigned_at   TIMESTAMPTZ NOT NULL DEFAULT now()
completed_at  TIMESTAMPTZ
cancelled_at  TIMESTAMPTZ
```

### Seed Data

The seed driver profile must exist before the seed shipment can be assigned:

```
driver_profiles: id=00000000-0000-0000-0000-000000000003, name="Demo Driver"
dispatch_queue:  seed shipment LS-DEMO0001 inserted as pending
```

---

## Driver-ops Service (`services/driver-ops`, port 8003)

### API Endpoints

```
GET  /api/v1/driver/tasks
     driver → own tasks filtered by WHERE driver_id = JWT.sub

GET  /api/v1/driver/tasks/:id
     driver → task detail

POST /api/v1/driver/tasks/:id/start
     assigned → navigating

POST /api/v1/driver/tasks/:id/arrive
     navigating → arrived

POST /api/v1/driver/tasks/:id/capture-pod
     arrived → pod_pending

POST /api/v1/driver/tasks/:id/complete
     pod_pending → completed
     Pre-condition: POD service must confirm POD record exists for this task_id
     (synchronous GET :8004/api/v1/pod/:task_id before accepting the transition;
     returns 422 if no POD record found)

POST /api/v1/driver/tasks/:id/fail
     any state → failed
     body: { reason: string }
```

### Task State Machine

```
assigned ──────────────────────────────────────────────┐
    │                                                   │
    ▼                                                   │
navigating ─────────────────────────────────────────┐  │
    │                                               │  │
    ▼                                               ▼  ▼
arrived ──────────────────────────────────────► failed
    │
    ▼
pod_pending ──────────────────────────────────► failed
    │
    ▼
completed
```

Any state can transition to `failed` via `POST .../fail`. The `complete` transition requires a confirmed POD record (see pre-condition above).

### Kafka Consumer — `logisticos.task.assigned`

On receipt: insert into `tasks` with `status = "assigned"`.

### Kafka Events

**`logisticos.task.status_changed`** — emitted on every state transition:

```json
{
  "event_type": "task.status_changed",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "task_id": "uuid",
    "assignment_id": "uuid",
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX",
    "driver_id": "uuid",
    "driver_name": "string",
    "from_status": "string",
    "to_status": "string",
    "reason": "string | null"
  }
}
```

**`logisticos.task.completed`** — emitted when `to_status = "completed"` (in addition to `task.status_changed`):

```json
{
  "event_type": "task.completed",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "task_id": "uuid",
    "assignment_id": "uuid",
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX",
    "driver_id": "uuid",
    "driver_name": "string"
  }
}
```

### Database Schema

```sql
-- tasks
id                UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id         UUID NOT NULL
assignment_id     UUID NOT NULL
driver_id         UUID NOT NULL   -- identity users.id = JWT sub
shipment_id       UUID NOT NULL
tracking_number   TEXT NOT NULL
recipient_name    TEXT NOT NULL
recipient_address TEXT NOT NULL
cod_amount        NUMERIC(12,2)
status            TEXT NOT NULL DEFAULT 'assigned'
  -- assigned | navigating | arrived | pod_pending | completed | failed
sequence          INT NOT NULL DEFAULT 1
created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()

-- task_events
id           UUID PRIMARY KEY DEFAULT gen_random_uuid()
task_id      UUID NOT NULL REFERENCES tasks(id)
from_status  TEXT NOT NULL
to_status    TEXT NOT NULL
reason       TEXT
occurred_at  TIMESTAMPTZ NOT NULL DEFAULT now()
```

---

## POD Service (`services/pod`, port 8004)

### API Endpoints

```
POST /api/v1/pod/:task_id
     driver role
     body: { signature_data, photo_url?, cod_collected, recipient_name_signed }
     → { id, captured_at }
     tenant_id resolved from JWT claims (not from task lookup)

GET  /api/v1/pod/:task_id
     driver | tenant_admin → POD record
```

### `tenant_id` Resolution

The POD service never calls another service to resolve `tenant_id`. It reads `tenant_id` directly from the JWT `tenant_id` claim of the authenticated driver. This is the same value that was propagated through the entire Kafka chain.

### Kafka Consumer — `logisticos.task.completed`

On receipt: verify that a POD record exists for `task_id`. If no record found:
- Insert a `pod_alerts` record with `alert_type = "missing_pod"` and the full event payload
- Emit no further events (does not block the `task.completed` flow)
- Ops team can query `pod_alerts` to follow up

### Kafka Event — `logisticos.pod.captured`

Emitted after successful `POST /api/v1/pod/:task_id`:

```json
{
  "event_type": "pod.captured",
  "occurred_at": "ISO8601",
  "tenant_id": "uuid",
  "data": {
    "task_id": "uuid",
    "shipment_id": "uuid",
    "tracking_number": "LS-XXXXXXXX",
    "cod_collected": 0.0,
    "captured_at": "ISO8601"
  }
}
```

### Database Schema

```sql
-- proof_of_deliveries
id                    UUID PRIMARY KEY DEFAULT gen_random_uuid()
tenant_id             UUID NOT NULL
task_id               UUID NOT NULL UNIQUE
shipment_id           UUID NOT NULL
tracking_number       TEXT NOT NULL
signature_data        TEXT NOT NULL   -- base64 or S3 URI
photo_url             TEXT
cod_collected         NUMERIC(12,2) NOT NULL DEFAULT 0
recipient_name_signed TEXT NOT NULL
captured_at           TIMESTAMPTZ NOT NULL DEFAULT now()

-- pod_alerts
id          UUID PRIMARY KEY DEFAULT gen_random_uuid()
task_id     UUID NOT NULL
alert_type  TEXT NOT NULL   -- missing_pod
payload     JSONB NOT NULL
created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
```

---

## Delivery-Experience Service (`services/delivery-experience`, port 8005)

### API Endpoints

```
GET /v1/tracking/public/:tracking_number
    no auth required
    → { tracking_number, status, origin_city, destination_city,
        eta, driver_name, driver_lat, driver_lng,
        timeline: [{ status, description, location, occurred_at }] }

GET /api/v1/delivery/:tracking_number
    authenticated (any role) → same response + shipment_id
```

### Kafka Consumers

All consumers upsert — if a later event arrives before an earlier one, the upsert is safe.

| Topic | Action |
|-------|--------|
| `logisticos.shipment.created` | Upsert `tracking_timelines`; insert "Shipment Created" event |
| `logisticos.shipment.cancelled` | Update `current_status = "cancelled"`; insert "Shipment Cancelled" event |
| `logisticos.task.assigned` | Upsert `driver_name`; insert "Driver Assigned" event. Uses `origin_city` from payload (included for replay safety). |
| `logisticos.task.status_changed` | Insert status event with human-readable description mapped from `to_status` |
| `logisticos.task.completed` | Update `current_status = "delivered"`; insert "Delivered" event |
| `logisticos.pod.captured` | Insert "POD Captured" event |

### Status → Description Map

| `to_status` | Timeline description |
|-------------|---------------------|
| `navigating` | "Driver is on the way" |
| `arrived` | "Driver has arrived at destination" |
| `pod_pending` | "Delivery in progress" |
| `completed` | "Successfully delivered" |
| `failed` | "Delivery attempt failed" |

### Database Schema

```sql
-- tracking_timelines
id               UUID PRIMARY KEY DEFAULT gen_random_uuid()
tracking_number  TEXT NOT NULL UNIQUE
tenant_id        UUID NOT NULL
origin_city      TEXT NOT NULL
destination_city TEXT NOT NULL
current_status   TEXT NOT NULL DEFAULT 'pending'
driver_name      TEXT
driver_lat       DOUBLE PRECISION
driver_lng       DOUBLE PRECISION
eta              TEXT
updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()

-- tracking_events
id               UUID PRIMARY KEY DEFAULT gen_random_uuid()
tracking_number  TEXT NOT NULL
status           TEXT NOT NULL
description      TEXT NOT NULL
location         TEXT
occurred_at      TIMESTAMPTZ NOT NULL DEFAULT now()
```

### Seed Data

Pre-insert a tracking timeline for `LS-DEMO0001` so the customer portal shows a result immediately on first boot without requiring the merchant to create a shipment first.

All seed rows use `INSERT ... ON CONFLICT DO NOTHING` to prevent duplicate failures when Kafka consumers replay the seed `shipment.created` event against already-seeded rows.

```
tracking_timelines: LS-DEMO0001, status=pending, Manila → Makati City
tracking_events:    "Shipment Created", occurred_at = now()
```

---

## Portal Wiring

Each portal gets `src/lib/api/client.ts` — a thin wrapper handling:
- `Authorization: Bearer {token}` header injection
- Automatic token refresh on 401 using the stored refresh token
- Typed error normalization

### Merchant Portal
- Login → `POST :8000/api/v1/auth/login` (role: `merchant`)
- Shipments list → `GET :8001/api/v1/shipments` (replaces `MOCK_SHIPMENTS`)
- New shipment modal → `POST :8001/api/v1/shipments`
- Status polling: 30s via TanStack Query `refetchInterval`

### Admin Portal
- Login → `POST :8000/api/v1/auth/login` (role: `tenant_admin`)
- Dispatch queue → `GET :8002/api/v1/dispatch/queue`
- Available drivers → `GET :8002/api/v1/dispatch/drivers`
- Assign action → `POST :8002/api/v1/dispatch/assign`
- Map driver positions: 10s polling of `GET :8002/api/v1/dispatch/drivers`

### Driver App
- New login screen → `POST :8000/api/v1/auth/login` (role: `driver`)
- Token stored in Redux `auth` slice (existing) + `SecureStore` on native
- Task list → `GET :8003/api/v1/driver/tasks` (replaces Redux mock seed)
- Task transitions → `POST :8003/api/v1/driver/tasks/:id/{action}`
- POD submit → `POST :8004/api/v1/pod/:task_id`

### Customer Portal
- No login required
- Track → `GET :8005/v1/tracking/public/:tn` (already wired in `page.tsx`)

---

## Shared Environment

Each service reads from a `.env` file (or environment variables). Common keys:

```env
DATABASE_URL=postgres://logisticos:logisticos@localhost:5432/logisticos_{service}
REDIS_URL=redis://localhost:6379
KAFKA_BROKERS=localhost:29092
JWT_SECRET=dev-secret-change-in-production
SERVICE_PORT=800x
```

---

## Implementation Order

1. Docker Compose + init SQL + topic creation script (infra)
2. Identity service — auth, JWT, seed users, `user.created` event
3. Order-intake service + merchant portal wiring
4. Dispatch service + admin portal wiring
5. Driver-ops service + driver app task wiring
6. POD service + driver app POD wiring
7. Delivery-experience service + customer portal live
