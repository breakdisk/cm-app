# Runbook: Cross-Border Hub Transfer — Deploy & Rollback

**Release:** `119bab2` (master) · `feat/hub-ops/cross-border-transfer` merged `930aff5`  
**Services affected:** `hub-ops` · `dispatch` · `carrier` · `payments` · `engagement` · `order-intake` · `admin-portal`  
**Risk level:** Low — additive migrations only; no breaking DDL; no existing-flow changes  
**Drafted:** 2026-06-03

---

## Pre-Deploy Checklist

### Build & CI
- [ ] GH Actions green on `master` after merge  
  ```sh
  gh run list --branch master --limit 5
  ```
- [ ] All 6 service images pushed to GHCR with `119bab2` SHA (or current CI tag)
- [ ] `admin-portal` image built (`tsc --noEmit` clean verified locally)

### Database — verify before hub-ops deploy
> **Status: verified in code — no manual action required.**

- **`hub_ops.containers` status CHECK** — migration `0006_extend_container_status.sql`
  drops and recreates the CHECK with all 10 values
  (`planning`, `manifested`, `loading`, `sealed`, `in_transit`, `arrived_at_port`,
  `customs`, `released`, `deconsolidated`, `delivered`).
  Postgres names the inline column CHECK `containers_status_check`;
  the `DROP CONSTRAINT IF EXISTS` matches this exactly, so existing VPS tables upgrade cleanly.

- **`departed_at` / `estimated_arrival` / `arrived_at` columns** already present since
  migration `0003_add_pallets_and_containers.sql`. No gap.

- **Verify after deploy:**
  ```sql
  docker exec logisticos-postgres psql -U logisticos -d svc_hub_ops -c \
    "SELECT constraint_name, check_clause
       FROM information_schema.check_constraints
      WHERE constraint_name = 'containers_status_check';"
  -- Expect: lists 'deconsolidated' in the clause

  \dt hub_ops.*
  -- Expect: hub_scans, hub_inventory, hub_locations,
  --         hub_transfer_manifests, hub_routing_configs present
  ```

### Engagement Templates
> **Status: fixed in code (`119bab2`) — no DB seed required.**

`event_consumer.rs` renders notification bodies from an inline `match resolved_template` block,
not the DB `notification_templates` registry (that registry is for HTTP `/v1/send` only).
Three hub-milestone arms were added: `shipment_at_port`, `shipment_customs_hold`,
`shipment_customs_cleared` (WhatsApp + Push). Any future consumer notification also
needs an inline arm — a DB seed alone is not sufficient.

### Kafka Topics
- [ ] Confirm broker `auto.create.topics.enable=true`, **or** pre-create the 10 new topics:
  ```
  hub.container.arrived_at_port     hub.container.customs_hold
  hub.container.customs_cleared     hub.container.released_domestic
  hub.container.deconsolidated      hub.shipment.dispatch_requested
  hub.shipment.carrier_booking_requested
  shipment.at_port  shipment.customs_hold  shipment.customs_cleared
  ```

### Config
- [ ] No new env vars required for this release — no `.env` changes on VPS

---

## Deploy Order (matters — producer before consumers)

### 1. hub-ops first
```sh
# Dokploy: redeploy hub-ops image
# On boot: logisticos_common::migrations::run applies 0006–0011 to svc_hub_ops
```
Watch logs for:
```
migrations applied: 0006_extend_container_status … 0011_create_hub_routing_configs
```

Verify migrations:
```sql
docker exec logisticos-postgres psql -U logisticos -d svc_hub_ops -c \
  "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 6;"
-- Expect: 0011, 0010, 0009, 0008, 0007, 0006 all present
```

### 2. Downstream consumers (order doesn't matter between these)
Redeploy: `dispatch`, `carrier`, `payments`, `engagement`, `order-intake`

Confirm each new consumer subscription in logs:
| Service | Log line to look for |
|---|---|
| dispatch | `Hub-dispatch consumer subscribed hub.shipment.dispatch_requested` |
| carrier | `Hub-carrier consumer subscribed hub.shipment.carrier_booking_requested` |
| payments | `customs_duty_consumer subscribed CONTAINER_CUSTOMS_CLEARED` |
| engagement | `hub_milestone_consumer subscribed` |
| order-intake | `hub status consumer started` |

