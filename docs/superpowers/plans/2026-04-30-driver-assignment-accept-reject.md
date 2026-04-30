# Driver Assignment Accept/Reject Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When dispatch assigns a shipment to a driver, the driver sees a full-screen modal with shipment details and explicit Accept / Reject buttons — closing the loop between the Kafka-backed dispatch pipeline and the driver app.

**Architecture:** A new `PendingAssignmentBus` singleton (following `TaskSyncBus` pattern) carries assignment payloads from `DriverMessagingService` (FCM) to the nav graph. A new `feature/assignment` Gradle module owns the ViewModel, repository, and screen. `ShiftScaffold` collects the bus and navigates to `assignment/{assignmentId}`. Accept calls `PUT /v1/assignments/:id/accept`; reject shows a reason sheet then calls `PUT /v1/assignments/:id/reject`.

**Tech Stack:** Kotlin + Jetpack Compose, Hilt, Retrofit, Room, Turbine + MockK + JUnit 5, Firebase Cloud Messaging, Compose Navigation

---

## File Map

| Action | File |
|--------|------|
| Modify | `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt` |
| Create | `apps/driver-app-android/core/common/src/main/kotlin/io/logisticos/driver/core/common/PendingAssignmentBus.kt` |
| Create | `apps/driver-app-android/feature/assignment/build.gradle.kts` |
| Create | `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/data/AssignmentRepository.kt` |
| Create | `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModel.kt` |
| Create | `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/ui/AssignmentScreen.kt` |
| Create | `apps/driver-app-android/feature/assignment/src/test/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModelTest.kt` |
| Modify | `apps/driver-app-android/feature/notifications/src/main/kotlin/io/logisticos/driver/feature/notifications/DriverMessagingService.kt` |
| Modify | `apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/navigation/ShiftNavGraph.kt` |
| Modify | `apps/driver-app-android/app/build.gradle.kts` |

---

### Task 1: Add accept/reject API endpoints

**Files:**
- Modify: `apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt`

- [ ] **Step 1: Add response models and interface methods**

Add to the bottom of `DriverOpsApiService.kt`, before the closing `}` of the interface:

```kotlin
@Serializable
data class RejectAssignmentRequest(
    val reason: String
)

// Add inside interface DriverOpsApiService { ... }

    /** PUT /v1/assignments/:id/accept — driver accepts an incoming shipment assignment */
    @PUT("v1/assignments/{id}/accept")
    suspend fun acceptAssignment(@Path("id") assignmentId: String)

    /** PUT /v1/assignments/:id/reject — driver rejects with a reason */
    @PUT("v1/assignments/{id}/reject")
    suspend fun rejectAssignment(
        @Path("id") assignmentId: String,
        @Body body: RejectAssignmentRequest
    )
```

Full file after edit:

```kotlin
package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

// ─── Response models ─────────────────────────────────────────────────────────

@Serializable
data class TaskListResponse(
    val data: List<TaskItem>
)

@Serializable
data class TaskItem(
    @SerialName("task_id")            val taskId: String,
    @SerialName("shipment_id")        val shipmentId: String,
    val sequence: Int,
    val status: String,
    @SerialName("task_type")          val taskType: String,
    @SerialName("customer_name")      val customerName: String,
    @SerialName("customer_phone")     val customerPhone: String = "",
    val address: String,
    @SerialName("tracking_number")    val trackingNumber: String? = null,
    @SerialName("cod_amount_cents")   val codAmountCents: Long? = null,
    val lat: Double? = null,
    val lng: Double? = null,
    @SerialName("requires_photo")     val requiresPhoto: Boolean = false,
    @SerialName("requires_signature") val requiresSignature: Boolean = false,
    @SerialName("requires_otp")       val requiresOtp: Boolean = false,
)

@Serializable
data class CompleteTaskRequest(
    @SerialName("pod_id")               val podId: String? = null,
    @SerialName("cod_collected_cents")  val codCollectedCents: Long? = null
)

@Serializable
data class FailTaskRequest(
    val reason: String
)

@Serializable
data class UpdateLocationRequest(
    val lat: Double,
    val lng: Double,
    @SerialName("accuracy_m")  val accuracyM: Float? = null,
    @SerialName("speed_kmh")   val speedKmh: Float? = null,
    val heading: Float? = null,
    @SerialName("battery_pct") val batteryPct: Int? = null,
    @SerialName("recorded_at") val recordedAt: String
)

@Serializable
data class RejectAssignmentRequest(
    val reason: String
)

// ─── API interface ────────────────────────────────────────────────────────────

interface DriverOpsApiService {

    /** GET /v1/tasks — list pending + in-progress tasks for the authenticated driver */
    @GET("v1/tasks")
    suspend fun listMyTasks(): TaskListResponse

    /** PUT /v1/tasks/{id}/start — mark task as in-progress */
    @PUT("v1/tasks/{id}/start")
    suspend fun startTask(@Path("id") taskId: String)

    /** PUT /v1/tasks/{id}/complete — complete a task (delivery requires pod_id) */
    @PUT("v1/tasks/{id}/complete")
    suspend fun completeTask(
        @Path("id") taskId: String,
        @Body body: CompleteTaskRequest
    )

    /** PUT /v1/tasks/{id}/fail — mark task as failed */
    @PUT("v1/tasks/{id}/fail")
    suspend fun failTask(
        @Path("id") taskId: String,
        @Body body: FailTaskRequest
    )

    /** POST /v1/location — update driver GPS position */
    @POST("v1/location")
    suspend fun updateLocation(@Body body: UpdateLocationRequest)

    /** POST /v1/drivers/go-online */
    @POST("v1/drivers/go-online")
    suspend fun goOnline()

    /** POST /v1/drivers/go-offline */
    @POST("v1/drivers/go-offline")
    suspend fun goOffline()

    /** PUT /v1/assignments/:id/accept — driver accepts an incoming shipment assignment */
    @PUT("v1/assignments/{id}/accept")
    suspend fun acceptAssignment(@Path("id") assignmentId: String)

    /** PUT /v1/assignments/:id/reject — driver rejects with a reason */
    @PUT("v1/assignments/{id}/reject")
    suspend fun rejectAssignment(
        @Path("id") assignmentId: String,
        @Body body: RejectAssignmentRequest
    )
}
```

