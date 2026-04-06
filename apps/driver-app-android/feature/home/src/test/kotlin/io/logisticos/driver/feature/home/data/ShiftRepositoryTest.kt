package io.logisticos.driver.feature.home.data

import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.ShiftResponse
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

class ShiftRepositoryTest {
    private val api: DriverOpsApiService = mockk()
    private val shiftDao: ShiftDao = mockk(relaxed = true)
    private val taskDao: TaskDao = mockk(relaxed = true)
    private val repo = ShiftRepository(api, shiftDao, taskDao)

    @Test
    fun `syncShift fetches from api and writes to room`() = runTest {
        coEvery { api.getActiveShift() } returns ShiftResponse(
            id = "shift-1", driverId = "d-1", tenantId = "t-1",
            totalStops = 5, tasks = emptyList()
        )
        repo.syncShift()
        coVerify { shiftDao.insert(any()) }
    }
}
