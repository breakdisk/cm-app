# ADR-0007: Offline-First Architecture for the Driver Super App

**Status:** Accepted
**Date:** 2026-03-17
**Deciders:** Principal Architect, Engineering Manager — Mobile, Senior React Native Engineer — Driver App, COO

---

## Context

The Driver Super App is the primary field tool for all couriers on the LogisticOS platform. Drivers use it to:
- Receive and acknowledge delivery tasks
- Navigate multi-stop routes
- Capture Proof of Delivery (POD): photos, signatures, OTP confirmation, barcode scans
- Log pickup completions and failed delivery attempts
- Receive real-time instructions from the dispatch console

A significant portion of the driver network operates in conditions that make continuous connectivity unreliable:

- **Rural Philippines** — 3G/LTE dead zones in provincial delivery routes (Mindanao, Visayas interior)
- **Urban basements and parking structures** — apartment buildings and malls in Metro Manila frequently lose signal during parcel handoff
- **Building lobbies and elevators** — momentary signal loss during POD capture causes data loss if state lives only on the server
- **High-congestion urban areas** — network saturation during peak hours (11am–2pm, 5pm–8pm) causes API timeouts

Current behavior (online-only app): when a driver's connection drops while capturing a POD signature, the entire capture flow resets. The driver must ask the customer to re-sign. If the app crashes before submission, the POD is lost. This results in:
- Failed delivery events recorded incorrectly as "incomplete"
- Drivers re-visiting addresses unnecessarily
- Customer complaints about being asked to sign multiple times
- Operations team manually correcting delivery statuses via admin portal

The business requirement is clear: **a driver must be able to complete their full route, capture all PODs, and log all status updates even with zero connectivity for up to 8 hours**.

---

## Decision

The Driver Super App adopts an **offline-first architecture** using local SQLite as the primary data store for all mutable task state. The network is treated as an enhancement, not a requirement.

### Core Principle

> Write to local SQLite first. Sync to the backend when connectivity is available. The app never blocks the driver waiting for a network response.

---

## Architecture

### Local Data Store

**Technology:** `expo-sqlite` (v13+, using the new synchronous API) with WAL journal mode enabled for concurrent read/write performance.

The local database schema mirrors the server-side task model with additional sync metadata columns:

```sql
-- Task list received from server, cached locally
CREATE TABLE tasks (
    id                TEXT PRIMARY KEY,   -- UUID
    shipment_id       TEXT NOT NULL,
    tenant_id         TEXT NOT NULL,
    task_type         TEXT NOT NULL,      -- 'pickup' | 'delivery' | 'return'
    status            TEXT NOT NULL,      -- 'pending' | 'in_progress' | 'completed' | 'failed'
    consignee_name    TEXT NOT NULL,
    consignee_phone   TEXT NOT NULL,
    delivery_address  TEXT NOT NULL,
    geo_lat           REAL,
    geo_lng           REAL,
    sequence_order    INTEGER NOT NULL,
    cod_amount        REAL,
    cod_currency      TEXT,
    notes             TEXT,
    route_id          TEXT NOT NULL,
    server_updated_at TEXT,               -- ISO8601, from server
    local_updated_at  TEXT NOT NULL,      -- ISO8601, set by app
    synced            INTEGER NOT NULL DEFAULT 0  -- 0=pending, 1=synced
);

-- Outbound events waiting to be sent to the server
CREATE TABLE sync_queue (
    id            TEXT PRIMARY KEY,   -- ULID
    event_type    TEXT NOT NULL,      -- 'task_status_updated' | 'pod_captured' | 'location_ping' etc.
    payload       TEXT NOT NULL,      -- JSON blob
    created_at    TEXT NOT NULL,      -- ISO8601
    retry_count   INTEGER NOT NULL DEFAULT 0,
    last_error    TEXT,
    next_retry_at TEXT               -- ISO8601; NULL = eligible for immediate retry
);

-- POD captures: photo metadata, signature SVG path, OTP confirmation
CREATE TABLE pod_captures (
    id              TEXT PRIMARY KEY,
    task_id         TEXT NOT NULL REFERENCES tasks(id),
    capture_type    TEXT NOT NULL,  -- 'photo' | 'signature' | 'otp' | 'barcode'
    local_file_path TEXT,           -- absolute path to photo file in app cache
    s3_key          TEXT,           -- set after successful upload
    data            TEXT,           -- signature SVG, OTP value, barcode string
    captured_at     TEXT NOT NULL,
    latitude        REAL,
    longitude       REAL,
    synced          INTEGER NOT NULL DEFAULT 0
);

-- Driver location pings (buffered for batch upload)
CREATE TABLE location_pings (
    id         TEXT PRIMARY KEY,
    latitude   REAL NOT NULL,
    longitude  REAL NOT NULL,
    accuracy   REAL,
    speed      REAL,
    heading    REAL,
    recorded_at TEXT NOT NULL,  -- ISO8601
    synced     INTEGER NOT NULL DEFAULT 0
);
```