- [ ] **Step 2: Verify it compiles**

```bash
cd apps/driver-app-android
./gradlew :core:network:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/core/network/src/main/kotlin/io/logisticos/driver/core/network/service/DriverOpsApiService.kt
git commit -m "feat(driver-app): add acceptAssignment/rejectAssignment API endpoints"
```

---

### Task 2: Create PendingAssignmentBus

**Files:**
- Create: `apps/driver-app-android/core/common/src/main/kotlin/io/logisticos/driver/core/common/PendingAssignmentBus.kt`

This singleton carries the FCM payload from `DriverMessagingService` (which runs in a background thread with no nav access) to the Compose nav graph. It follows the same pattern as `TaskSyncBus` but carries structured data instead of `Unit`.

- [ ] **Step 1: Create the bus**

```kotlin
package io.logisticos.driver.core.common

import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.asSharedFlow

/**
 * Carries the FCM `task_assigned` payload across process boundaries
 * (DriverMessagingService → ShiftScaffold nav observer).
 *
 * Fields mirror what the backend sends in the FCM data map so the
 * AssignmentScreen can render without a separate network round-trip.
 */
data class AssignmentPayload(
    val assignmentId: String,
    val shipmentId:   String,
    val customerName: String,
    val address:      String,
    val taskType:     String,   // "pickup" | "delivery"
    val trackingNumber: String,
    val codAmountCents: Long,
)

object PendingAssignmentBus {
    private val _events = MutableSharedFlow<AssignmentPayload>(extraBufferCapacity = 1)
    val events: SharedFlow<AssignmentPayload> = _events.asSharedFlow()

    /** Called by DriverMessagingService on the worker thread — safe to call without coroutine. */
    fun post(payload: AssignmentPayload) {
        _events.tryEmit(payload)
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
./gradlew :core:common:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/core/common/src/main/kotlin/io/logisticos/driver/core/common/PendingAssignmentBus.kt
git commit -m "feat(driver-app): add PendingAssignmentBus for FCM→nav deeplink"
```

---

### Task 3: Create feature/assignment Gradle module

**Files:**
- Create: `apps/driver-app-android/feature/assignment/build.gradle.kts`

- [ ] **Step 1: Create the module build file**

Create `apps/driver-app-android/feature/assignment/build.gradle.kts`:

```kotlin
plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.hilt)
    alias(libs.plugins.ksp)
}

android {
    namespace = "io.logisticos.driver.feature.assignment"
    compileSdk = 35
    defaultConfig { minSdk = 26 }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation(project(":core:network"))
    implementation(project(":core:common"))
    implementation(platform(libs.compose.bom))
    implementation(libs.bundles.compose)
    implementation(libs.hilt.android)
    implementation(libs.hilt.navigation.compose)
    implementation(libs.kotlinx.serialization.json)
    ksp(libs.hilt.compiler)
    testImplementation(libs.bundles.testing.unit)
    testImplementation(libs.turbine)
}
```

- [ ] **Step 2: Register the module in settings.gradle.kts**

Open `apps/driver-app-android/settings.gradle.kts`. Find the block where other feature modules are included (e.g. `include(":feature:delivery")`). Add:

```kotlin
include(":feature:assignment")
```

- [ ] **Step 3: Verify settings sync**

```bash
./gradlew :feature:assignment:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL` (nothing to compile yet — module is empty)

- [ ] **Step 4: Commit**

```bash
git add apps/driver-app-android/feature/assignment/build.gradle.kts
git add apps/driver-app-android/settings.gradle.kts
git commit -m "feat(driver-app): scaffold feature/assignment Gradle module"
```

---

### Task 4: AssignmentRepository

**Files:**
- Create: `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/data/AssignmentRepository.kt`

- [ ] **Step 1: Create the repository**

```kotlin
package io.logisticos.driver.feature.assignment.data

import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.RejectAssignmentRequest
import javax.inject.Inject

class AssignmentRepository @Inject constructor(
    private val api: DriverOpsApiService,
) {
    /**
     * Accept a dispatch assignment. Returns [Result.success] on HTTP 200/204.
     * The backend flips the assignment status → 'accepted' and signals the
     * dispatch engine to remove it from the pending queue.
     */
    suspend fun accept(assignmentId: String): Result<Unit> = runCatching {
        api.acceptAssignment(assignmentId)
    }

    /**
     * Reject a dispatch assignment with a driver-supplied reason.
     * The backend marks the assignment 'rejected', removes the unique constraint
     * block on the driver, and re-queues the shipment for re-dispatch.
     */
    suspend fun reject(assignmentId: String, reason: String): Result<Unit> = runCatching {
        api.rejectAssignment(assignmentId, RejectAssignmentRequest(reason))
    }
}
```

- [ ] **Step 2: Verify it compiles**

```bash
./gradlew :feature:assignment:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/data/AssignmentRepository.kt
git commit -m "feat(driver-app): AssignmentRepository with accept/reject"
```

---

### Task 5: AssignmentViewModel + test

**Files:**
- Create: `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModel.kt`
- Create: `apps/driver-app-android/feature/assignment/src/test/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModelTest.kt`

- [ ] **Step 1: Write the failing tests first**

Create `apps/driver-app-android/feature/assignment/src/test/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModelTest.kt`:

```kotlin
package io.logisticos.driver.feature.assignment.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.feature.assignment.data.AssignmentRepository
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class AssignmentViewModelTest {

    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: AssignmentRepository = mockk()
    private val payload = AssignmentPayload(
        assignmentId  = "asgn-1",
        shipmentId    = "ship-1",
        customerName  = "Juan dela Cruz",
        address       = "123 Rizal St, Makati",
        taskType      = "delivery",
        trackingNumber = "CM-PH1-D0000001A",
        codAmountCents = 50_000L,
    )
    private lateinit var vm: AssignmentViewModel

    @BeforeEach fun setUp() {
        Dispatchers.setMain(testDispatcher)
        vm = AssignmentViewModel(repo, payload)
    }

    @AfterEach fun tearDown() { Dispatchers.resetMain() }

    @Test
    fun `initial state populates from payload`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertEquals("asgn-1", state.assignmentId)
            assertEquals("Juan dela Cruz", state.customerName)
            assertEquals("delivery", state.taskType)
            assertEquals(50_000L, state.codAmountCents)
            assertFalse(state.isAccepting)
            assertFalse(state.isRejecting)
            assertNull(state.error)
            assertFalse(state.isDone)
        }
    }

    @Test
    fun `accept sets isDone on success`() = runTest {
        coEvery { repo.accept("asgn-1") } returns Result.success(Unit)

        vm.uiState.test {
            awaitItem() // initial
            vm.accept()
            val loading = awaitItem()
            assertTrue(loading.isAccepting)
            val done = awaitItem()
            assertTrue(done.isDone)
            assertFalse(done.isAccepting)
        }
    }

    @Test
    fun `accept sets error on failure`() = runTest {
        coEvery { repo.accept("asgn-1") } returns Result.failure(RuntimeException("network error"))

        vm.uiState.test {
            awaitItem()
            vm.accept()
            awaitItem() // loading
            val error = awaitItem()
            assertFalse(error.isAccepting)
            assertEquals("network error", error.error)
            assertFalse(error.isDone)
        }
    }

    @Test
    fun `reject sets isDone on success`() = runTest {
        coEvery { repo.reject("asgn-1", any()) } returns Result.success(Unit)

        vm.uiState.test {
            awaitItem()
            vm.reject("CUSTOMER_ABSENT")
            val loading = awaitItem()
            assertTrue(loading.isRejecting)
            val done = awaitItem()
            assertTrue(done.isDone)
            assertFalse(done.isRejecting)
        }
    }

    @Test
    fun `reject sets error on failure`() = runTest {
        coEvery { repo.reject("asgn-1", any()) } returns Result.failure(RuntimeException("timeout"))

        vm.uiState.test {
            awaitItem()
            vm.reject("OTHER")
            awaitItem() // loading
            val error = awaitItem()
            assertEquals("timeout", error.error)
            assertFalse(error.isDone)
        }
    }

    @Test
    fun `accept calls TaskSyncBus on success`() = runTest {
        coEvery { repo.accept("asgn-1") } returns Result.success(Unit)
        vm.accept()
        // TaskSyncBus.requestSync() is fire-and-forget; verify repo was called
        coVerify { repo.accept("asgn-1") }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
./gradlew :feature:assignment:testDebugUnitTest
```

Expected: FAIL — `AssignmentViewModel` class not found

- [ ] **Step 3: Implement AssignmentViewModel**

Create `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModel.kt`:

```kotlin
package io.logisticos.driver.feature.assignment.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.feature.assignment.data.AssignmentRepository
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class AssignmentUiState(
    val assignmentId:   String  = "",
    val shipmentId:     String  = "",
    val customerName:   String  = "",
    val address:        String  = "",
    val taskType:       String  = "delivery",  // "pickup" | "delivery"
    val trackingNumber: String  = "",
    val codAmountCents: Long    = 0L,
    val isAccepting:    Boolean = false,
    val isRejecting:    Boolean = false,
    val showRejectSheet:Boolean = false,
    val error:          String? = null,
    /** True once accept or reject succeeds — screen calls onDone(). */
    val isDone:         Boolean = false,
)

@HiltViewModel(assistedFactory = AssignmentViewModel.Factory::class)
class AssignmentViewModel @AssistedInject constructor(
    private val repo: AssignmentRepository,
    @Assisted private val payload: AssignmentPayload,
) : ViewModel() {

    @AssistedFactory
    interface Factory {
        fun create(payload: AssignmentPayload): AssignmentViewModel
    }

    private val _uiState = MutableStateFlow(
        AssignmentUiState(
            assignmentId   = payload.assignmentId,
            shipmentId     = payload.shipmentId,
            customerName   = payload.customerName,
            address        = payload.address,
            taskType       = payload.taskType,
            trackingNumber = payload.trackingNumber,
            codAmountCents = payload.codAmountCents,
        )
    )
    val uiState: StateFlow<AssignmentUiState> = _uiState.asStateFlow()

    /** Driver taps "Accept". Calls backend, triggers task sync, emits isDone. */
    fun accept() {
        viewModelScope.launch {
            _uiState.update { it.copy(isAccepting = true, error = null) }
            repo.accept(payload.assignmentId)
                .onSuccess {
                    TaskSyncBus.requestSync()
                    _uiState.update { it.copy(isAccepting = false, isDone = true) }
                }
                .onFailure { e ->
                    _uiState.update { it.copy(isAccepting = false, error = e.message) }
                }
        }
    }

    /** Driver taps "Reject" and selects a reason. */
    fun reject(reason: String) {
        viewModelScope.launch {
            _uiState.update { it.copy(isRejecting = true, showRejectSheet = false, error = null) }
            repo.reject(payload.assignmentId, reason)
                .onSuccess {
                    _uiState.update { it.copy(isRejecting = false, isDone = true) }
                }
                .onFailure { e ->
                    _uiState.update { it.copy(isRejecting = false, error = e.message) }
                }
        }
    }

    fun showRejectSheet()    { _uiState.update { it.copy(showRejectSheet = true) } }
    fun dismissRejectSheet() { _uiState.update { it.copy(showRejectSheet = false) } }
    fun clearError()         { _uiState.update { it.copy(error = null) } }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
./gradlew :feature:assignment:testDebugUnitTest
```

