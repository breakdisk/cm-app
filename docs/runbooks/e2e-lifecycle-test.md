# E2E Lifecycle Test Runbook

**Purpose:** verify the full merchant → dispatch → driver → POD → delivered flow after every backend change that touches those services. Not a unit test — a **manual integration gate**.

**Scope:** happy path + three negative tests. Expected runtime ~20 minutes if clean, 40 minutes if something breaks.

---

## Prerequisites — verify before starting

```bash
# 1. Latest images pulled on VPS
cd /etc/dokploy/compose/oscargomarketnet-logisticosbackend-pqfh0u/code/
git log --oneline -1     # should be >= 87d4c08 (or later)

# 2. Required env var on VPS for geocoder
grep GEOCODER__MAPBOX_ACCESS_TOKEN .env
# If missing, append (use same pk.* token from driver-app-android local.properties):
# echo "GEOCODER__MAPBOX_ACCESS_TOKEN=pk.eyJ1IjoiZWR1YXJkY2xlb2ZlI..." >> .env

# 3. All services up with recent images
docker compose pull order-intake dispatch pod driver-ops payments engagement delivery-experience
docker compose up -d --force-recreate order-intake dispatch pod driver-ops payments engagement delivery-experience

# 4. Confirm order-intake picked up Mapbox
docker logs logisticos-order-intake --since 60s 2>&1 | grep -i "address normalizer"
# Expect: "address normalizer: Mapbox geocoder"

# 5. Driver seed state
docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops -c "
SET row_security = off;
SELECT user_id, status, is_active,
       (SELECT COUNT(*) FROM dispatch.driver_assignments da
          WHERE da.driver_id = d.user_id
            AND da.status IN ('pending','accepted')) AS blocking_assignments
FROM driver_ops.drivers d
WHERE user_id = '02a01c16-03a9-46ea-a582-76cc3d4425ec';"
# Expect: status='available', is_active=true, blocking_assignments=0
# If blocking_assignments>0, cancel first:
# UPDATE dispatch.driver_assignments SET status='cancelled'
#   WHERE driver_id='<uuid>' AND status IN ('pending','accepted');
```

Driver app on device:
- Logged in as `02a01c16-03a9-46ea-a582-76cc3d4425ec`
- Home screen shows **ONLINE** green toggle
- Location permission granted (Settings → App → Permissions → Location = Allow)
- Recent ping (< 60s) — verify with the driver state query above showing non-null `last_ping_at`

---

## Test 1 — Happy path (merchant books → driver delivers)

Record a test session ID so it's easy to filter logs:
```bash
export TEST_ID="e2e-$(date +%Y%m%d-%H%M)"
echo $TEST_ID
```

### Stage 1.1 — Merchant creates shipment

**Action (UI):** Merchant portal → Shipments → New Shipment → fill real-looking origin + destination addresses for the driver's region. Use Abu Dhabi coords (24.45/54.37) as origin since the test driver is there.

**Validation:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_order_intake -c "
SET row_security = off;
SELECT id, tracking_number, status,
       origin_lat, origin_lng, origin_city, origin_country_code,
       dest_lat,   dest_lng,   dest_city,   dest_country_code,
       origin_point IS NOT NULL AS origin_geo,
       dest_point   IS NOT NULL AS dest_geo,
       created_at
FROM order_intake.shipments
ORDER BY created_at DESC
LIMIT 1;"
```

**Expect:**
- `status = 'pending'`
- `origin_lat`, `origin_lng` populated (non-NULL) ← **this proves the Mapbox geocoder fired**
- `origin_geo = t`, `dest_geo = t` (trigger `sync_shipment_points` did its job)

**If origin_lat is NULL:** check order-intake logs for `"Mapbox geocode failed"` or `"no results"`. The address may be too sparse for Mapbox to resolve.

```bash
docker logs logisticos-order-intake --since 2m 2>&1 | grep -iE "mapbox|normalizer|geocode"
```

### Stage 1.2 — Dispatch assigns driver

**Action (UI):** Admin portal → Dispatch console → find the new shipment in the queue → click **Dispatch**.

**Validation:**
```bash
docker logs logisticos-dispatch --since 30s 2>&1 | grep -iE "quick dispatch|no drivers|no coordinates"
```

**Expect:**
- `INFO ... Quick dispatch complete, shipment_id: <uuid>, driver_id: 02a01c16-..., assignment_id: <new uuid>`

**If "No available drivers nearby":**
- Re-run driver seed state query (prereq #5)
- If `blocking_assignments > 0`, an orphan pending row exists — cancel it

**If "Shipment has no origin/destination coordinates":**
- The Manila fallback removal is working AS INTENDED. Fix = address wasn't geocoded at Stage 1.1. Re-run prereq #4.

**Dispatch DB state:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_dispatch -c "
SELECT da.id, da.driver_id, da.status, da.assigned_at, r.status AS route_status
FROM dispatch.driver_assignments da
LEFT JOIN dispatch.routes r ON r.id = da.route_id
ORDER BY da.assigned_at DESC LIMIT 1;"
```
Expect: `status = 'pending'`, `route_status = 'Planned'`.