WAL mode is enabled at database open time:
```typescript
db.execSync('PRAGMA journal_mode = WAL;');
db.execSync('PRAGMA foreign_keys = ON;');
db.execSync('PRAGMA synchronous = NORMAL;');
```

---

### Write Path (Offline-Safe)

Every state mutation follows this sequence — no exceptions:

```
User Action (button tap)
       ↓
1. Write to SQLite (sync, blocking — < 2ms)
       ↓
2. Update local React state (optimistic UI — immediate feedback)
       ↓
3. Enqueue sync_queue entry for this mutation
       ↓
4. Return success to user — driver continues work
       ↓
5. (Background) SyncService drains sync_queue when online
```

**Example: POD signature capture**

```typescript
// src/features/pod/usePodCapture.ts

export async function captureSignature(
  taskId: string,
  signatureSvgPath: string,
  location: GeoPoint,
): Promise<void> {
  const podId = ulid();
  const capturedAt = new Date().toISOString();

  // Step 1: Write to SQLite — this CANNOT fail (retry if SQLite error)
  await db.runAsync(
    `INSERT INTO pod_captures (id, task_id, capture_type, data, latitude, longitude, captured_at, synced)
     VALUES (?, ?, 'signature', ?, ?, ?, ?, 0)`,
    [podId, taskId, signatureSvgPath, location.lat, location.lng, capturedAt],
  );

  // Step 2 & 3: Enqueue upload to sync queue
  await enqueueSyncEvent({
    event_type: 'pod_signature_captured',
    payload: { pod_id: podId, task_id: taskId, captured_at: capturedAt },
  });

  // Step 4: Update task status locally
  await updateTaskStatusLocal(taskId, 'completed');
}
```

The driver sees the signature accepted immediately. Network is irrelevant to the UX.

---

### SyncService — Queue Drain

`SyncService` is a singleton that runs continuously in the background:

```typescript
// src/services/sync/SyncService.ts

export class SyncService {
  private isOnline: boolean = false;
  private isSyncing: boolean = false;

  constructor(
    private readonly apiClient: LogisticOSApiClient,
    private readonly db: SQLiteDatabase,
  ) {}

  async start(): Promise<void> {
    NetInfo.addEventListener((state) => {
      this.isOnline = state.isConnected ?? false;
      if (this.isOnline) this.drainQueue();
    });

    // Also drain on app foreground
    AppState.addEventListener('change', (state) => {
      if (state === 'active' && this.isOnline) this.drainQueue();
    });
  }

  private async drainQueue(): Promise<void> {
    if (this.isSyncing) return;
    this.isSyncing = true;

    try {
      while (true) {
        const events = await this.db.getAllAsync<SyncQueueRow>(
          `SELECT * FROM sync_queue
           WHERE (next_retry_at IS NULL OR next_retry_at <= datetime('now'))
             AND retry_count < 5
           ORDER BY created_at ASC
           LIMIT 20`,
        );

        if (events.length === 0) break;

        for (const event of events) {
          await this.processEvent(event);
        }
      }
    } finally {
      this.isSyncing = false;
    }
  }

  private async processEvent(event: SyncQueueRow): Promise<void> {
    try {
      await this.apiClient.submitSyncEvent(event.event_type, JSON.parse(event.payload));
      await this.db.runAsync(`DELETE FROM sync_queue WHERE id = ?`, [event.id]);
    } catch (error) {
      const backoffSeconds = Math.pow(2, event.retry_count) * 30; // 30s, 60s, 120s, 240s, 480s
      await this.db.runAsync(
        `UPDATE sync_queue
         SET retry_count = retry_count + 1,
             last_error = ?,
             next_retry_at = datetime('now', '+' || ? || ' seconds')
         WHERE id = ?`,
        [String(error), backoffSeconds, event.id],
      );
    }
  }
}
```