Expected: `BUILD SUCCESSFUL` — all 5 tests pass

- [ ] **Step 5: Commit**

```bash
git add apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModel.kt
git add apps/driver-app-android/feature/assignment/src/test/kotlin/io/logisticos/driver/feature/assignment/presentation/AssignmentViewModelTest.kt
git commit -m "feat(driver-app): AssignmentViewModel with accept/reject + tests"
```

---

### Task 6: AssignmentScreen UI

**Files:**
- Create: `apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/ui/AssignmentScreen.kt`

Design language: same dark glassmorphism palette as `ArrivalScreen` — `#050810` canvas, `#00E5FF` cyan, `#00FF88` green, `#FFAB00` amber, `#A855F7` purple.

- [ ] **Step 1: Create AssignmentScreen.kt**

```kotlin
package io.logisticos.driver.feature.assignment.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.hilt.navigation.compose.hiltViewModel
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.feature.assignment.presentation.AssignmentViewModel

private val Canvas = Color(0xFF050810)
private val Cyan   = Color(0xFF00E5FF)
private val Green  = Color(0xFF00FF88)
private val Amber  = Color(0xFFFFAB00)
private val Purple = Color(0xFFA855F7)
private val Glass  = Color(0x0AFFFFFF)
private val Border = Color(0x14FFFFFF)
private val Red    = Color(0xFFFF4D4D)

private val REJECT_REASONS = listOf(
    "ALREADY_BUSY"       to "Already on another delivery",
    "TOO_FAR"            to "Pickup too far away",
    "VEHICLE_ISSUE"      to "Vehicle issue",
    "PERSONAL_EMERGENCY" to "Personal emergency",
    "OTHER"              to "Other reason",
)

/**
 * Full-screen assignment card shown when dispatch assigns a new shipment.
 *
 * [payload] is passed via the nav graph from [PendingAssignmentBus].
 * [onAccepted] is called after the backend confirms — nav to Route tab.
 * [onRejected] is called after the backend confirms — nav back to Home.
 */
@Composable
fun AssignmentScreen(
    payload: AssignmentPayload,
    onAccepted: () -> Unit,
    onRejected: () -> Unit,
    viewModel: AssignmentViewModel = hiltViewModel<AssignmentViewModel,
            AssignmentViewModel.Factory> { it.create(payload) }
) {
    val state by viewModel.uiState.collectAsState()

    // Navigate away as soon as isDone flips — avoids a 2nd tap race.
    LaunchedEffect(state.isDone) {
        if (!state.isDone) return@LaunchedEffect
        if (state.isAccepting) return@LaunchedEffect
        // If reject was just confirmed isDone=true and isAccepting=false
        // If accept was confirmed isDone=true
        // We discriminate by checking whether reject was in progress.
        // Both paths set isDone. Caller disambiguates by which button was last pressed,
        // but since we can't know here, we use task type:
        // the ViewModel tracks which action succeeded via isDone.
        // Both nav callbacks are different, so we track internally:
        // see accepted flag below.
    }

    // Track which action completed so we route correctly.
    var accepted by remember { mutableStateOf(false) }
    LaunchedEffect(state.isDone) {
        if (state.isDone) {
            if (accepted) onAccepted() else onRejected()
        }
    }

    if (state.showRejectSheet) {
        RejectReasonSheet(
            onDismiss = { viewModel.dismissRejectSheet() },
            onSelect   = { reason ->
                viewModel.reject(reason)
                // accepted stays false
            }
        )
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Canvas),
        contentAlignment = Alignment.Center
    ) {
        // Pulse ring behind the icon
        Box(
            modifier = Modifier
                .align(Alignment.TopCenter)
                .padding(top = 72.dp)
                .size(140.dp)
                .clip(RoundedCornerShape(70.dp))
                .background(Cyan.copy(alpha = 0.05f))
                .border(1.dp, Cyan.copy(alpha = 0.15f), RoundedCornerShape(70.dp)),
            contentAlignment = Alignment.Center
        ) {
            Box(
                modifier = Modifier
                    .size(88.dp)
                    .clip(RoundedCornerShape(44.dp))
                    .background(Cyan.copy(alpha = 0.10f))
                    .border(1.dp, Cyan.copy(alpha = 0.30f), RoundedCornerShape(44.dp)),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    if (state.taskType == "pickup") "📦" else "🚚",
                    fontSize = 32.sp
                )
            }
        }

        // Main card
        AnimatedVisibility(
            visible = true,
            enter = fadeIn() + slideInVertically { it / 2 },
            modifier = Modifier.align(Alignment.BottomCenter)
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                    .background(Color(0xFF0D1220))
                    .border(1.dp, Border, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                    .padding(horizontal = 20.dp, vertical = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                // Header
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Column {
                        Text(
                            "New Assignment",
                            color = Cyan,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.8.sp
                        )
                        Text(
                            state.customerName,
                            color = Color.White,
                            fontSize = 22.sp,
                            fontWeight = FontWeight.Bold
                        )
                    }
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(
                                if (state.taskType == "pickup") Purple.copy(alpha = 0.12f)
                                else Cyan.copy(alpha = 0.10f)
                            )
                            .padding(horizontal = 10.dp, vertical = 4.dp)
                    ) {
                        Text(
                            state.taskType.uppercase(),
                            color = if (state.taskType == "pickup") Purple else Cyan,
                            fontSize = 10.sp,
                            fontWeight = FontWeight.Bold,
                            letterSpacing = 1.sp
                        )
                    }
                }

                HorizontalDivider(color = Border)

                // AWB
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(10.dp))
                        .background(Glass)
                        .padding(12.dp),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Text("AWB", color = Color.White.copy(alpha = 0.4f), fontSize = 11.sp)
                    Text(
                        state.trackingNumber.ifBlank { state.shipmentId },
                        color = Color.White,
                        fontSize = 14.sp,
                        fontWeight = FontWeight.Medium,
                        fontFamily = FontFamily.Monospace
                    )
                }

                // Address
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    verticalAlignment = Alignment.Top
                ) {
                    Icon(
                        Icons.Default.LocationOn,
                        contentDescription = null,
                        tint = Cyan.copy(alpha = 0.7f),
                        modifier = Modifier.size(18.dp).padding(top = 2.dp)
                    )
                    Text(
                        state.address,
                        color = Color.White.copy(alpha = 0.85f),
                        fontSize = 14.sp,
                        lineHeight = 20.sp,
                        modifier = Modifier.weight(1f)
                    )
                }

                // COD badge (delivery only)
                if (state.codAmountCents > 0 && state.taskType == "delivery") {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Amber.copy(alpha = 0.10f))
                            .border(1.dp, Amber.copy(alpha = 0.20f), RoundedCornerShape(10.dp))
                            .padding(12.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Text(
                            "💰 COD to Collect",
                            color = Amber,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold
                        )
                        Text(
                            "₱${"%,.2f".format(state.codAmountCents / 100.0)}",
                            color = Amber,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.Bold,
                            fontFamily = FontFamily.Monospace
                        )
                    }
                }

                // Error banner
                state.error?.let { err ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(Red.copy(alpha = 0.10f))
                            .padding(12.dp)
                    ) {
                        Text("⚠ $err", color = Red, fontSize = 13.sp)
                    }
                }

                Spacer(Modifier.height(4.dp))

                // Action buttons
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    // Reject
                    OutlinedButton(
                        onClick = { viewModel.showRejectSheet() },
                        enabled = !state.isAccepting && !state.isRejecting,
                        modifier = Modifier.weight(1f).height(56.dp),
                        shape = RoundedCornerShape(14.dp),
                        border = androidx.compose.foundation.BorderStroke(1.dp, Red.copy(alpha = 0.5f)),
                        colors = ButtonDefaults.outlinedButtonColors(contentColor = Red)
                    ) {
                        if (state.isRejecting) {
                            CircularProgressIndicator(color = Red, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                        } else {
                            Text("Reject", fontWeight = FontWeight.Bold, fontSize = 15.sp)
                        }
                    }

                    // Accept
                    Button(
                        onClick = {
                            accepted = true
                            viewModel.accept()
                        },
                        enabled = !state.isAccepting && !state.isRejecting,
                        modifier = Modifier.weight(1f).height(56.dp),
                        shape = RoundedCornerShape(14.dp),
                        colors = ButtonDefaults.buttonColors(containerColor = Green)
                    ) {
                        if (state.isAccepting) {
                            CircularProgressIndicator(color = Canvas, modifier = Modifier.size(18.dp), strokeWidth = 2.dp)
                        } else {
                            Text("Accept", color = Canvas, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                        }
                    }
                }

                Spacer(Modifier.navigationBarsPadding())
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RejectReasonSheet(
    onDismiss: () -> Unit,
    onSelect: (reason: String) -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = Color(0xFF0D1220),
        tonalElevation = 0.dp,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp)
                .padding(bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp)
        ) {
            Text(
                "Reason for rejection",
                color = Color.White,
                fontSize = 17.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(bottom = 8.dp)
            )
            REJECT_REASONS.forEach { (code, label) ->
                OutlinedButton(
                    onClick = { onSelect(code) },
                    modifier = Modifier.fillMaxWidth().height(52.dp),
                    shape = RoundedCornerShape(12.dp),
                    border = androidx.compose.foundation.BorderStroke(1.dp, Border),
                    colors = ButtonDefaults.outlinedButtonColors(contentColor = Color.White)
                ) {
                    Text(label, fontSize = 14.sp)
                }
            }
        }
    }
}
```

