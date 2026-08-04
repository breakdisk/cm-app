package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.work.ListenableWorker.Result
import androidx.work.WorkerParameters
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.HubOpsApiService
import io.logisticos.driver.core.network.service.PodApiService
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test

class OutboundSyncWorkerTest {

    private val context: Context = mockk(relaxed = true)
    private val workerParams: WorkerParameters = mockk(relaxed = true)
    private val syncQueueDao: SyncQueueDao = mockk(relaxed = true)
    private val podDao: PodDao = mockk(relaxed = true)
    private val taskDao: TaskDao = mockk(relaxed = true)
    private val locationDao: LocationBreadcrumbDao = mockk(relaxed = true)
    private val driverOpsApi: DriverOpsApiService = mockk(relaxed = true)
    private val podApi: PodApiService = mockk(relaxed = true)
    private val hubOpsApi: HubOpsApiService = mockk(relaxed = true)
    private val okHttpClient: OkHttpClient = mockk(relaxed = true)
    private val sessionManager: SessionManager = mockk(relaxed = true)

    @BeforeEach
    fun setUp() {
        // Every test below assumes an authenticated driver; doWork() short-circuits
        // to success without one.
        every { sessionManager.isLoggedIn() } returns true
    }

    private fun buildWorker() = OutboundSyncWorker(
        context = context,
        workerParams = workerParams,
        syncQueueDao = syncQueueDao,
        podDao = podDao,
        taskDao = taskDao,
        locationDao = locationDao,
        driverOpsApi = driverOpsApi,
        podApi = podApi,
        hubOpsApi = hubOpsApi,
        okHttpClient = okHttpClient,
        sessionManager = sessionManager,
    )

    private fun item(
        id: Long,
        action: SyncAction,
        payloadJson: String,
        createdAt: Long = System.currentTimeMillis(),
    ) = SyncQueueEntity(id = id, action = action, payloadJson = payloadJson, createdAt = createdAt)

    @Test
    fun `doWork short-circuits to success when logged out`() = runTest {
        every { sessionManager.isLoggedIn() } returns false

        assertEquals(Result.success(), buildWorker().doWork())

        // Queued items must survive logout rather than being drained against a
        // session that would 401.
        coVerify(exactly = 0) { syncQueueDao.getPendingItems(any()) }
    }

    @Test
    fun `doWork returns success when queue is empty`() = runTest {
        coEvery { syncQueueDao.getPendingItems(any()) } returns emptyList()

        assertEquals(Result.success(), buildWorker().doWork())
    }

    @Test
    fun `doWork removes item from queue after successful processing`() = runTest {
        val queued = item(1L, SyncAction.SHIFT_START, "{}")
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(queued)
        coEvery { driverOpsApi.goOnline() } returns Unit

        buildWorker().doWork()

        coVerify(exactly = 1) { syncQueueDao.remove(1L) }
    }

    @Test
    fun `doWork marks item failed and requests retry when the API throws`() = runTest {
        val queued = item(2L, SyncAction.SHIFT_START, "{}")
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(queued)
        coEvery { driverOpsApi.goOnline() } throws RuntimeException("network error")

        val result = buildWorker().doWork()

        coVerify(exactly = 1) { syncQueueDao.markFailed(eq(2L), eq("network error"), any()) }
        coVerify(exactly = 0) { syncQueueDao.remove(2L) }
        // Regression guard: this used to return success unconditionally, which
        // meant WorkManager never applied the one-shot exponential backoff and
        // recovery silently fell through to the 15-minute periodic tick.
        assertEquals(Result.retry(), result)
    }

    @Test
    fun `malformed payload is discarded permanently`() = runTest {
        val queued = item(3L, SyncAction.TASK_COMPLETE, "not-json")
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(queued)

        val result = buildWorker().doWork()

        coVerify(atLeast = 1) { syncQueueDao.remove(3L) }
        // A permanently-undeliverable item is not a retryable failure.
        assertEquals(Result.success(), result)
    }

    @Test
    fun `POD_SUBMIT past the retry window is abandoned and flagged locally`() = runTest {
        val stale = item(
            id = 4L,
            action = SyncAction.POD_SUBMIT,
            payloadJson = """{"taskId":"t-old"}""",
            createdAt = System.currentTimeMillis() - OutboundSyncWorker.EXPIRY_MS - 1_000,
        )
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(stale)

        buildWorker().doWork()

        // Surfaced to the driver rather than retried every 5 minutes forever.
        coVerify(exactly = 1) { taskDao.markSyncFailed("t-old") }
        coVerify(atLeast = 1) { syncQueueDao.remove(4L) }
        coVerify(exactly = 0) { podDao.getForTask(any()) }
    }

    @Test
    fun `POP_SUBMIT past the retry window is abandoned and flagged locally`() = runTest {
        val stale = item(
            id = 5L,
            action = SyncAction.POP_SUBMIT,
            payloadJson = """{"taskId":"t-old","shipmentId":"s-old"}""",
            createdAt = System.currentTimeMillis() - OutboundSyncWorker.EXPIRY_MS - 1_000,
        )
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(stale)

        buildWorker().doWork()

        coVerify(exactly = 1) { taskDao.markSyncFailed("t-old") }
        coVerify(atLeast = 1) { syncQueueDao.remove(5L) }
        coVerify(exactly = 0) { podApi.initiatePop(any()) }
    }

    @Test
    fun `POD_SUBMIT is dropped when no local POD row exists`() = runTest {
        val queued = item(6L, SyncAction.POD_SUBMIT, """{"taskId":"missing-task"}""")
        coEvery { syncQueueDao.getPendingItems(any()) } returns listOf(queued)
        coEvery { podDao.getForTask("missing-task") } returns null

        buildWorker().doWork()

        coVerify(atLeast = 1) { syncQueueDao.remove(6L) }
        coVerify(exactly = 0) { podApi.initiate(any()) }
    }
}