### 3. admin-portal (after hub-ops API is live)
The board degrades gracefully to empty states if hub-ops is not yet up, but
deploy after hub-ops to avoid 404 noise.

---

## Smoke Tests

### 1. Health checks
```sh
for svc in hub-ops dispatch carrier payments engagement order-intake; do
  curl -s http://<VPS>:PORT/health | jq .status
done
# All should return "ok"
```

### 2. End-to-end container lifecycle
```sh
# Create container
POST /v1/containers { transport_mode: "sea", origin_hub_id: ..., destination_hub_id: ... }
# → 201, container_id

# Walk the state machine
POST /v1/containers/:id/arrive-at-port     { details: [...] }
POST /v1/containers/:id/enter-customs      { details: [...] }
POST /v1/containers/:id/clear-customs      { tenant_code: "CM", details: [...] }
POST /v1/containers/:id/release-domestic
POST /v1/containers/:id/deconsolidate      { destination_zone: "Makati", master_awbs: [...] }
```

After each step verify:
- The corresponding `hub.*` event published (Kafka console consumer)
- `GET /v1/containers?hub_id=...` returns updated status

After `clear-customs` with billable detail:
- `payments` logs a `customs_duty` invoice creation

After `deconsolidate` with `master_awbs`:
- `dispatch` attempts own-driver assignment (check dispatch logs)
- `engagement` sends/queues WhatsApp + push per recipient

### 3. Ops Portal
- Load `/hub-transfer` in admin portal
- Select a hub → confirm all 4 tabs render (Container Board, Customs Queue, Routing Config, Inventory Map)
- Container Board shows the test container in the correct column
- Inline transition button triggers the next state

### 4. MCP tools (if AI agent access is wired)
```json
{ "method": "tools/list" }
// Expect: get_hub_inventory, get_container_status, get_customs_queue,
//         assign_piece_to_pallet, trigger_deconsolidation in hub-ops MCP server
```

---

## Post-Deploy Monitoring (15 minutes)

- [ ] Error rates nominal on all 6 services (Grafana)
- [ ] No consumer lag building on `hub.*` topics (Kafka UI or `kafka-consumer-groups.sh`)
- [ ] P99 on `GET /v1/containers` < 200 ms
- [ ] No crash-loops (Dokploy service list — all green)

---

## Rollback

### Triggers
| Condition | Action |
|---|---|
| Any service crash-loops on boot | Redeploy previous GHCR image tag |
| Migration failure on hub-ops | See migration rollback below |
| 5xx rate > 2% on hub-ops endpoints sustained | Roll hub-ops back |
| Consumer crash-loop / unbounded lag on `hub.*` | Roll affected consumer back |
| P99 on `/v1/containers` > 200 ms sustained | Roll hub-ops back |

### Procedure
1. **Code rollback** — redeploy the previous GHCR image for the affected service(s).
   Old images do not subscribe to `hub.*` topics, so no orphan events accumulate.

2. **Migrations are additive — no down-migration needed.**
   Migrations `0006–0011` create new tables and widen the status CHECK.
   Rolling back the code leaves these tables in place; they are simply unused.
   Do **not** drop them — data may have been written if the service ran briefly.

3. If hub-ops is rolled back but consumers are left on the new image:
   The new consumers will idle (no `hub.*` events published); this is safe.
   Roll consumers back if crash-loops occur independently.

---

## Known Gaps / Follow-up

| Item | Status |
|---|---|
| Driver App Hub Mode (Kotlin `feature/hub` module) | Deferred — needs Gradle CI verification |
| Auto deconsolidation fallback timer (wall-clock `auto_fallback_window_mins`) | Currently escalates on immediate failure; scheduler is a future enhancement |
| Rate-shop carrier routing (needs parcel weight/dims in event) | Skipped with warn log when `carrier_id` is None |
| Engagement milestone templates (future copy changes) | Edit inline arms in `event_consumer.rs` — not DB seeds |
