package io.logisticos.driver.feature.home.data

import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.TaskItem
import io.logisticos.driver.core.network.service.TaskListResponse
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class ShiftRepositoryTest {
    private val api: DriverOpsApiService = mockk()
    private val shiftDao: ShiftDao = mockk(relaxed = true)
    private val taskDao: TaskDao = mockk(relaxed = true)
    private val sessionManager: SessionManager = mockk(relaxed = true)
    private val repo = ShiftRepository(api, shiftDao, taskDao, sessionManager)

    @Test
    fun `syncShift fetches from api and writes to room`() = runTest {
        every { sessionManager.getDriverId() } returns "d-1"
        every { sessionManager.getTenantId() } returns "t-1"
        coEvery { api.listMyTasks() } returns TaskListResponse(
            data = listOf(
                TaskItem(
                    taskId = "task-1",
                    shipmentId = "ship-1",
                    sequence = 1,
                    status = "pending",
                    taskType = "delivery",
                    customerName = "Alice",
                    address = "123 Main St",
                )
            )
        )
        coEvery { shiftDao.getShiftById(any()) } returns null
        coEvery { taskDao.getTasksForShiftOnce(any()) } returns emptyList()
        repo.syncShift()
        coVerify { shiftDao.insert(any()) }
        coVerify { taskDao.insertAll(any()) }
    }
}
