# POD Sync Gap Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix deliveries that complete locally in the driver app but never reach the backend, by splitting the sync queue's `POD_SUBMIT` action into `POD_SUBMIT` (steps 1–6) and `TASK_COMPLETE` (step 7), adding an `isSynced` field to `TaskEntity` for a pending-sync badge, wiring a `NetworkConnectivityObserver` for immediate retry on reconnect, and adding backend validation requiring `pod_id` on delivery task completion.

**Architecture:** `DeliveryRepository.submitPod()` runs steps 1–6 then marks the task `COMPLETED` locally with `isSynced=false`. Step 7 (`PUT /v1/tasks/{id}/complete`) runs inside a nested try; if it fails, only a `TASK_COMPLETE` item is enqueued. `OutboundSyncWorker` gets a new `TASK_COMPLETE` branch that calls only `completeTask()` and sets `isSynced=true` on success. The backend rejects delivery task completions without `pod_id`.

**Tech Stack:** Kotlin + Room + WorkManager + OkHttp + Hilt (Android); Rust + Axum (backend)

---

## File Map

| Action | File |
|--------|------|
| Modify | `services/driver-ops/src/application/services/task_service.rs` |
| Modify | `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/SyncQueueEntity.kt` |
| Modify | `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/TaskEntity.kt` |
| Modify | `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/DriverDatabase.kt` |
| Modify | `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/dao/TaskDao.kt` |
| Modify | `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/worker/OutboundSyncWorker.kt` |
| Modify | `apps/driver-app-android/feature/delivery/src/main/kotlin/io/logisticos/driver/feature/delivery/data/DeliveryRepository.kt` |
| Create | `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/NetworkConnectivityObserver.kt` |
| Modify | `apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/DriverApplication.kt` |
| Modify | `apps/driver-app-android/feature/route/src/main/kotlin/io/logisticos/driver/feature/route/ui/RouteScreen.kt` |

---

## Task 1: Backend — Validate `pod_id` on Delivery Task Completion

**Files:**
- Modify: `services/driver-ops/src/application/services/task_service.rs`

- [ ] **Step 1: Add validation before `task.complete()`**

In `complete_task()` (line 101), add this block immediately after the `InProgress` status check (after line 111, before `task.complete(cmd.pod_id)` on line 113):

```rust
// Delivery tasks must always link to a POD — no completions without evidence.
if task.task_type == TaskType::Delivery && cmd.pod_id.is_none() {
    return Err(AppError::BusinessRule(
        "delivery task completion requires pod_id".into(),
    ));
}
```

The complete `complete_task` function block around the change site should now read:

```rust
pub async fn complete_task(
    &self,
    driver_id: &DriverId,
    tenant_id: &TenantId,
    cmd: CompleteTaskCommand,
) -> AppResult<()> {
    let mut task = self.fetch_and_validate_ownership(driver_id, cmd.task_id).await?;

    if task.status != TaskStatus::InProgress {
        return Err(AppError::BusinessRule("Can only complete an in-progress task".into()));
    }

    // Delivery tasks must always link to a POD — no completions without evidence.
    if task.task_type == TaskType::Delivery && cmd.pod_id.is_none() {
        return Err(AppError::BusinessRule(
            "delivery task completion requires pod_id".into(),
        ));
    }

    task.complete(cmd.pod_id)
        .map_err(|e| AppError::BusinessRule(e.to_string()))?;
    // ... rest unchanged
```

- [ ] **Step 2: Run the backend tests**

```bash
cd services/driver-ops
cargo test 2>&1 | tail -20
```

Expected: all tests pass. If any fail, fix before continuing.

- [ ] **Step 3: Commit**

```bash
git add services/driver-ops/src/application/services/task_service.rs
git commit -m "feat(driver-ops): require pod_id for delivery task completion"
```

---

## Task 2: Android — Add `TASK_COMPLETE` to `SyncAction` and `FAILED_SYNC` to `TaskStatus`

**Files:**
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/SyncQueueEntity.kt`
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/TaskEntity.kt`

