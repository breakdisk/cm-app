package io.logisticos.driver.feature.assignment.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.common.AssignmentPayload
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
    fun `accept calls repo with correct assignmentId`() = runTest {
        coEvery { repo.accept("asgn-1") } returns Result.success(Unit)
        vm.accept()
        coVerify { repo.accept("asgn-1") }
    }
}