**Background sync** uses `expo-background-fetch` to drain the queue even when the app is not in the foreground. Registered task name: `LOGISTICOS_SYNC_QUEUE`. Minimum interval: 15 minutes (iOS/Android background task minimum). On Android, `expo-task-manager` with `BACKGROUND_FETCH` is registered at app startup.

---

### Photo Upload (POD Images)

Photo files are large (500KB–3MB each). They use a separate upload path from the sync queue:

1. Photo captured → saved to `FileSystem.cacheDirectory/pods/<pod_id>.jpg`
2. `pod_captures` row inserted with `local_file_path` set, `s3_key` null, `synced = 0`
3. When online: `PhotoUploadService` requests an S3 presigned URL from the `pod-service` API
4. Uploads directly from device to S3 using the presigned URL (bypasses the API server for large payloads)
5. On success: updates `pod_captures.s3_key` and `synced = 1`, deletes local cache file
6. On failure: backs off using the same retry schedule as the sync queue

Photos are never deleted from local cache until confirmed uploaded. If device storage is under 100MB, old synced photos are purged in FIFO order.

---

### Server → Device Sync (Task Updates)

The server pushes task updates via:
1. **On foreground / connect:** full task list refresh via `GET /driver/routes/active` — overwrites local tasks for the current route
2. **Push notifications** (Expo Push) — wake signal to trigger a background sync pull
3. **WebSocket** (when online and app is foregrounded) — real-time task mutations (new stop added, task cancelled by dispatcher)

WebSocket events are processed by updating the local SQLite row first, then reflecting in UI — same write path as local mutations.

---

### Conflict Resolution

| Data Type | Strategy | Rationale |
|-----------|----------|-----------|
| Location pings | Last-write-wins; server always accepts | Location is append-only telemetry; no conflicts possible |
| Task status | **Server-authoritative** — server rejects a status transition that violates the state machine | Prevents driver from marking a task complete when dispatcher has cancelled it |
| POD captures | Immutable after creation; no conflict possible | POD records are append-only |
| Route sequence | Server-authoritative — server may reorder stops (traffic rerouting by dispatch AI) | Dispatcher changes take precedence over driver's current sequence |

**Task status conflict handling:**
When the server rejects a local status update (e.g., driver marks `completed` but server says the shipment was `cancelled`):
1. Server returns `409 Conflict` with the authoritative task state
2. SyncService writes the server state to local SQLite
3. UI shows a toast: "This task has been updated by your dispatcher. Please review."
4. The conflict event is logged for ops review

---

### SQLite Schema Migrations

Migrations are managed by a lightweight in-app migrator (`src/db/migrations/`) using sequential integer version numbers stored in `PRAGMA user_version`. On app startup, pending migrations are applied before any database access.

Migration rules:
- **Backward-compatible only** — new columns must have `DEFAULT` values; tables are never dropped in a live migration
- **No destructive operations** in migration files once released to production
- **Tested in CI** — the test suite runs all migrations from version 0 on a fresh SQLite database on every PR

---

### Connectivity State Indicator

The app displays a persistent connectivity badge in the top navigation bar:

- **Solid green dot** — online, sync queue empty
- **Animated yellow dot** — online, syncing (queue draining)
- **Grey dot** — offline, queue depth shown as a badge count (e.g., "12 pending")

The badge count motivates drivers to seek connectivity (find a window, step outside) rather than continuing to work with a large pending queue.

---

## Technology Choices

| Concern | Technology | Version |
|---------|-----------|---------|
| Local database | `expo-sqlite` | v14+ (new synchronous API) |
| Background sync | `expo-background-fetch` + `expo-task-manager` | latest |
| Network state detection | `@react-native-community/netinfo` | ^11 |
| File system (POD photos) | `expo-file-system` | ^17 |
| Barcode scanning | `expo-barcode-scanner` (migrating to `expo-camera` v14 API) | ^13 |
| Signature capture | `react-native-signature-canvas` | ^4 |
| ULID generation | `ulid` npm package | ^2 |
| State management | Zustand (online task UI state) + SQLite (persistent source of truth) | ^4 |
| Animations | React Native Reanimated 3 | ^3 |

---

## Consequences

### Positive

- **Zero POD data loss** — signature, photo, and OTP are committed to local storage before any network call. Even a device reboot mid-capture recovers the data on restart.
- **Driver UX unaffected by connectivity** — the app never shows a loading spinner waiting for a server response during core task flows. Drivers complete their routes uninterrupted.
- **Battery and data efficiency** — location pings and status updates are batched and sent together when online, rather than one HTTP request per event.
- **Resilience to server downtime** — drivers continue operating during backend deployments or incidents. Data is safely queued.

### Negative

- **SQLite schema migrations must be backward-compatible** — a bad migration deployed to drivers in the field cannot be rolled back. Requires discipline in migration authoring and staging environment validation.
- **Conflict edge cases require ops tooling** — the operations team needs a dashboard to view and resolve task status conflicts. This is a new support workflow.
- **Storage growth** — a driver with poor connectivity for 8 hours accumulates local POD photos. Storage management (cache eviction policy) must be implemented and tested on low-storage devices.
- **Background sync limitations on iOS** — iOS aggressively limits background fetch to ~15-minute intervals. Drivers who park their phone for extended periods may have a large sync queue on return. Mitigated by sync-on-foreground trigger.
- **Testing complexity** — offline-first logic requires mocking `NetInfo`, `SQLite`, and simulating queue drain in unit and integration tests. Additional test infrastructure cost.

---

## Alternatives Considered

| Alternative | Reason Rejected |
|-------------|----------------|
| **Service Workers + IndexedDB (React Native Web)** | Not viable for React Native; service workers are a browser API. IndexedDB has no first-class React Native support. |
| **Redux Persist + AsyncStorage** | AsyncStorage is key-value only, not relational. Complex queries (e.g., "all unsynced PODs for route X") require manual indexing. Performance degrades with large data sets. |
| **WatermelonDB** | Feature-rich reactive DB for React Native. Evaluated seriously. Rejected because `expo-sqlite` v14's synchronous API provides sufficient performance without WatermelonDB's Rx subscription complexity. May revisit if sync conflict requirements grow. |
| **Apollo Client with offline mutations (GraphQL)** | Requires GraphQL adoption across the backend, which conflicts with the Rust/REST/gRPC architecture. Offline mutation queuing is Apollo-specific and non-portable. |
| **Realm (MongoDB Realm)** | Vendor lock-in to MongoDB Atlas Device Sync. Sync conflict resolution is opaque. Our server-authoritative conflict model requires custom logic. |

---

## Related ADRs

- [ADR-0001](0001-rust-for-all-backend-services.md) — Rust backend (driver-ops service receives synced events)
- [ADR-0002](0002-event-driven-inter-service-communication.md) — Event-driven communication (synced events published to Kafka)
- [ADR-0006](0006-kafka-event-streaming-topology.md) — Kafka topology (`logisticos.driver.delivery.completed`, `logisticos.pod.capture.completed`)
- [ADR-0008](0008-multi-tenancy-rls-strategy.md) — RLS (tenant_id propagated in all synced events)