### Stage 1.3 — Driver app receives tasks

**Validation:**
```bash
docker logs logisticos-driver-ops --since 30s 2>&1 | grep "task created"
# Expect: 2 lines (pickup seq 1 + delivery seq 2) for the new shipment_id

docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops -c "
SET row_security = off;
SELECT task_type, sequence, status, shipment_id
FROM driver_ops.tasks
WHERE driver_id='02a01c16-03a9-46ea-a582-76cc3d4425ec'
  AND shipment_id = '<paste shipment_id from 1.1>'
ORDER BY sequence;"
```
Expect: 2 rows, both `status='pending'`, one pickup (seq=1), one delivery (seq=2).

**On device:** Route list shows 2 new task cards.

**If not showing on device:** check the driver is logged in as the correct UUID (we hit this earlier — user was logged in as `e88a7717` instead of `02a01c16`).

### Stage 1.4 — Driver completes pickup

**Action (device):** Tap pickup task → Navigation screen → "I've Arrived" → "Start Task" → PodScreen (pickup typically requires no photo/sig) → **Submit POD**.

**Validation — driver-ops:**
```bash
docker logs logisticos-driver-ops --since 1m 2>&1 | grep -iE "start|complete|task_completed"
```
Expect: a `Task completed` log with pickup task_id.

**Validation — pod service:**
```bash
docker logs logisticos-pod --since 1m 2>&1 | grep -iE "geofence|submit"
```
Expect: `POD geofence check`, then `POD submitted`.

