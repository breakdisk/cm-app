package net.cargomarket.omnideliv.courier.data

import kotlinx.coroutines.flow.Flow
import net.cargomarket.omnideliv.courier.data.db.OutboundDao
import net.cargomarket.omnideliv.courier.data.db.OutboundEntity
import net.cargomarket.omnideliv.courier.data.db.toDomain
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.SyncDecision
import net.cargomarket.omnideliv.courier.domain.decideAfterFailure
import net.cargomarket.omnideliv.courier.domain.drainOrder
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Recording milestones, and getting them to the server eventually.
 *
 * Every write lands in the queue first and is uploaded second, so a courier in a
 * basement can finish a delivery and move on. The queue is what makes that safe:
 * the local row is the claim, and the server accepting it is what settles it.
 */
@Singleton
class OutboundRepository @Inject constructor(
    private val dao: OutboundDao,
    private val api: CourierApi,
) {

    val pendingCount: Flow<Int> get() = dao.pendingCount()

    /**
     * Record a milestone.
     *
     * `deviceTimestamp` is taken **here**, at the moment the courier tapped —
     * not when the upload eventually runs. This is project law and it is load
     * bearing: SLA maths uses the device clock, so a payload that sat offline
     * for twenty minutes must not bill those minutes to the courier.
     */
    suspend fun record(
        kind: MilestoneKind,
        assignmentId: String,
        stopRef: String?,
        proofPath: String? = null,
        atMillis: Long = System.currentTimeMillis(),
    ): Long = dao.insert(
        OutboundEntity(
            kind = kind.name,
            assignmentId = assignmentId,
            stopRef = stopRef,
            deviceTimestamp = isoFromMillis(atMillis),
            proofPath = proofPath,
        ),
    )

    /**
     * Send everything pending, oldest physical event first, stopping at the
     * first row that cannot go.
     *
     * Stopping rather than skipping is deliberate. The server's state machine
     * refuses a delivery for an order it has not seen collected, so pushing past
     * a stuck row would send milestones out of order and have them correctly
     * rejected — turning one stuck row into several.
     *
     * @return true if the queue drained completely.
     */
    suspend fun drain(): Boolean {
        // Re-read once. Parking a row does not change the order of the rest, so
        // there is no reason to re-query — and recursing to "start again after a
        // park" would be quadratic and stack-deep on a shift with several bad
        // rows.
        val queue = drainOrder(dao.pending().map { it.toDomain() })
        var drainedEverything = true

        for (item in queue) {
            val sent = runCatching {
                send(item.kind, item.assignmentId, item.stopRef, item.deviceTimestamp)
            }
            val status = sent.getOrNull()

            if (status != null && status in 200..299) {
                dao.markSynced(item.id)
                continue
            }

            val attempts = item.attempts + 1
            dao.recordAttempt(item.id, attempts)

            // A null status means the request threw — no response at all, which
            // is transient by definition and must never read as a rejection.
            when (val decision = decideAfterFailure(attempts, status)) {
                is SyncDecision.Retry -> {
                    // Stop here rather than skipping ahead. The server's state
                    // machine refuses a delivery for an order it has not seen
                    // collected, so pushing past a stuck row would send
                    // milestones out of order and have them correctly rejected —
                    // turning one stuck row into several.
                    return false
                }

                is SyncDecision.Park -> {
                    dao.park(item.id, decision.reason)
                    // A parked row no longer blocks: that is the entire purpose
                    // of parking. Carry on with the rest, but the queue did not
                    // drain cleanly and the caller should know.
                    drainedEverything = false
                }
            }
        }
        return drainedEverything
    }

    private suspend fun send(
        kind: MilestoneKind,
        assignmentId: String,
        stopRef: String?,
        deviceTimestamp: String,
    ): Int = when (kind) {
        MilestoneKind.ARRIVED -> api.arrived(
            assignmentId,
            ArrivedRequest(stopRef = stopRef.orEmpty(), deviceTimestamp = deviceTimestamp),
        ).code()

        MilestoneKind.COLLECTED -> api.collected(
            assignmentId,
            CollectedRequest(vendorId = stopRef.orEmpty(), deviceTimestamp = deviceTimestamp),
        ).code()

        MilestoneKind.DELIVERED -> api.delivered(
            assignmentId,
            DeliveredRequest(deviceTimestamp = deviceTimestamp),
        ).code()
    }

    private companion object {
        /**
         * ISO-8601 in UTC, the one format the backend parses.
         *
         * Converted at capture rather than at send, for the same reason the
         * timestamp is taken at capture.
         */
        fun isoFromMillis(millis: Long): String =
            DateTimeFormatter.ISO_OFFSET_DATE_TIME.format(
                Instant.ofEpochMilli(millis).atOffset(ZoneOffset.UTC),
            )
    }
}
