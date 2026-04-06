package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.work.ListenableWorker.Result
import androidx.work.WorkerParameters
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.PodApiService
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class OutboundSyncWorkerTest {

    private val context: Context = mockk(relaxed = true)
    private val workerParams: WorkerParameters = mockk(relaxed = true)
    private val syncQueueDao: SyncQueueDao = mockk(relaxed = true)
    private val podDao: PodDao = mockk(relaxed = true)
    private val driverOpsApi: DriverOpsApiService = mockk(relaxed = true)
    private val podApi: PodApiService = mockk(relaxed = true)

    private fun buildWorker() = OutboundSyncWorker(
        context = context,
        workerParams = workerParams,
        syncQueueDao = syncQueueDao,
        podDao = podDao,
        driverOpsApi = driverOpsApi,
        podApi = podApi
    )

    @Test
    fun `doWork returns SUCCESS when queue is empty`() = runTest {
        coEvery { syncQueueDao.getPendingItems(any()) } returns emptyList()

        val result = buildWorker().doWork()

        assertEquals(Result.success(), result)
    }

    @Test
    fun `doWork removes item from queue after successful processing`() = runTest {
        val item = SyncQueueEntity(
            id = 1L,
            action = SyncAction.TASK_STATUS_UPDATE,
            payloadJson = """{"taskId":"t1","status":"DELIVERED"}""",
            createdAt = 0L
        )
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(item)
        coEvery { driverOpsApi.updateTaskStatus(any(), any()) } returns Unit

        buildWorker().doWork()

        coVerify(exactly = 1) { syncQueueDao.remove(1L) }
    }

    @Test
    fun `doWork marks item failed when API throws`() = runTest {
        val item = SyncQueueEntity(
            id = 2L,
            action = SyncAction.TASK_STATUS_UPDATE,
            payloadJson = """{"taskId":"t2","status":"FAILED"}""",
            createdAt = 0L
        )
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(item)
        coEvery { driverOpsApi.updateTaskStatus(any(), any()) } throws RuntimeException("network error")

        buildWorker().doWork()

        coVerify(exactly = 1) { syncQueueDao.markFailed(eq(2L), eq("network error"), any()) }
        coVerify(exactly = 0) { syncQueueDao.remove(2L) }
    }

    @Test
    fun `doWork skips POD_SUBMIT when pod not found`() = runTest {
        val item = SyncQueueEntity(
            id = 3L,
            action = SyncAction.POD_SUBMIT,
            payloadJson = """{"taskId":"missing-task"}""",
            createdAt = 0L
        )
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(item)
        coEvery { podDao.getForTask("missing-task") } returns null

        buildWorker().doWork()

        // Item is still removed from queue (processItem returned without throwing)
        coVerify(exactly = 1) { syncQueueDao.remove(3L) }
        coVerify(exactly = 0) { podApi.submitPod(any(), any(), any(), any()) }
    }
}
