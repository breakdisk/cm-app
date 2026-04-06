package io.logisticos.driver.feature.route.presentation

import app.cash.turbine.test
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.route.data.RouteRepository
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.*
import org.junit.jupiter.api.*
import org.junit.jupiter.api.Assertions.*

@OptIn(ExperimentalCoroutinesApi::class)
class RouteViewModelTest {
    private val testDispatcher = UnconfinedTestDispatcher()
    private val repo: RouteRepository = mockk(relaxed = true)
    private lateinit var vm: RouteViewModel

    private fun makeTask(id: String, order: Int, status: TaskStatus = TaskStatus.ASSIGNED) =
        TaskEntity(
            id = id,
            shiftId = "s1",
            awb = "LS-$id",
            recipientName = "Name",
            recipientPhone = "",
            address = "Addr",
            lat = 0.0,
            lng = 0.0,
            status = status,
            stopOrder = order,
            requiresPhoto = false,
            requiresSignature = false,
            requiresOtp = false,
            isCod = false,
            codAmount = 0.0,
            attemptCount = 0,
            notes = null,
            syncedAt = null
        )

    @BeforeEach
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        every { repo.observeTasks("s1") } returns flowOf(
            listOf(
                makeTask("t1", 1),
                makeTask("t2", 2),
                makeTask("t3", 3, TaskStatus.COMPLETED)
            )
        )
        vm = RouteViewModel(repo, "s1")
    }

    @AfterEach
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `active tasks excludes completed`() = runTest {
        vm.uiState.test {
            val state = awaitItem()
            assertEquals(2, state.activeTasks.size)
            assertEquals(1, state.completedTasks.size)
        }
    }

    @Test
    fun `reorder moves task to new position`() = runTest {
        vm.uiState.test {
            awaitItem()
            vm.reorder(fromIndex = 0, toIndex = 1)
            val state = awaitItem()
            assertEquals("t2", state.activeTasks[0].id)
            assertEquals("t1", state.activeTasks[1].id)
        }
    }
}
