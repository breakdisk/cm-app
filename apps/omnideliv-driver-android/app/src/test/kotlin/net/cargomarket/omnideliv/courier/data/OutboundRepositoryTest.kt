package net.cargomarket.omnideliv.courier.data

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import net.cargomarket.omnideliv.courier.data.db.OutboundDao
import net.cargomarket.omnideliv.courier.data.db.OutboundEntity
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.SyncScheduler
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Test
import retrofit2.Response

/**
 * The outbound queue is the only place in this app where a local write is
 * authoritative-pending: a row in it is a delivery the courier performed and
 * the server has not yet agreed to. Losing one loses money.
 *
 * These are the rules a background drain makes reachable. Until it existed the
 * queue only ever ran while a courier was looking at the manifest screen; a
 * worker fires unattended, hours later, with whatever session the phone still
 * has.
 */
class OutboundRepositoryTest {

    /** Real state. Only the network is faked — the queue's behaviour is the subject. */
    private class FakeOutboundDao : OutboundDao {
        val rows = mutableListOf<OutboundEntity>()
        private var nextId = 0L

        override suspend fun insert(row: OutboundEntity): Long {
            val id = ++nextId
            rows += row.copy(id = id)
            return id
        }

        override suspend fun pending(): List<OutboundEntity> =
            rows.filter { it.state == "PENDING" }
                .sortedWith(compareBy({ it.deviceTimestamp }, { it.id }))

        override fun pendingCount(): Flow<Int> =
            MutableStateFlow(rows.count { it.state == "PENDING" })

        override suspend fun parked(): List<OutboundEntity> = rows.filter { it.state == "PARKED" }

        override suspend fun markSynced(id: Long) = mutate(id) { it.copy(state = "SYNCED") }

        override suspend fun recordAttempt(id: Long, attempts: Int) =
            mutate(id) { it.copy(attempts = attempts) }

        override suspend fun clearProof(id: Long) = mutate(id) { it.copy(proofPath = null) }

        override suspend fun park(id: Long, reason: String) =
            mutate(id) { it.copy(state = "PARKED", parkedReason = reason) }

        private fun mutate(id: Long, f: (OutboundEntity) -> OutboundEntity) {
            val i = rows.indexOfFirst { it.id == id }
            if (i >= 0) rows[i] = f(rows[i])
        }
    }

    private class FakeScheduler : SyncScheduler {
        var kicks = 0
        override fun kick() { kicks++ }
    }

    private val dao = FakeOutboundDao()
    private val api = mockk<CourierApi>()
    private val scheduler = FakeScheduler()
    private val repo = OutboundRepository(dao, api, scheduler)

    private fun refused(code: Int) = Response.error<Unit>(code, "".toResponseBody())

    /**
     * Recording is what makes a drain due, so recording is what asks for one.
     *
     * Asked here rather than at the call sites because there are four of them
     * and a fifth added later would silently queue a delivery nothing ever
     * sends — which is exactly the state this app shipped in.
     */
    @Test
    fun `recording a milestone asks for a drain`() = runTest {
        repo.record(MilestoneKind.COLLECTED, "assignment-1", "vendor-1")

        assertEquals(1, scheduler.kicks)
    }

    /**
     * The token lives an hour and there is no refresh. A worker that meets an
     * expired session must leave the queue exactly as it found it: the courier
     * signs in again and the delivery goes.
     *
     * Consuming an attempt would be a slow version of the same loss — five
     * unattended passes overnight and the row parks itself.
     */
    @Test
    fun `an expired session leaves the delivery pending and unattempted`() = runTest {
        repo.record(MilestoneKind.DELIVERED, "assignment-1", "order-1")
        coEvery { api.delivered(any(), any()) } returns refused(401)

        val drained = repo.drain()

        assertFalse(drained)
        assertEquals("PENDING", dao.rows.single().state)
        assertEquals(0, dao.rows.single().attempts)
    }

    /**
     * Wiring the worker created a second drain path alongside the manifest
     * screen's. Both read the same queue, so without serialisation the same
     * delivery goes twice — and a duplicate `delivered` is a duplicate credit
     * attempt against the courier ledger.
     */
    @Test
    fun `two drains running at once send a milestone once`() = runTest {
        repo.record(MilestoneKind.COLLECTED, "assignment-1", "vendor-1")
        val inFlight = CompletableDeferred<Unit>()
        coEvery { api.collected(any(), any()) } coAnswers {
            inFlight.await()
            Response.success(Unit)
        }

        val first = launch { repo.drain() }
        runCurrent()
        val second = launch { repo.drain() }
        runCurrent()
        inFlight.complete(Unit)
        advanceUntilIdle()

        first.join()
        second.join()
        coVerify(exactly = 1) { api.collected(any(), any()) }
        assertEquals("SYNCED", dao.rows.single().state)
    }
}
