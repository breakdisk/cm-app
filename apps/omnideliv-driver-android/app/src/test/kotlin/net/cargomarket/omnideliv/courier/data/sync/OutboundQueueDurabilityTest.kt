package net.cargomarket.omnideliv.courier.data.sync

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import androidx.work.Configuration
import androidx.work.ListenableWorker
import androidx.work.WorkManager
import androidx.work.WorkerFactory
import androidx.work.WorkerParameters
import androidx.work.testing.SynchronousExecutor
import androidx.work.testing.WorkManagerTestInitHelper
import io.mockk.coEvery
import io.mockk.mockk
import kotlinx.coroutines.runBlocking
import net.cargomarket.omnideliv.courier.data.ArrivedRequest
import net.cargomarket.omnideliv.courier.data.CollectedRequest
import net.cargomarket.omnideliv.courier.data.CourierApi
import net.cargomarket.omnideliv.courier.data.DeliveredRequest
import net.cargomarket.omnideliv.courier.data.OutboundRepository
import net.cargomarket.omnideliv.courier.data.db.CourierDb
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.SyncScheduler
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import retrofit2.Response
import java.io.File
import java.io.IOException

/**
 * The spec's V2: the outbound queue survives, orders itself by the device
 * clock, and never duplicates — proven against a real Room database and a real
 * WorkManager rather than against fakes.
 *
 * Why this file exists at all: the queue is the only place in the app where a
 * local write is authoritative-pending. A row in it is a delivery the courier
 * performed and the server has not agreed to yet, so losing one loses money —
 * and every test of it until now ran against an in-memory fake DAO, which
 * cannot lose anything and therefore cannot prove anything about durability.
 *
 * Why Robolectric and not `androidTest`: this app's CI runs on a plain ubuntu
 * runner with no emulator, so an instrumented test would compile and never
 * execute. A suite that reports green without running is the failure this
 * repository has been bitten by more than any other. This runs inside the
 * ordinary `testDebugUnitTest` task, which CI already gates on.
 *
 * `WorkManagerTestInitHelper` cannot force-kill a process, as the spec notes.
 * Closing the database and opening a new instance against the same file is the
 * valid equivalent: the queue is Room-backed, so surviving that is exactly what
 * surviving process death means for it.
 */
@RunWith(RobolectricTestRunner::class)
// Robolectric 4.13 ships SDK 34 as its ceiling while the app compiles against
// 35. Pinned rather than left to default, so this is a decision on the record
// instead of a resolution error on some future runner.
@Config(sdk = [34])
class OutboundQueueDurabilityTest {

    private val context: Context = ApplicationProvider.getApplicationContext()
    private lateinit var db: CourierDb

    /** Every milestone the server was actually asked to accept, in order. */
    private val sent = mutableListOf<String>()

    /** Assignment ids the server refuses outright, as a stale id would be. */
    private val refuse = mutableSetOf<String>()

    /** Assignment ids the server has already completed — a lost response. */
    private val alreadyDone = mutableSetOf<String>()

    /** When false, every call throws: no route, no DNS, a basement. */
    private var online = true

    private val api = mockk<CourierApi>()

    /** Scheduling is asserted on its own; recording must not depend on it. */
    private val noScheduler = SyncScheduler { }

    private fun repo(database: CourierDb = db) =
        OutboundRepository(database.outbound(), api, noScheduler)

    private fun openDb(): CourierDb =
        // File-backed on purpose. An in-memory database cannot demonstrate
        // durability, because closing it is what destroys it — the test would
        // prove the opposite of what it claims.
        Room.databaseBuilder(context, CourierDb::class.java, DB_NAME).build()

    private fun answer(assignmentId: String, kind: String): Response<Unit> {
        if (!online) throw IOException("offline")
        sent += "$kind:$assignmentId"
        return when (assignmentId) {
            // 404 is what a stale assignment id returns after the hardening
            // work: a permanent refusal, never worth a retry.
            in refuse -> Response.error(404, "".toResponseBody())
            // 202 is what field-ops answers for a milestone it has already
            // completed — the retry of a delivery whose response was lost.
            in alreadyDone -> Response.success(202, Unit)
            else -> Response.success(Unit)
        }
    }

    @Before
    fun setUp() {
        deleteDb(context.getDatabasePath(DB_NAME))
        db = openDb()
        sent.clear()
        refuse.clear()
        alreadyDone.clear()
        online = true

        coEvery { api.arrived(any(), any<ArrivedRequest>()) } coAnswers {
            answer(firstArg(), "ARRIVED")
        }
        coEvery { api.collected(any(), any<CollectedRequest>()) } coAnswers {
            answer(firstArg(), "COLLECTED")
        }
        coEvery { api.delivered(any(), any<DeliveredRequest>()) } coAnswers {
            answer(firstArg(), "DELIVERED")
        }
    }