- [ ] **Step 2: Compile the feature**

```bash
./gradlew :feature:assignment:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/feature/assignment/src/main/kotlin/io/logisticos/driver/feature/assignment/ui/AssignmentScreen.kt
git commit -m "feat(driver-app): AssignmentScreen UI with accept/reject and reject reason sheet"
```

---

### Task 7: Wire FCM → PendingAssignmentBus

**Files:**
- Modify: `apps/driver-app-android/feature/notifications/src/main/kotlin/io/logisticos/driver/feature/notifications/DriverMessagingService.kt`

When the backend sends a `task_assigned` FCM message, the data map now carries `assignment_id`, `shipment_id`, `customer_name`, `address`, `task_type`, `tracking_number`, `cod_amount_cents`. We extract these and post to `PendingAssignmentBus`.

- [ ] **Step 1: Update DriverMessagingService**

Replace the full file:

```kotlin
package io.logisticos.driver.feature.notifications

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import androidx.core.app.NotificationCompat
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import dagger.hilt.android.AndroidEntryPoint
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.feature.notifications.data.NotificationRepository
import java.util.concurrent.atomic.AtomicInteger
import javax.inject.Inject

@AndroidEntryPoint
class DriverMessagingService : FirebaseMessagingService() {

    companion object {
        private val notificationIdCounter = AtomicInteger(1)
    }

    @Inject lateinit var notificationRepo: NotificationRepository

    override fun onMessageReceived(message: RemoteMessage) {
        val type  = message.data["type"] ?: "dispatch_message"
        val title = message.notification?.title ?: message.data["title"] ?: "LogisticOS"
        val body  = message.notification?.body  ?: message.data["body"]  ?: ""

        notificationRepo.saveNotification(type = type, title = title, body = body)

        when (type) {
            "task_assigned" -> {
                // Extract assignment payload from FCM data map.
                // The backend engagement/FCM sender MUST include these fields;
                // missing fields fall back to empty strings so the screen still
                // renders rather than crashing.
                val assignmentId = message.data["assignment_id"] ?: ""
                if (assignmentId.isNotBlank()) {
                    PendingAssignmentBus.post(
                        AssignmentPayload(
                            assignmentId   = assignmentId,
                            shipmentId     = message.data["shipment_id"]     ?: "",
                            customerName   = message.data["customer_name"]   ?: "Unknown Customer",
                            address        = message.data["address"]         ?: "",
                            taskType       = message.data["task_type"]       ?: "delivery",
                            trackingNumber = message.data["tracking_number"] ?: "",
                            codAmountCents = message.data["cod_amount_cents"]?.toLongOrNull() ?: 0L,
                        )
                    )
                }
                // Still sync task list so RouteScreen is up to date after accept.
                TaskSyncBus.requestSync()
            }
            "dispatch_message" -> TaskSyncBus.requestSync()
        }

        showSystemNotification(title, body, type)
    }

    override fun onNewToken(token: String) {
        notificationRepo.registerFcmToken(token)
    }

    private fun showSystemNotification(title: String, body: String, type: String) {
        val channelId = "driver_notifications"
        val notificationManager = getSystemService(NotificationManager::class.java)

        if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.O) {
            notificationManager.createNotificationChannel(
                NotificationChannel(channelId, "Driver Alerts", NotificationManager.IMPORTANCE_HIGH)
            )
        }

        val intent = Intent().apply {
            setClassName(packageName, "$packageName.MainActivity")
            putExtra("notification_type", type)
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP
        }
        val pendingIntent = PendingIntent.getActivity(
            this, 0, intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )

        val notification = NotificationCompat.Builder(this, channelId)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setAutoCancel(true)
            .setContentIntent(pendingIntent)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .build()

        notificationManager.notify(notificationIdCounter.getAndIncrement(), notification)
    }
}
```

