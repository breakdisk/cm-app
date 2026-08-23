package net.cargomarket.omnideliv.courier.domain

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class SyncPolicyTest {

    private fun row(
        id: Long,
        at: String,
        state: SyncState = SyncState.PENDING,
        attempts: Int = 0,
    ) = QueuedMilestone(
        id = id,
        kind = MilestoneKind.COLLECTED,
        assignmentId = "a",
        stopRef = "s",
        deviceTimestamp = at,
        proofPath = null,
        attempts = attempts,
        state = state,
    )

    @Test
    fun `a transport failure is retried`() {
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(0, null))
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(4, null))
    }

    @Test
    fun `a 5xx is retried`() {
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(0, 500))
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(2, 503))
    }

    /**
     * The head-of-line hazard. Under strict ordering one immortal row blocks
     * every milestone behind it, so a courier's whole shift stops syncing
     * behind one bad record while the UI shows a spinner.
     */
    @Test
    fun `a transport failure is parked once the attempt budget runs out`() {
        val d = decideAfterFailure(MAX_SYNC_ATTEMPTS, null)
        assertInstanceOf(SyncDecision.Park::class.java, d)
    }

    /**
     * A stale assignment id, or a milestone the server refuses because this
     * courier does not hold the job. Five attempts cannot change either answer.
     */
    @Test
    fun `a 4xx is parked immediately without burning the budget`() {
        assertInstanceOf(SyncDecision.Park::class.java, decideAfterFailure(0, 404))
        assertInstanceOf(SyncDecision.Park::class.java, decideAfterFailure(0, 400))
        assertInstanceOf(SyncDecision.Park::class.java, decideAfterFailure(0, 409))
    }

    /** Both explicitly mean "try again", so they are not ordinary 4xx. */
    @Test
    fun `408 and 429 are retried despite being 4xx`() {
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(0, 408))
        assertInstanceOf(SyncDecision.Retry::class.java, decideAfterFailure(0, 429))
    }

    /**
     * The server's state machine refuses a delivery that arrives before its
     * collection, so the drain must preserve the order the courier performed —
     * by the *device* clock, not by insertion or upload time.
     */
    @Test
    fun `the drain is ordered by the device clock`() {
        val queue = listOf(
            row(3, "2026-08-17T10:05:00Z"),
            row(1, "2026-08-17T10:01:00Z"),
            row(2, "2026-08-17T10:03:00Z"),
        )
        assertEquals(listOf(1L, 2L, 3L), drainOrder(queue).map { it.id })
    }

    /** Two events in the same second still need a total order. */
    @Test
    fun `ties break on insertion order`() {
        val queue = listOf(
            row(7, "2026-08-17T10:01:00Z"),
            row(2, "2026-08-17T10:01:00Z"),
        )
        assertEquals(listOf(2L, 7L), drainOrder(queue).map { it.id })
    }

    @Test
    fun `synced and parked rows are not re-attempted`() {
        val queue = listOf(
            row(1, "2026-08-17T10:01:00Z", SyncState.SYNCED),
            row(2, "2026-08-17T10:02:00Z", SyncState.PARKED),
            row(3, "2026-08-17T10:03:00Z", SyncState.PENDING),
        )
        assertEquals(listOf(3L), drainOrder(queue).map { it.id })
    }

    /**
     * A parked row is still unsynced work. Hiding it from the badge would make
     * a stuck queue look idle, which is the one thing the badge exists to
     * prevent.
     */
    @Test
    fun `the pending badge counts parked rows`() {
        val queue = listOf(
            row(1, "a", SyncState.SYNCED),
            row(2, "b", SyncState.PARKED),
            row(3, "c", SyncState.PENDING),
        )
        assertEquals(2, pendingCount(queue))
    }

    @Test
    fun `a fully synced queue shows nothing pending`() {
        val queue = listOf(row(1, "a", SyncState.SYNCED), row(2, "b", SyncState.SYNCED))
        assertEquals(0, pendingCount(queue))
        assertTrue(drainOrder(queue).isEmpty())
    }

    /**
     * A 401 refuses the *caller*, not the payload — and parking on it would
     * discard a delivery the courier really made, along with the credit for
     * it.
     *
     * This is not hypothetical. The access token lives one hour
     * (`AUTH__JWT_EXPIRY_SECONDS: 3600`) and this app stores no refresh token,
     * so every shift longer than an hour meets one. A background drain firing
     * unattended is what turns that from "the screen looks stale" into
     * "yesterday's deliveries were thrown away while the courier slept".
     *
     * Halt rather than Retry so the attempt budget is not spent either: five
     * unattended passes against an expired token would otherwise park the row
     * by the back door.
     */
    @Test
    fun `an expired session halts the pass rather than parking the delivery`() {
        assertInstanceOf(SyncDecision.Halt::class.java, decideAfterFailure(1, 401))
        assertInstanceOf(SyncDecision.Halt::class.java, decideAfterFailure(1, 403))
        // Past the budget it still halts. The budget counts refusals of the
        // payload; this was never one.
        assertInstanceOf(SyncDecision.Halt::class.java, decideAfterFailure(MAX_SYNC_ATTEMPTS, 401))
    }

    /**
     * The neighbours stay put. 404 is what a stale assignment id returns after
     * the hardening work, and it must still park rather than halt — otherwise
     * one dead row stops the queue forever instead of stepping aside.
     */
    @Test
    fun `a refusal of the payload still parks`() {
        assertInstanceOf(SyncDecision.Park::class.java, decideAfterFailure(1, 404))
        assertInstanceOf(SyncDecision.Park::class.java, decideAfterFailure(1, 400))
    }
}