    @After
    fun tearDown() {
        db.close()
        deleteDb(context.getDatabasePath(DB_NAME))
    }

    private fun deleteDb(file: File) {
        file.delete()
        File("${file.path}-shm").delete()
        File("${file.path}-wal").delete()
    }

    /**
     * The restart. Five milestones recorded with no connectivity, the database
     * closed, and a fresh instance opened against the same file — which is what
     * a process death and relaunch leaves behind for a Room-backed queue.
     */
    @Test
    fun `queued milestones survive the database being closed and reopened`() = runBlocking {
        online = false
        val offline = repo()
        repeat(5) { i ->
            offline.record(MilestoneKind.ARRIVED, "assignment-$i", "stop-$i", atMillis = 1_000L + i)
        }
        offline.drain() // No network: everything stays exactly where it is.
        db.close()

        db = openDb()

        val rows = db.outbound().pending()
        assertEquals("no milestone may be lost to a restart", 5, rows.size)
        assertTrue("a restart must not mark anything as sent", rows.all { it.state == "PENDING" })
    }

    /**
     * Ordering is by the device clock, not by insertion order or row id.
     *
     * These are inserted deliberately out of order, which is the real case: the
     * UI writes a proof-backed delivery only once its photo has encoded, and can
     * therefore commit a later event first. The server's state machine refuses a
     * delivery for an order it has not seen collected, so a queue that replays
     * in insertion order is correctly rejected and the courier's shift stops.
     */
    @Test
    fun `the queue drains in device timestamp order across a restart`() = runBlocking {
        online = false
        val offline = repo()
        offline.record(MilestoneKind.DELIVERED, "job", "order-1", atMillis = 3_000)
        offline.record(MilestoneKind.ARRIVED, "job", "vendor-1", atMillis = 1_000)
        offline.record(MilestoneKind.COLLECTED, "job", "vendor-1", atMillis = 2_000)
        offline.drain()
        db.close()

        db = openDb()
        online = true
        val drained = repo().drain()

        assertTrue("a healthy queue drains completely", drained)
        assertEquals(listOf("ARRIVED:job", "COLLECTED:job", "DELIVERED:job"), sent)
        assertEquals("nothing may be sent twice", sent.size, sent.distinct().size)
        assertTrue("a drained queue is empty", db.outbound().pending().isEmpty())
    }

    /**
     * A row the server will never accept must step aside, not block.
     *
     * Under strict ordering an immortal row at the head freezes every milestone
     * behind it — a courier's whole shift stops syncing behind one bad record
     * while the badge shows work pending forever.
     */
    @Test
    fun `a permanently refused row parks and the rest still go`() = runBlocking {
        refuse += "stale-assignment"
        val r = repo()
        r.record(MilestoneKind.ARRIVED, "stale-assignment", "stop", atMillis = 1_000)
        r.record(MilestoneKind.COLLECTED, "good-assignment", "vendor", atMillis = 2_000)
        r.record(MilestoneKind.DELIVERED, "good-assignment", "order", atMillis = 3_000)

        val drained = r.drain()

        assertFalse("the pass did not complete cleanly, and must say so", drained)
        assertEquals(
            "the refused row must not have stopped the two behind it",
            listOf(
                "ARRIVED:stale-assignment",
                "COLLECTED:good-assignment",
                "DELIVERED:good-assignment",
            ),
            sent,
        )
        assertEquals(1, db.outbound().parked().size)
        assertEquals("stale-assignment", db.outbound().parked().single().assignmentId)
        assertTrue(
            "the good rows are accepted, not left pending",
            db.outbound().pending().isEmpty(),
        )
    }