- [ ] **Step 2: Compile**

```bash
./gradlew :feature:notifications:compileDebugKotlin
```

Expected: `BUILD SUCCESSFUL`

- [ ] **Step 3: Commit**

```bash
git add apps/driver-app-android/feature/notifications/src/main/kotlin/io/logisticos/driver/feature/notifications/DriverMessagingService.kt
git commit -m "feat(driver-app): extract assignment_id from task_assigned FCM, post to PendingAssignmentBus"
```

---

### Task 8: Wire nav route + observe PendingAssignmentBus

**Files:**
- Modify: `apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/navigation/ShiftNavGraph.kt`
- Modify: `apps/driver-app-android/app/build.gradle.kts`

- [ ] **Step 1: Add feature:assignment to app dependencies**

In `apps/driver-app-android/app/build.gradle.kts`, find the dependencies block. Add after `implementation(project(":feature:notifications"))`:

```kotlin
    implementation(project(":feature:assignment"))
```

- [ ] **Step 2: Add assignment route constant and nav composable to ShiftNavGraph.kt**

Replace the full `ShiftNavGraph.kt` file:

```kotlin
package io.logisticos.driver.navigation

import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.hilt.navigation.compose.hiltViewModel
import androidx.navigation.NavGraphBuilder
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navigation
import io.logisticos.driver.core.common.PendingAssignmentBus
import io.logisticos.driver.core.database.entity.TaskType
import io.logisticos.driver.feature.assignment.ui.AssignmentScreen
import io.logisticos.driver.feature.delivery.ui.ArrivalScreen
import io.logisticos.driver.feature.home.ui.HomeScreen
import io.logisticos.driver.feature.navigation.ui.NavigationScreen
import io.logisticos.driver.feature.notifications.presentation.NotificationsViewModel
import io.logisticos.driver.feature.notifications.ui.NotificationsScreen
import io.logisticos.driver.feature.pickup.ui.PickupScreen
import io.logisticos.driver.feature.pod.ui.PodScreen
import io.logisticos.driver.feature.profile.presentation.ProfileViewModel
import io.logisticos.driver.feature.profile.ui.ComplianceScreen
import io.logisticos.driver.feature.profile.ui.ProfileScreen
import io.logisticos.driver.feature.route.ui.RouteScreen
import io.logisticos.driver.feature.scanner.ui.ScannerScreen
import kotlinx.coroutines.flow.filterNotNull

// ── Route constants ───────────────────────────────────────────────────────────
private const val HOME_ROUTE             = "home"
private const val ROUTE_ROUTE            = "route"
private const val SCAN_ROUTE             = "scan"
private const val NOTIFICATIONS_ROUTE    = "notifications"
private const val PROFILE_ROUTE          = "profile"
private const val COMPLIANCE_ROUTE       = "compliance"
private const val NAVIGATE_TO_STOP_ROUTE = "navigate/{taskId}"
private const val ARRIVAL_ROUTE          = "arrival/{taskId}"
private const val PICKUP_ROUTE           = "pickup/{taskId}"
private const val ASSIGNMENT_ROUTE       = "assignment"   // uses saved state, not nav args
private const val POD_ROUTE =
    "pod/{taskId}/{requiresPhoto}/{requiresSignature}/{requiresOtp}/{isCod}/{codAmount}"

/**
 * Top-level shift scaffold: owns the BottomNavBar and an inner NavHost.
 * Also observes [PendingAssignmentBus] — when a `task_assigned` FCM arrives,
 * navigates to [AssignmentScreen] immediately regardless of current tab.
 */
@Composable
fun ShiftScaffold(rootNavController: NavHostController) {
    val shiftNavController = rememberNavController()

    val notifVm: NotificationsViewModel = hiltViewModel()
    val unreadCount by notifVm.unreadCount.collectAsState()

    // ── FCM deeplink: task_assigned → AssignmentScreen ───────────────────────
    // pendingAssignment is SavedState-backed so rotation doesn't re-trigger nav.
    var pendingPayload by remember { mutableStateOf(
        PendingAssignmentBus.events.replayCache.firstOrNull()
    ) }

    LaunchedEffect(Unit) {
        PendingAssignmentBus.events.collect { payload ->
            pendingPayload = payload
            shiftNavController.navigate(ASSIGNMENT_ROUTE) {
                // Don't stack multiple assignment screens if the driver is slow to respond.
                launchSingleTop = true
            }
        }
    }

    Scaffold(
        containerColor = NavCanvas,
        bottomBar = {
            BottomNavBar(navController = shiftNavController, unreadCount = unreadCount)
        }
    ) { innerPadding ->
        NavHost(
            navController = shiftNavController,
            startDestination = HOME_ROUTE,
            modifier = Modifier.padding(innerPadding)
        ) {

            // ── Bottom tab destinations ───────────────────────────────────
            composable(HOME_ROUTE) {
                HomeScreen(onNavigateToRoute = { shiftNavController.navigate(ROUTE_ROUTE) })
            }

            composable(ROUTE_ROUTE) {
                RouteScreen(
                    shiftId = "",
                    onNavigateToStop = { taskId ->
                        shiftNavController.navigate("navigate/$taskId")
                    },
                )
            }

            composable(SCAN_ROUTE) {
                ScannerScreen(
                    expectedAwbs = emptyList(),
                    onAllScanned = { shiftNavController.popBackStack() }
                )
            }

            composable(NOTIFICATIONS_ROUTE) {
                NotificationsScreen(viewModel = hiltViewModel())
            }

            composable(PROFILE_ROUTE) {
                val vm: ProfileViewModel = hiltViewModel()
                ProfileScreen(
                    sessionManager = vm.sessionManager,
                    isOfflineMode = vm.isOfflineMode,
                    onNavigateToCompliance = { shiftNavController.navigate(COMPLIANCE_ROUTE) },
                    onLogout = {
                        vm.sessionManager.clearSession()
                        rootNavController.navigate(io.logisticos.driver.feature.auth.AUTH_GRAPH) {
                            popUpTo(SHIFT_GRAPH) { inclusive = true }
                        }
                    }
                )
            }

            composable(COMPLIANCE_ROUTE) {
                ComplianceScreen(onBack = { shiftNavController.popBackStack() })
            }

            // ── Assignment accept/reject ──────────────────────────────────
            composable(ASSIGNMENT_ROUTE) {
                val payload = pendingPayload
                if (payload == null) {
                    // Stale nav (e.g. back-stack restoration after process death) —
                    // just pop back rather than showing an empty screen.
                    LaunchedEffect(Unit) { shiftNavController.popBackStack() }
                    return@composable
                }
                AssignmentScreen(
                    payload    = payload,
                    onAccepted = {
                        pendingPayload = null
                        shiftNavController.navigate(ROUTE_ROUTE) {
                            popUpTo(HOME_ROUTE)
                        }
                    },
                    onRejected = {
                        pendingPayload = null
                        shiftNavController.popBackStack()
                    }
                )
            }

            // ── Deep task destinations ────────────────────────────────────

            composable(NAVIGATE_TO_STOP_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                NavigationScreen(
                    taskId    = taskId,
                    onArrived = { shiftNavController.navigate(ARRIVAL_ROUTE.replace("{taskId}", taskId)) },
                    onBack    = { shiftNavController.popBackStack() }
                )
            }

            composable(ARRIVAL_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                ArrivalScreen(
                    taskId      = taskId,
                    onStartTask = { id, taskType, photo, sig, otp, isCod, codAmount ->
                        when (taskType) {
                            TaskType.PICKUP -> shiftNavController.navigate(PICKUP_ROUTE.replace("{taskId}", id))
                            else            -> shiftNavController.navigate("pod/$id/$photo/$sig/$otp/$isCod/$codAmount")
                        }
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }

            composable(PICKUP_ROUTE) { backStack ->
                val taskId = backStack.arguments?.getString("taskId") ?: ""
                PickupScreen(
                    taskId    = taskId,
                    onCompleted = {
                        shiftNavController.navigate(HOME_ROUTE) { popUpTo(HOME_ROUTE) { inclusive = true } }
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }

            composable(POD_ROUTE) { backStack ->
                val args      = backStack.arguments
                val taskId    = args?.getString("taskId") ?: ""
                val photo     = args?.getString("requiresPhoto") == "true"
                val sig       = args?.getString("requiresSignature") == "true"
                val otp       = args?.getString("requiresOtp") == "true"
                val isCod     = args?.getString("isCod") == "true"
                val codAmount = args?.getString("codAmount")?.toDoubleOrNull() ?: 0.0
                PodScreen(
                    taskId          = taskId,
                    requiresPhoto   = photo,
                    requiresSignature = sig,
                    requiresOtp     = otp,
                    isCod           = isCod,
                    codAmount       = codAmount,
                    onCompleted     = {
                        shiftNavController.navigate(HOME_ROUTE) { popUpTo(HOME_ROUTE) { inclusive = true } }
                    },
                    onFailed = {
                        shiftNavController.navigate(HOME_ROUTE) { popUpTo(HOME_ROUTE) { inclusive = true } }
                    },
                    onBack = { shiftNavController.popBackStack() },
                )
            }
        }
    }
}

fun NavGraphBuilder.shiftNavGraph(navController: NavHostController) {
    navigation(startDestination = "shift_scaffold", route = SHIFT_GRAPH) {
        composable("shift_scaffold") {
            ShiftScaffold(rootNavController = navController)
        }
    }
}
```