- [ ] **Step 1: Add `TASK_COMPLETE` to `SyncAction`**

In `SyncQueueEntity.kt`, change the enum to:

```kotlin
enum class SyncAction {
    TASK_STATUS_UPDATE, POD_SUBMIT, TASK_COMPLETE, SCAN_EVENT, COD_CONFIRM, SHIFT_START, SHIFT_END
}
```

- [ ] **Step 2: Add `FAILED_SYNC` to `TaskStatus`**

In `TaskEntity.kt`, change the enum to:

```kotlin
enum class TaskStatus {
    ASSIGNED, EN_ROUTE, ARRIVED, IN_PROGRESS, COMPLETED, ATTEMPTED, FAILED, RETURNED, FAILED_SYNC
}
```

- [ ] **Step 3: Build to confirm no compile errors**

```bash
cd apps/driver-app-android
./gradlew :core:database:assembleDebug --no-daemon 2>&1 | tail -20
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/SyncQueueEntity.kt \
        apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/TaskEntity.kt
git commit -m "feat(driver-app): add TASK_COMPLETE sync action and FAILED_SYNC task status"
```

---

## Task 3: Android — Add `isSynced` to `TaskEntity` + Room Migration 3 → 4

**Files:**
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/TaskEntity.kt`
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/DriverDatabase.kt`

- [ ] **Step 1: Add `isSynced` field to `TaskEntity`**

In `TaskEntity.kt`, add `isSynced: Boolean = true` as the last field before the closing parenthesis. The full entity becomes:

```kotlin
package io.logisticos.driver.core.database.entity

import androidx.room.Entity
import androidx.room.PrimaryKey

enum class TaskStatus {
    ASSIGNED, EN_ROUTE, ARRIVED, IN_PROGRESS, COMPLETED, ATTEMPTED, FAILED, RETURNED, FAILED_SYNC
}

enum class TaskType {
    PICKUP,
    DELIVERY,
    RETURN,
    HUB_DROP
}

@Entity(tableName = "tasks")
data class TaskEntity(
    @PrimaryKey val id: String,
    val shiftId: String = "",
    val shipmentId: String = "",
    val taskType: TaskType = TaskType.DELIVERY,
    val awb: String,
    val recipientName: String,
    val recipientPhone: String,
    val address: String,
    val lat: Double = 0.0,
    val lng: Double = 0.0,
    val status: TaskStatus,
    val stopOrder: Int,
    val requiresPhoto: Boolean = false,
    val requiresSignature: Boolean = false,
    val requiresOtp: Boolean = false,
    val isCod: Boolean = false,
    val codAmount: Double = 0.0,
    val attemptCount: Int = 0,
    val failureReason: String? = null,
    val notes: String? = null,
    val syncedAt: Long?,
    val isSynced: Boolean = true,
)
```

- [ ] **Step 2: Bump database version and add migration in `DriverDatabase.kt`**

Replace the entire file with:

```kotlin
package io.logisticos.driver.core.database

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import io.logisticos.driver.core.database.dao.*
import io.logisticos.driver.core.database.entity.*

val MIGRATION_3_4 = object : Migration(3, 4) {
    override fun migrate(database: SupportSQLiteDatabase) {
        database.execSQL(
            "ALTER TABLE tasks ADD COLUMN is_synced INTEGER NOT NULL DEFAULT 1"
        )
    }
}

@TypeConverters(Converters::class)
@Database(
    entities = [
        ShiftEntity::class,
        TaskEntity::class,
        RouteEntity::class,
        PodEntity::class,
        LocationBreadcrumbEntity::class,
        ScanEventEntity::class,
        SyncQueueEntity::class,
    ],
    version = 4,
    exportSchema = true
)
abstract class DriverDatabase : RoomDatabase() {
    abstract fun shiftDao(): ShiftDao
    abstract fun taskDao(): TaskDao
    abstract fun routeDao(): RouteDao
    abstract fun podDao(): PodDao
    abstract fun locationBreadcrumbDao(): LocationBreadcrumbDao
    abstract fun scanEventDao(): ScanEventDao
    abstract fun syncQueueDao(): SyncQueueDao
}
```

