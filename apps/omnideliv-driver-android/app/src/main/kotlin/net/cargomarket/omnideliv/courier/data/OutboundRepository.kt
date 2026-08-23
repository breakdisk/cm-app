package net.cargomarket.omnideliv.courier.data

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.asRequestBody
import net.cargomarket.omnideliv.courier.data.db.OutboundDao
import net.cargomarket.omnideliv.courier.data.db.OutboundEntity
import net.cargomarket.omnideliv.courier.data.db.toDomain
import net.cargomarket.omnideliv.courier.domain.MilestoneKind
import net.cargomarket.omnideliv.courier.domain.SyncDecision
import net.cargomarket.omnideliv.courier.domain.SyncScheduler
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
    private val scheduler: SyncScheduler,
) {

    val pendingCount: Flow<Int> get() = dao.pendingCount()

    /**
     * One drain at a time, whoever asked for it.
     *
     * There are two callers now — the manifest screen, for immediate feedback
     * while a courier is looking at it, and the background worker. Both read
     * the same rows, so without this the same delivery is sent twice: a
     * duplicate credit attempt against the courier ledger, and a duplicate
     * milestone against an order's state machine. Both run in this process, so
     * an in-process lock is the whole boundary.
     */
    private val drainLock = Mutex()

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
    ): Long {
        val id = dao.insert(
            OutboundEntity(
                kind = kind.name,
                assignmentId = assignmentId,
                stopRef = stopRef,
                deviceTimestamp = isoFromMillis(atMillis),
                proofPath = proofPath,
            ),
        )
        // Recording is what makes a drain due, so recording is what asks for
        // one. Asked here rather than at the four call sites because a fifth
        // added later would otherwise queue a delivery that nothing ever sends
        // — which is the state this app shipped in.
        scheduler.kick()
        return id
    }

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
    suspend fun drain(): Boolean = drainLock.withLock { drainPass() }

    /**
     * One pass over the queue. Never run concurrently with itself — see
     * [drainLock].
     */
    private suspend fun drainPass(): Boolean {
        // Re-read once. Parking a row does not change the order of the rest, so
        // there is no reason to re-query — and recursing to "start again after a
        // park" would be quadratic and stack-deep on a shift with several bad
        // rows.
        val queue = drainOrder(dao.pending().map { it.toDomain() })
        var drainedEverything = true

        for (item in queue) {
            // Evidence before the milestone. If the milestone went first and the
            // upload then failed, the row would be marked synced and the photo
            // lost — the delivery is recorded either way, so the only orderable
            // outcome worth protecting is the proof.
            if (item.proofPath != null) {
                when (uploadProof(item.stopRef, item.proofPath)) {
                    ProofOutcome.Sent -> dao.clearProof(item.id)
                    // Permanently refused: the server will not take this file on
                    // any retry. Drop it and let the delivery through rather than
                    // blocking a courier's queue behind an unloved image.
                    ProofOutcome.Rejected -> dao.clearProof(item.id)
                    // No response, or a server fault. Retry the whole row so the
                    // proof and its milestone stay together.
                    ProofOutcome.Retry -> return false
                }
            }

            val sent = runCatching {
                send(item.kind, item.assignmentId, item.stopRef, item.deviceTimestamp)
            }
            val status = sent.getOrNull()

            if (status != null && status in 200..299) {
                dao.markSynced(item.id)
                continue
            }

            val attempts = item.attempts + 1

            // A null status means the request threw — no response at all, which
            // is transient by definition and must never read as a rejection.
            when (val decision = decideAfterFailure(attempts, status)) {
                // The session was refused, not the milestone. Leave the queue
                // exactly as it was found — no attempt spent, nothing parked —
                // and stop. Once the courier signs in again every row still
                // goes, in the order it happened.
                is SyncDecision.Halt -> return false

                is SyncDecision.Retry -> {
                    dao.recordAttempt(item.id, attempts)
                    // Stop here rather than skipping ahead. The server's state
                    // machine refuses a delivery for an order it has not seen
                    // collected, so pushing past a stuck row would send
                    // milestones out of order and have them correctly rejected —
                    // turning one stuck row into several.
                    return false
                }

                is SyncDecision.Park -> {
                    dao.recordAttempt(item.id, attempts)
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

    private enum class ProofOutcome { Sent, Rejected, Retry }

    /**
     * Push one delivery photo.
     *
     * `stopRef` on a DELIVERED row is the **order id** — that is the contract
     * the manifest sets, and the proof route is keyed on the order so an
     * at-least-once queue replaces rather than accumulates.
     */
    private suspend fun uploadProof(orderId: String?, path: String): ProofOutcome {
        if (orderId.isNullOrBlank()) return ProofOutcome.Rejected
        val file = java.io.File(path)
        // The file is gone — the cache was cleared, or the encode never landed.
        // Nothing to retry forever over.
        if (!file.exists()) return ProofOutcome.Rejected

        val part = MultipartBody.Part.createFormData(
            "file",
            file.name,
            file.asRequestBody("image/webp".toMediaType()),
        )

        val result = runCatching { api.uploadProof(orderId, part) }
        val code = result.getOrNull()?.code() ?: return ProofOutcome.Retry

        return when {
            code in 200..299 -> {
                // Delete on success: a proof that reached the server has no
                // reason to keep occupying a courier's cache.
                file.delete()
                ProofOutcome.Sent
            }
            // 4xx will not change on retry.
            code in 400..499 -> ProofOutcome.Rejected
            else -> ProofOutcome.Retry
        }
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