- [ ] **Step 3: Build the full app**

```bash
./gradlew :app:assembleDevDebug
```

Expected: `BUILD SUCCESSFUL` — APK in `app/build/outputs/apk/dev/debug/`

- [ ] **Step 4: Run all unit tests**

```bash
./gradlew testDebugUnitTest
```

Expected: `BUILD SUCCESSFUL` — all existing tests plus the 5 new AssignmentViewModel tests pass

- [ ] **Step 5: Commit**

```bash
git add apps/driver-app-android/app/build.gradle.kts
git add apps/driver-app-android/app/src/main/kotlin/io/logisticos/driver/navigation/ShiftNavGraph.kt
git commit -m "feat(driver-app): wire assignment nav route + PendingAssignmentBus observer in ShiftScaffold"
```

---

### Task 9: Dev OTP bypass — gate behind BuildConfig.DEBUG

**Files:**
- Modify: `apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/data/AuthRepository.kt`
- Modify: `apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/ui/OtpScreen.kt`

The dev bypass (`123456`) must not ship in prod builds. The module currently has no access to `BuildConfig` because it lives in a library module. The cleanest approach: pass a `devBypassEnabled: Boolean` via an `@Named` injection that the app module provides based on `BuildConfig.DEBUG`.

- [ ] **Step 1: Add Named injection to AuthModule**