    /**
     * The worker, not just the repository.
     *
     * Everything above calls `drain()` directly. This drives the path a
     * courier's phone actually uses — `SyncScheduler.kick()` enqueues unique
     * work under a network constraint, and WorkManager runs it once that
     * constraint is met — so the background drain is covered rather than
     * assumed.
     *
     * The worker factory is supplied by hand because Hilt does not run here.
     * That is the one seam this test does not reach; `CourierApp` is where it is
     * declared, and the manifest's removal of the default WorkManager
     * initializer is what makes it take effect on a device.
     */
    @Test
    fun `the scheduled worker drains the queue when the network constraint is met`() = runBlocking {
        val repo = repo()
        WorkManagerTestInitHelper.initializeTestWorkManager(
            context,
            Configuration.Builder()
                .setExecutor(SynchronousExecutor())
                .setWorkerFactory(object : WorkerFactory() {
                    override fun createWorker(
                        appContext: Context,
                        workerClassName: String,
                        workerParameters: WorkerParameters,
                    ): ListenableWorker = OutboundDrainWorker(appContext, workerParameters, repo)
                })
                .build(),
        )

        repo.record(MilestoneKind.DELIVERED, "job", "order-1", atMillis = 1_000)
        assertEquals("nothing is sent before the worker runs", emptyList<String>(), sent)

        WorkManagerSyncScheduler(context).kick()
        val work = WorkManager.getInstance(context)
            .getWorkInfosForUniqueWork(KICK_WORK_NAME)
            .get()
        assertEquals("the kick must enqueue exactly one drain", 1, work.size)

        WorkManagerTestInitHelper.getTestDriver(context)!!.setAllConstraintsMet(work.single().id)
        // `SynchronousExecutor` runs `startWork()` on this thread, but a
        // CoroutineWorker immediately hands its body to Dispatchers.Default and
        // returns a future — so without waiting, the assertions race the drain
        // and read an empty list. Bounded, so a worker that never finishes
        // fails the test rather than hanging the build.
        awaitFinished(work.single().id)

        assertEquals(listOf("DELIVERED:job"), sent)
        assertTrue("the worker must clear what it sent", db.outbound().pending().isEmpty())
    }

    /**
     * The spec's V4, on the app's side of the line: the three rules composed
     * rather than each proven alone.
     *
     * A delivery committed **outside the geofence** — advisory, so the courier
     * is never blocked — recorded **offline** so the row is the only record of
     * it, then **retried** against a server that has already completed it and
     * answers 202. Each rule is trivial in isolation. Two rules tested apart and
     * never composed is the exact shape of the last defect shipped on this
     * surface, and of both defects found while building this one.
     *
     * The period boundary that the composition's server half turns on is proven
     * where it lives, at the index: `services/field-ops/tests/`
     * `ledger_period_idempotency.rs`.
     */
    @Test
    fun `a delivery committed out of bounds, queued offline, and retried is sent once`() = runBlocking {
        // Far from the stop: advice, never a block. The commit proceeds.
        val advice = net.cargomarket.omnideliv.courier.domain.adviseGeofence(
            courierLat = 14.5995,
            courierLng = 120.9842,
            fixAgeSeconds = 5,
            stopLat = 14.6100,
            stopLng = 120.9842,
        )
        assertTrue(
            "the fixture must actually be out of bounds, or this proves nothing",
            advice is net.cargomarket.omnideliv.courier.domain.GeofenceAdvice.Away,
        )

        online = false
        val r = repo()
        r.record(MilestoneKind.COLLECTED, "job", "vendor-1", atMillis = 1_000)
        r.record(MilestoneKind.DELIVERED, "job", "order-1", atMillis = 2_000)
        assertFalse("offline, nothing can be sent", r.drain())
        assertEquals(emptyList<String>(), sent)

        // The first attempt reached the server after all; its response was lost.
        // The retry meets a job field-ops has already completed.
        online = true
        alreadyDone += "job"
        db.close()
        db = openDb()

        val drained = repo().drain()

        assertTrue("an already-completed job is a success, not an error", drained)
        assertEquals(listOf("COLLECTED:job", "DELIVERED:job"), sent)
        assertTrue(
            "a 202 must settle the row, not leave it to be sent again forever",
            db.outbound().pending().isEmpty(),
        )
        assertTrue("nothing may be parked by a successful retry", db.outbound().parked().isEmpty())
    }

    /** Wait for one work request to reach a terminal state, or fail loudly. */
    private fun awaitFinished(id: java.util.UUID) {
        val deadline = System.currentTimeMillis() + WORK_TIMEOUT_MS
        while (System.currentTimeMillis() < deadline) {
            val info = WorkManager.getInstance(context).getWorkInfoById(id).get()
            if (info != null && info.state.isFinished) return
            Thread.sleep(20)
        }
        throw AssertionError("the drain worker did not finish within ${WORK_TIMEOUT_MS}ms")
    }

    private companion object {
        const val DB_NAME = "durability-test.db"
        const val WORK_TIMEOUT_MS = 10_000L

        /** Mirrors the private constant in [WorkManagerSyncScheduler]. */
        const val KICK_WORK_NAME = "outbound_drain_kick"
    }
}
