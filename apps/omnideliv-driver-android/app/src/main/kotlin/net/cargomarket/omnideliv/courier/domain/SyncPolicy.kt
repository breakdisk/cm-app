package net.cargomarket.omnideliv.courier.domain

/**
 * A milestone the courier recorded that the server has not yet accepted.
 *
 * The outbound queue is the only place a local write is authoritative-pending.
 * The manifest cache is a render cache; this is a claim.
 */
data class QueuedMilestone(
    val id: Long,
    val kind: MilestoneKind,
    val assignmentId: String,
    /** Vendor id for a pickup, order id for the dropoff. Opaque to field-ops. */
    val stopRef: String?,
    /**
     * Hardware clock at the physical event — the tap, the shutter — serialised
     * the moment it happened.
     *
     * Never re-read at upload time. SLA maths uses this, so a payload that sat
     * in a dead zone for twenty minutes must not bill those minutes to the
     * courier.
     */
    val deviceTimestamp: String,
    val proofPath: String?,
    val attempts: Int,
    val state: SyncState,
)

enum class MilestoneKind { ARRIVED, COLLECTED, DELIVERED }

enum class SyncState {
    /** Waiting for a network, or for its turn in the queue. */
    PENDING,

    /** Accepted by the server. */
    SYNCED,

    /**
     * Given up on. Surfaced to the courier and to support rather than retried
     * forever — see [decideAfterFailure].
     */
    PARKED,
}

/** What the worker should do with a row after one failed attempt. */
sealed interface SyncDecision {
    /** Try again later, with backoff. */
    data object Retry : SyncDecision

    /** Stop trying. Carries why, for the support surface. */
    data class Park(val reason: String) : SyncDecision
}

/**
 * How many times a transient failure is retried before the row is parked.
 *
 * Bounded because the queue drains in order: an immortal row at the head blocks
 * every milestone behind it, so a courier's whole shift stops syncing behind
 * one bad record while the UI shows a spinner.
 */
const val MAX_SYNC_ATTEMPTS = 5

/**
 * Decide a queued row's fate after an attempt failed.
 *
 * `httpStatus` is null for a transport failure — no route, DNS, a socket closed
 * mid-flight — which is the ordinary offline case and always worth retrying
 * until the attempt budget runs out.
 *
 * A 4xx will never succeed on retry: a stale assignment id, a milestone the
 * server refuses because this courier does not hold the job, a malformed
 * payload. Parking it at once is both correct and faster than exhausting five
 * attempts to learn what the first one already said.
 *
 * 408 and 429 are the exceptions inside 4xx — both explicitly mean *try again*.
 */
fun decideAfterFailure(attempts: Int, httpStatus: Int?): SyncDecision {
    if (httpStatus != null && httpStatus in 400..499 && httpStatus != 408 && httpStatus != 429) {
        return SyncDecision.Park("server refused with $httpStatus; retrying cannot change that")
    }
    if (attempts >= MAX_SYNC_ATTEMPTS) {
        return SyncDecision.Park("gave up after $MAX_SYNC_ATTEMPTS attempts")
    }
    return SyncDecision.Retry
}

/**
 * The rows a drain pass should attempt, in the order it should attempt them.
 *
 * Chronological by the *device* clock, so the server sees the sequence the
 * courier actually performed — a collection recorded before a delivery must not
 * arrive after it, or the order's state machine correctly refuses the pair.
 *
 * Parked rows are skipped rather than removed: they are evidence, and a courier
 * asking "why does this job still say syncing" needs them to still exist.
 */
fun drainOrder(queue: List<QueuedMilestone>): List<QueuedMilestone> =
    queue.filter { it.state == SyncState.PENDING }
        .sortedWith(compareBy({ it.deviceTimestamp }, { it.id }))

/**
 * How many milestones are still unacknowledged.
 *
 * Drives the badge. Parked rows count: they are unsynced, and hiding them would
 * make a stuck queue look idle.
 */
fun pendingCount(queue: List<QueuedMilestone>): Int =
    queue.count { it.state != SyncState.SYNCED }