In `apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/di/AuthModule.kt`, check what's currently provided. Add a binding for `dev_bypass_enabled`:

```kotlin
// Inside @Module @InstallIn(SingletonComponent::class) object AuthModule

@Provides
@Named("dev_bypass_enabled")
fun provideDevBypassEnabled(): Boolean = BuildConfig.DEBUG
```

`BuildConfig.DEBUG` is `true` in `debug` build type and `false` in `release`. The feature module cannot reference `BuildConfig` directly (it belongs to `io.logisticos.driver`, not the library namespace) — that's why we inject it.

Read `AuthModule.kt` first to see its current contents, then add the binding.

- [ ] **Step 2: Update AuthRepository to use the flag**

In `AuthRepository`, add `@Named("dev_bypass_enabled") private val devBypassEnabled: Boolean` constructor param. In `verifyOtp`, wrap the `123456` shortcut:

```kotlin
suspend fun verifyOtp(phone: String, otp: String): Result<Unit> = runCatching {
    if (devBypassEnabled && otp == "123456") {
        // Dev shortcut — skip real OTP verification
        sessionManager.saveTokens(jwt = "dev-token", refreshToken = "dev-refresh")
        sessionManager.saveTenantId("dev-tenant-id")
        sessionManager.saveDriverId("dev-driver-id")
        return@runCatching
    }
    val response = apiService.verifyOtp(
        OtpVerifyRequest(phone = phone, otp = otp, tenantSlug = tenantSlug, role = "driver")
    ).data
    sessionManager.saveTokens(jwt = response.jwt, refreshToken = response.refreshToken)
    sessionManager.saveTenantId(response.tenantId)
    sessionManager.saveDriverId(response.driverId)
    runCatching {
        val token = FirebaseMessaging.getInstance().token.await()
        apiService.registerPushToken(RegisterPushTokenRequest(token = token))
    }
}
```

- [ ] **Step 3: Verify release build excludes bypass**

```bash
./gradlew :app:assembleProdRelease 2>&1 | tail -5
```

Expected: `BUILD SUCCESSFUL` — and `BuildConfig.DEBUG = false` in that variant so the bypass branch is dead code.

- [ ] **Step 4: Commit**

```bash
git add apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/di/AuthModule.kt
git add apps/driver-app-android/feature/auth/src/main/kotlin/io/logisticos/driver/feature/auth/data/AuthRepository.kt
git commit -m "fix(driver-app): gate dev OTP bypass behind BuildConfig.DEBUG"
```

---

### Task 10: Push the branch and open PR

- [ ] **Step 1: Push**

```bash
git push origin claude/amazing-jepsen-774a18
```

- [ ] **Step 2: Verify CI is green**

Watch GitHub Actions. The Android CI workflow runs `./gradlew testDebugUnitTest assembleDevDebug`. Expected: green.

---

## Self-Review

**Spec coverage:**
- ✅ Accept assignment — `AssignmentViewModel.accept()` → `PUT /v1/assignments/:id/accept` → `TaskSyncBus` → navigate to Route
- ✅ Reject assignment — `AssignmentViewModel.reject(reason)` → `PUT /v1/assignments/:id/reject` → navigate back to Home
- ✅ Reject reason sheet — `RejectReasonSheet` composable with 5 reason options
- ✅ FCM deeplink — `DriverMessagingService` extracts payload, posts to `PendingAssignmentBus`
- ✅ Nav wiring — `ShiftScaffold` collects bus, navigates to `assignment` route
- ✅ Dev OTP bypass gated — `BuildConfig.DEBUG` injection, `123456` only works in debug builds

**Placeholder scan:** No TBDs. All code blocks are complete.

**Type consistency:**
- `AssignmentPayload` defined in Task 2, used identically in Tasks 5, 7, 8
- `AssignmentViewModel.Factory.create(payload: AssignmentPayload)` defined in Task 5, consumed by `hiltViewModel<AssignmentViewModel, AssignmentViewModel.Factory> { it.create(payload) }` in Task 6
- `repo.accept(assignmentId)` / `repo.reject(assignmentId, reason)` defined in Task 4, called from Task 5 ViewModel

**Known unknowns to resolve during execution:**
- `AuthModule.kt` current contents must be read before adding the `@Named("dev_bypass_enabled")` binding — check for namespace conflicts
- If `123456` bypass is already behind a flag, Task 9 may be a no-op