- [ ] **Step 3: Wire the migration into the Room builder**

Search for where `DriverDatabase` is built with Room (look for `Room.databaseBuilder`):

```bash
grep -r "databaseBuilder" apps/driver-app-android --include="*.kt" -l
```

Open that file and add `.addMigrations(MIGRATION_3_4)` to the builder chain, importing `io.logisticos.driver.core.database.MIGRATION_3_4`. It will look like:

```kotlin
Room.databaseBuilder(context, DriverDatabase::class.java, "driver_db")
    .addMigrations(MIGRATION_3_4)
    .build()
```

- [ ] **Step 4: Build to confirm migration compiles**

```bash
cd apps/driver-app-android
./gradlew :core:database:assembleDebug --no-daemon 2>&1 | tail -20
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 5: Commit**

```bash
git add apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/entity/TaskEntity.kt \
        apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/DriverDatabase.kt
# Also add the DI file found in step 3
git commit -m "feat(driver-app): add isSynced to TaskEntity, Room migration 3→4"
```

---

## Task 4: Android — Add New `TaskDao` Queries

**Files:**
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/dao/TaskDao.kt`

- [ ] **Step 1: Add three new queries to `TaskDao`**

Append these three methods to the `TaskDao` interface (before the closing `}`):

```kotlin
    @Query("UPDATE tasks SET is_synced = 1 WHERE id = :taskId")
    suspend fun markSynced(taskId: String)

    @Query("UPDATE tasks SET is_synced = 0, status = 'FAILED_SYNC' WHERE id = :taskId")
    suspend fun markSyncFailed(taskId: String)

    @Query("UPDATE tasks SET status = :status, is_synced = :isSynced WHERE id = :taskId")
    suspend fun updateStatusWithSync(taskId: String, status: TaskStatus, isSynced: Boolean)
```

The existing `updateStatus(taskId, status)` stays unchanged — callers that don't touch `isSynced` keep using it.

- [ ] **Step 2: Build to confirm**

```bash
cd apps/driver-app-android
./gradlew :core:database:assembleDebug --no-daemon 2>&1 | tail -20
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/dao/TaskDao.kt
git commit -m "feat(driver-app): add markSynced, markSyncFailed, updateStatusWithSync to TaskDao"
```

---

## Task 5: Android — Update `OutboundSyncWorker`

**Files:**
- Modify: `apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/worker/OutboundSyncWorker.kt`

- [ ] **Step 1: Replace the `POD_SUBMIT` branch and add `TASK_COMPLETE` branch**

In `processItem()`, make two changes:

**Change A — End of `POD_SUBMIT` branch:** Replace the final 3 lines of the `POD_SUBMIT` branch:

```kotlin
// OLD — replace these three lines:
// 5. Complete the task with the pod_id so driver-ops links them
driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId))
podDao.markSynced(taskId)
taskDao.updateStatus(taskId, io.logisticos.driver.core.database.entity.TaskStatus.COMPLETED)
```

With:

```kotlin
// 4b. Mark POD as synced now that it's on the server.
podDao.markSynced(taskId)

// 5. Enqueue TASK_COMPLETE (step 7) separately so it has its own retry lifecycle.
//    The task is already COMPLETED locally; this just confirms it with the backend.
taskDao.updateStatusWithSync(
    taskId,
    io.logisticos.driver.core.database.entity.TaskStatus.COMPLETED,
    isSynced = false,
)
val taskCompletePayload = kotlinx.serialization.json.buildJsonObject {
    put("taskId", taskId)
    put("podId", podId)
}.toString()
syncQueueDao.enqueue(
    io.logisticos.driver.core.database.entity.SyncQueueEntity(
        action = io.logisticos.driver.core.database.entity.SyncAction.TASK_COMPLETE,
        payloadJson = taskCompletePayload,
        createdAt = System.currentTimeMillis(),
    )
)
OutboundSyncWorker.kickOnce(applicationContext)
```

**Change B — Add `TASK_COMPLETE` branch** in the `when (item.action)` block, before the final `else ->` branch:

```kotlin
SyncAction.TASK_COMPLETE -> {
    val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
        ?: run { syncQueueDao.remove(item.id); return }
    val podId = payload["podId"]?.jsonPrimitive?.contentOrNull
        ?: run { syncQueueDao.remove(item.id); return }

    // After 7 days with no success, the backend may have auto-cancelled the task.
    // Mark locally as permanently failed so the driver knows to contact support.
    val sevenDaysMs = 7L * 24 * 60 * 60 * 1_000
    if (item.createdAt < System.currentTimeMillis() - sevenDaysMs) {
        taskDao.markSyncFailed(taskId)
        syncQueueDao.remove(item.id)
        return
    }

    driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId))
    taskDao.markSynced(taskId)
    podDao.markSynced(taskId)
}
```

Add the required import for `buildJsonObject` at the top of the file:

```kotlin
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
```

- [ ] **Step 2: Build to confirm**

```bash
cd apps/driver-app-android
./gradlew :core:database:assembleDebug --no-daemon 2>&1 | tail -20
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/core/database/src/main/kotlin/io/logisticos/driver/core/database/worker/OutboundSyncWorker.kt
git commit -m "feat(driver-app): split POD_SUBMIT into POD_SUBMIT + TASK_COMPLETE in OutboundSyncWorker"
```

---

## Task 6: Android — Split `DeliveryRepository.submitPod()` Catch Block

**Files:**
- Modify: `apps/driver-app-android/feature/delivery/src/main/kotlin/io/logisticos/driver/feature/delivery/data/DeliveryRepository.kt`

- [ ] **Step 1: Replace the submit + catch section**

Find the section starting at `// 4. Submit POD` in `submitPod()`. Replace from that point to the end of the outer `catch` block with:

```kotlin
            // 4. Submit POD
            podApi.submit(podId, SubmitPodRequest(codCollectedCents = codCollectedCents, otpCode = otpCode))

            // POD is on the server. Mark it synced before attempting step 7
            // so a subsequent failure doesn't re-upload the same photo.
            podDao.markSynced(taskId)

            // Optimistic local task completion — isSynced=false until backend confirms.
            taskDao.updateStatusWithSync(taskId, TaskStatus.COMPLETED, isSynced = false)
            val shift = shiftDao.getActiveShiftOnce()
            if (shift != null) shiftDao.incrementCompleted(shift.id)

            try {
                // 5. Complete the task on the backend — links pod_id to the task.
                driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId, codCollectedCents = codCollectedCents))
                taskDao.markSynced(taskId)   // Backend confirmed — clear the pending badge.
            } catch (e: Exception) {
                // Only step 7 (task completion) failed. POD evidence is safe on the server.
                // Enqueue just the task completion for retry; do NOT replay all 7 steps.
                android.util.Log.w("DeliveryRepository", "completeTask failed, queuing TASK_COMPLETE: ${e.message}")
                enqueueAndKick(
                    SyncQueueEntity(
                        action = SyncAction.TASK_COMPLETE,
                        payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "podId" to podId)),
                        createdAt = System.currentTimeMillis()
                    )
                )
            }

            podId
        } catch (e: Exception) {
            // Steps 1–6 failed (POD not yet on server). Enqueue a full POD_SUBMIT retry.
            android.util.Log.e("DeliveryRepository", "submitPod failed: ${e.javaClass.simpleName}: ${e.message}", e)
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.POD_SUBMIT,
                    payloadJson = Json.encodeToString(mapOf("taskId" to taskId)),
                    createdAt = System.currentTimeMillis()
                )
            )
            throw e
        }
    }
```

**Remove** the old `podDao.markSynced` and `taskDao.updateStatus` and `shiftDao.incrementCompleted` lines that were previously at the end of the happy path inside the outer `try`, since they are now handled in the new structure above.

- [ ] **Step 2: Build to confirm**

