package io.logisticos.driver.feature.assignment.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.common.AssignmentPayload
import io.logisticos.driver.feature.assignment.data.AssignmentRepository
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*

/**
 * Note on why these tests do not assert on the transient `isAccepting` /
 * `isRejecting` state by simply calling `awaitItem()` twice.
 *
 * `Dispatchers.Main` is an `UnconfinedTestDispatcher`, so `accept()`'s coroutine
 * runs eagerly and, when the repository mock returns immediately, completes
 * before the call returns. `_uiState` is a `MutableStateFlow`, which conflates:
 * by the time the test resumes, the loading value has already been overwritten
 * by the terminal one and was never emitted to a collector. The previous version
 * of this file awaited it anyway, so it read the terminal state as if it were
 * the loading state ("expected true but was false") and then blocked forever
 * waiting for an item that had already been superseded.
 *
 * Where the in-flight state genuinely matters it is tested by suspending the
 * repository on a [CompletableDeferred], which holds the coroutine open so the
 * intermediate state is actually observable.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class AssignmentViewModelTest {

    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: AssignmentRepository = mockk()
    private val payload = AssignmentPayload(
        assignmentId   = "asgn-1",
        shipmentId     = "ship-1",
        customerName   = "Juan dela Cruz",
        address        = "123 Rizal St, Makati",
        taskType       = "delivery",
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
    fun `initial state populates from payload`() = runTest(testDispatcher.scheduler) {
        val state = vm.uiState.value
        assertEquals("asgn-1", state.assignmentId)
        assertEquals("ship-1", state.shipmentId)
        assertEquals("Juan dela Cruz", state.customerName)
        assertEquals("delivery", state.taskType)
        assertEquals(50_000L, state.codAmountCents)
        assertFalse(state.isAccepting)
        assertFalse(state.isRejecting)
        assertNull(state.error)
        assertFalse(state.isDone)
    }

    @Test
    fun `accept sets isDone on success`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.accept("asgn-1") } returns Result.success(Unit)

        vm.accept()

        val state = vm.uiState.value
        assertTrue(state.isDone)
        assertFalse(state.isAccepting)
        assertNull(state.error)
    }

    @Test
    fun `accept sets error on failure and does not mark done`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.accept("asgn-1") } returns Result.failure(RuntimeException("network error"))

        vm.accept()

        val state = vm.uiState.value
        assertEquals("network error", state.error)
        assertFalse(state.isAccepting)
        // The offer must stay actionable so the driver can retry rather than
        // having the screen dismiss itself on a failed accept.
        assertFalse(state.isDone)
    }

    @Test
    fun `accept exposes isAccepting while the call is in flight`() = runTest(testDispatcher.scheduler) {
        val gate = CompletableDeferred<Result<Unit>>()
        coEvery { repo.accept("asgn-1") } coAnswers { gate.await() }

        vm.uiState.test {
            assertFalse(awaitItem().isAccepting)   // initial

            vm.accept()
            assertTrue(awaitItem().isAccepting)    // held open by the gate

            gate.complete(Result.success(Unit))
            val done = awaitItem()
            assertTrue(done.isDone)
            assertFalse(done.isAccepting)
            cancelAndIgnoreRemainingEvents()
        }
    }

    @Test
    fun `reject sets isDone on success`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.reject("asgn-1", any()) } returns Result.success(Unit)

        vm.reject("CUSTOMER_ABSENT")

        val state = vm.uiState.value
        assertTrue(state.isDone)
        assertFalse(state.isRejecting)
    }

    @Test
    fun `reject sets error on failure`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.reject("asgn-1", any()) } returns Result.failure(RuntimeException("timeout"))

        vm.reject("OTHER")

        val state = vm.uiState.value
        assertEquals("timeout", state.error)
        assertFalse(state.isDone)
    }

    @Test
    fun `reject forwards the selected reason`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.reject("asgn-1", "CUSTOMER_ABSENT") } returns Result.success(Unit)

        vm.reject("CUSTOMER_ABSENT")

        coVerify(exactly = 1) { repo.reject("asgn-1", "CUSTOMER_ABSENT") }
    }

    @Test
    fun `accept calls repo with correct assignmentId`() = runTest(testDispatcher.scheduler) {
        coEvery { repo.accept("asgn-1") } returns Result.success(Unit)
        vm.accept()
        coVerify(exactly = 1) { repo.accept("asgn-1") }
    }
}