**Validation — shipment status flipped** *(new behavior after this session's fix):*
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_order_intake -c "
SET row_security = off;
SELECT status, updated_at FROM order_intake.shipments
WHERE id = '<shipment_id>';"
```
**Expect: `status = 'picked_up'`** ← **this proves the PICKUP_COMPLETED consumer fix works**

**If still `pickup_assigned`:** the `PICKUP_COMPLETED` handler we added to `services/order-intake/src/infrastructure/messaging/status_consumer.rs` didn't deploy. Verify:
```bash
docker logs logisticos-order-intake --since 5m 2>&1 | grep -iE "PICKUP|pickup_completed|status_consumer"
```
A WARN line naming the shipment means the handler ran but didn't find the row. Silence means the consumer isn't subscribed — rebuild order-intake.

**On device:** pickup task disappears, delivery task (sequence 2) is now the active one.

### Stage 1.5 — Driver navigates + delivers

**Action (device):** Tap delivery task → Navigation → Arrival → Start Task → PodScreen (this one may require photo+signature based on service type).

For the test, use a task with `requiresPhoto=false, requiresSignature=false, requiresOtp=false` first. If the booking forces them:
- Photo: tap camera button, snap anything
- Signature: draw on pad, Save
- OTP: tap "Generate OTP" then enter `123456` (dev bypass — see `project_driver_app_android_status.md`)

Submit POD.

**Validation — shipment status:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_order_intake -c "
SET row_security = off;
SELECT status, updated_at FROM order_intake.shipments
WHERE id = '<shipment_id>';"
```
**Expect: `status = 'delivered'`**

**Validation — driver-ops tasks:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops -c "
SET row_security = off;
SELECT task_type, sequence, status, completed_at, pod_id
FROM driver_ops.tasks
WHERE driver_id='02a01c16-03a9-46ea-a582-76cc3d4425ec'
  AND shipment_id = '<shipment_id>'
ORDER BY sequence;"
```
Expect: both rows `status='completed'`, `completed_at` set, `pod_id` set.

**Validation — driver free again:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops -c "
SELECT (SELECT COUNT(*) FROM dispatch.driver_assignments
        WHERE driver_id = '02a01c16-03a9-46ea-a582-76cc3d4425ec'
          AND status IN ('pending','accepted')) AS blocking;"
```
Expect: `blocking = 0` (the assignment moved to `accepted`/`completed`, freeing the driver for the next dispatch).

### Stage 1.6 — Portals reflect final state

**Merchant portal** → Shipments list → refresh → the shipment row shows **Delivered** status.

**Admin portal** → Shipments → same shipment shows **Delivered** + driver name + delivered_at timestamp.

**Customer portal** (if the shipment had a tracking link) → `/track/{AWB}` → timeline shows all events: created → pickup_assigned → picked_up → delivered, each with timestamps.

---

## Test 2 — Negative: missing Mapbox token

**Purpose:** confirm the Manila-fallback removal surfaces the real error instead of routing to Manila.

**Setup:**
```bash
# On VPS — temporarily blank the token
sed -i.bak 's/^GEOCODER__MAPBOX_ACCESS_TOKEN=.*/GEOCODER__MAPBOX_ACCESS_TOKEN=/' .env
docker compose up -d --force-recreate order-intake
docker logs logisticos-order-intake --since 10s 2>&1 | grep -i "GEOCODER"
# Expect: WARN "GEOCODER__MAPBOX_ACCESS_TOKEN not set — shipments will be created with coordinates: None..."
```

**Action:** Create a shipment in merchant portal.

**Validation:**
```sql
-- origin_lat and origin_lng should be NULL
-- because we fell back to PassthroughNormalizer
```

**Action:** Admin portal → Dispatch.

**Expect error in UI:** `"Shipment has no origin/destination coordinates — cannot dispatch. Ensure the merchant address is geocoded before booking."`

**Teardown:**
```bash
mv .env.bak .env
docker compose up -d --force-recreate order-intake
```

---

## Test 3 — Negative: stale driver (no recent GPS ping)

**Purpose:** confirm dispatch filters drivers whose last ping is > 10 min old.

**Setup:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_driver_ops -c "
UPDATE driver_ops.driver_locations
SET recorded_at = NOW() - INTERVAL '15 minutes'
WHERE driver_id = '02a01c16-03a9-46ea-a582-76cc3d4425ec';"
```

**Action:** Admin → Dispatch on a fresh shipment.

**Expect:** `"No available drivers nearby"` (driver filtered out by staleness, correctly this time).

**Teardown:** open driver app → Home → Offline toggle → Online toggle. This pushes a fresh ping.

---

## Test 4 — Orphan assignment cleanup

**Purpose:** confirm the `uq_driver_active_assignment` unique index prevents double-assignment.

**Setup:**
```bash
# Manually create an orphan pending assignment
docker exec logisticos-postgres psql -U logisticos -d svc_dispatch -c "
INSERT INTO dispatch.driver_assignments (tenant_id, driver_id, route_id, status)
SELECT 'cc919797-2997-4f3a-a825-3fd0470ccae8'::uuid,
       '02a01c16-03a9-46ea-a582-76cc3d4425ec'::uuid,
       id, 'pending'
FROM dispatch.routes LIMIT 1;"
```

**Action:** Admin → Dispatch.

**Expect:** `"No available drivers nearby"` (driver blocked by orphan).

**Teardown:**
```bash
docker exec logisticos-postgres psql -U logisticos -d svc_dispatch -c "
UPDATE dispatch.driver_assignments SET status='cancelled'
WHERE driver_id='02a01c16-03a9-46ea-a582-76cc3d4425ec'
  AND status IN ('pending','accepted');"
```

---

## Failure mode → diagnosis cheatsheet

| Symptom | First thing to check | Likely cause |
|---|---|---|
| Dispatch says "No available drivers nearby" | driver seed state SQL | orphan assignment, stale ping, or wrong tenant |
| Dispatch says "Shipment has no coordinates" | order-intake logs for Mapbox WARN | geocoder token missing / address unresolvable |
| Driver app doesn't see new task | driver UUID matches tenant's driver record | logged in as different driver |
| POD submit returns 422 | pod service logs + driver app logcat | SubmitPodCommand / CompleteTaskCommand body/path mismatch (regression) |
| POD submit succeeds but shipment stays `pickup_assigned` | order-intake status_consumer logs | PICKUP_COMPLETED not subscribed (regression of this session's fix) |
| Shipment stays `picked_up` after delivery POD | DELIVERY_COMPLETED consumer log | same class of bug on delivery event |
| Task disappears but merchant portal doesn't refresh | front-end polling or cache | not a backend bug; investigate `SWR`/refetch interval |
| Customer portal tracking timeline missing events | `delivery-experience` consumer logs | downstream tracking store not updated |

---

## After each run

1. Record the shipment_id, assignment_id, and test outcome in a test log.
2. If any stage fails, capture the relevant service log tail + DB row before fixing.
3. After fix + re-run, mark green.

Test passes only when Test 1 + Tests 2, 3, 4 all behave as specified above. Do not treat Stage 1 passing in isolation as "done" — a regression in any of 2–4 is just as important as a Stage 1 failure.