```bash
cd apps/driver-app-android
./gradlew :feature:delivery:assembleDebug --no-daemon 2>&1 | tail -20
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/feature/delivery/src/main/kotlin/io/logisticos/driver/feature/delivery/data/DeliveryRepository.kt
git commit -m "feat(driver-app): split submitPod catch — TASK_COMPLETE retries only step 7"
```

---

## Task 7: Android — `NetworkConnectivityObserver` + Register in Application

**Files:**
- Create: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/NetworkConnectivityObserver.kt`
- Modify: `apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/DriverApplication.kt`

- [ ] **Step 1: Create `NetworkConnectivityObserver.kt`**

```kotlin
package io.logisticos.driver.core.network

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import androidx.core.content.getSystemService
import io.logisticos.driver.core.database.worker.OutboundSyncWorker

class NetworkConnectivityObserver(private val context: Context) {

    private val connectivityManager = context.getSystemService<ConnectivityManager>()!!

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            OutboundSyncWorker.kickOnce(context)
        }
    }

    fun register() {
        val request = NetworkRequest.Builder()
            .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .build()
        connectivityManager.registerNetworkCallback(request, callback)
    }

    fun unregister() {
        runCatching { connectivityManager.unregisterNetworkCallback(callback) }
    }
}
```

- [ ] **Step 2: Register in `DriverApplication.onCreate()`**

In `DriverApplication.kt`, add the field and register call. The full file becomes:

```kotlin
package io.logisticos.driver

import android.app.Application
import androidx.hilt.work.HiltWorkerFactory
import androidx.work.Configuration
import com.mapbox.common.MapboxOptions
import dagger.hilt.android.HiltAndroidApp
import io.logisticos.driver.core.network.NetworkConnectivityObserver
import javax.inject.Inject

@HiltAndroidApp
class DriverApplication : Application(), Configuration.Provider {

    @Inject lateinit var workerFactory: HiltWorkerFactory

    private lateinit var connectivityObserver: NetworkConnectivityObserver

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setWorkerFactory(workerFactory)
            .build()

    override fun onCreate() {
        super.onCreate()
        if (BuildConfig.MAPBOX_ACCESS_TOKEN.isNotEmpty()) {
            MapboxOptions.accessToken = BuildConfig.MAPBOX_ACCESS_TOKEN
        }
        connectivityObserver = NetworkConnectivityObserver(this)
        connectivityObserver.register()
    }
}
```

- [ ] **Step 3: Build to confirm**

```bash
cd apps/driver-app-android
./gradlew :app:assembleDebug --no-daemon 2>&1 | tail -30
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 4: Commit**

```bash
git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/NetworkConnectivityObserver.kt \
        apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/DriverApplication.kt
git commit -m "feat(driver-app): add NetworkConnectivityObserver — kick sync on reconnect"
```

---

## Task 8: Android — Pending Sync Badge in `RouteScreen`

**Files:**
- Modify: `apps/driver-app-android/feature/route/src/main/kotlin/io/logisticos/driver/feature/route/ui/RouteScreen.kt`

- [ ] **Step 1: Update `statusColor` to handle `FAILED_SYNC` and add badge in `TaskStopCardBody`**

Add the import for `CircleShape` at the top of the file with the other imports:

```kotlin
import androidx.compose.foundation.shape.CircleShape
```

Add the color constant at the top with the other color constants (after `private val Border = ...`):

```kotlin
private val Red = Color(0xFFFF4444)
```

In `TaskStopCardBody`, update the `statusColor` computation to handle `FAILED_SYNC`:

```kotlin
val statusColor = when (task.status) {
    TaskStatus.COMPLETED -> Green
    TaskStatus.ATTEMPTED, TaskStatus.FAILED -> Amber
    TaskStatus.EN_ROUTE, TaskStatus.ARRIVED, TaskStatus.IN_PROGRESS -> Cyan
    TaskStatus.FAILED_SYNC -> Red
    else -> Color.White.copy(alpha = 0.6f)
}
```

After the `Text(task.awb, ...)` line in `TaskStopCardBody`, add the sync badge:

```kotlin
Text(task.awb, color = statusColor, fontSize = 11.sp)

// Sync state badge — only visible when task is done locally but not yet confirmed by backend.
val syncBadgeColor: Color?
val syncBadgeLabel: String?
when {
    task.status == TaskStatus.FAILED_SYNC -> {
        syncBadgeColor = Red
        syncBadgeLabel = "Sync failed — contact support"
    }
    task.status == TaskStatus.COMPLETED && !task.isSynced -> {
        syncBadgeColor = Amber
        syncBadgeLabel = "Pending sync"
    }
    else -> {
        syncBadgeColor = null
        syncBadgeLabel = null
    }
}
if (syncBadgeColor != null && syncBadgeLabel != null) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier.padding(top = 2.dp),
    ) {
        Box(
            modifier = Modifier
                .size(6.dp)
                .background(syncBadgeColor, shape = CircleShape)
        )
        Text(syncBadgeLabel, color = syncBadgeColor, fontSize = 10.sp)
    }
}
```

- [ ] **Step 2: Build the full app to confirm no compose errors**

```bash
cd apps/driver-app-android
./gradlew assembleStagingDebug --no-daemon 2>&1 | tail -30
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/feature/route/src/main/kotlin/io/logisticos/driver/feature/route/ui/RouteScreen.kt
git commit -m "feat(driver-app): show pending sync / failed sync badge on route stop cards"
```

---

## Task 9: Final Build + CI Push

- [ ] **Step 1: Full app release build**

```bash
cd apps/driver-app-android
./gradlew assembleStagingRelease --no-daemon 2>&1 | tail -30
```

Expected: `BUILD SUCCESSFUL`. Fix any remaining compile errors.

- [ ] **Step 2: Push to trigger CI**

```bash
git push origin HEAD
```

CI will build `stagingDebug`, `stagingRelease`, and `prodRelease` APKs. Monitor the Actions run at `https://github.com/breakdisk/cm-app/actions`.

- [ ] **Step 3: Install the staging debug APK and smoke-test**

Download the `driver-app-staging-debug-*` artifact from the CI run. Install:

```bash
adb install -r app-staging-debug.apk
```

Test the fix:
1. Log in as a driver with an active shift
2. Navigate to a delivery stop → complete delivery with photo
3. While submitting, toggle airplane mode ON immediately after "Submit" tap
4. Observe route screen: completed task shows **amber "Pending sync" dot**
5. Toggle airplane mode OFF
6. Within 30 seconds, badge disappears (WorkManager fires on reconnect)
7. Check admin portal or query backend: `GET /v1/tasks/{id}` should show `status: completed` with a `pod_id`

- [ ] **Step 4: Verify backend validation**

```bash
# Replace TOKEN and TASK_ID with real values
curl -s -X PUT https://os-api.cargomarket.net/v1/tasks/TASK_ID/complete \
  -H "Authorization: Bearer TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}' | jq .
```

Expected: `{"error": "delivery task completion requires pod_id"}` with HTTP 422.

---

## Self-Review Checklist

- **Spec coverage:**
  - ✅ Split `POD_SUBMIT` → `POD_SUBMIT` + `TASK_COMPLETE` (Tasks 2, 5, 6)
  - ✅ `isSynced` on `TaskEntity` with Room migration (Tasks 3, 4)
  - ✅ `NetworkConnectivityObserver` → `kickOnce` on reconnect (Task 7)
  - ✅ Backend `pod_id` validation for delivery tasks (Task 1)
  - ✅ Amber "Pending sync" badge in route screen (Task 8)
  - ✅ Red "Sync failed" badge for `FAILED_SYNC` status (Task 8)
  - ✅ 7-day permanent failure in `TASK_COMPLETE` branch (Task 5)
  - ✅ `FAILED_SYNC` enum value (Task 2)

- **Type consistency:** `markSynced(taskId: String)`, `markSyncFailed(taskId: String)`, `updateStatusWithSync(taskId, status, isSynced)` — used consistently in Tasks 4, 5, 6.

- **Key invariant maintained:** `TASK_COMPLETE` is only enqueued after `podApi.submit()` succeeds — always carries a valid `podId`.
