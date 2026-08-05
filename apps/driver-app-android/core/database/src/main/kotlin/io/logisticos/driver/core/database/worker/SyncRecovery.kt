package io.logisticos.driver.core.database.worker

import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Recovers tasks that [OutboundSyncWorker] abandoned after they exhausted the
 * retry window.
 *
 * Lives beside the worker rather than in the UI layer because it has to build the
 * same queue payloads the worker parses — keeping the two together means a change
 * to one shape cannot silently desync from the other.
 *
 * Needed because clearing queue backoff is not sufficient on its own: an expired
 * item is *removed* from the queue and the task flagged, so there is nothing left
 * for a backoff reset to revive. Without this the driver-facing Retry silently
 * does nothing for exactly the tasks that most need it — ones where the work is
 * done and the evidence is still sitting on the device.
 */
@Singleton
class SyncRecovery @Inject constructor(
    private val taskDao: TaskDao,
    private val syncQueueDao: SyncQueueDao,
) {

    /**
     * Re-enqueues a sync item for every task flagged as sync-failed and clears the
     * flag. Returns how many were re-queued.
     *
     * The item chosen depends on how far the original attempt got:
     *  - a `pod_id` / `pop_id` on the task means the proof reached the server and
     *    only the completion call failed, so [SyncAction.TASK_COMPLETE] is enough;
     *  - otherwise the proof itself never landed, and the full
     *    [SyncAction.POD_SUBMIT] replay is required — that path re-reads the photo
     *    and signature from local storage and re-uploads them.
     */
    suspend fun requeueAbandonedTasks(): Int {
        val abandoned = taskDao.getSyncFailed()
        if (abandoned.isEmpty()) return 0

        abandoned.forEach { task ->
            val action: SyncAction
            val payload: String
            when {
                task.podId != null -> {
                    action = SyncAction.TASK_COMPLETE
                    payload = Json.encodeToString(
                        mapOf("taskId" to task.id, "podId" to task.podId)
                    )
                }
                task.popId != null -> {
                    action = SyncAction.TASK_COMPLETE
                    payload = Json.encodeToString(
                        mapOf("taskId" to task.id, "popId" to task.popId)
                    )
                }
                else -> {
                    action = SyncAction.POD_SUBMIT
                    payload = Json.encodeToString(mapOf("taskId" to task.id))
                }
            }
            syncQueueDao.enqueue(
                SyncQueueEntity(
                    action      = action,
                    payloadJson = payload,
                    // Stamped now rather than carrying the original time, so the
                    // re-queued item starts a fresh retry window instead of being
                    // discarded on the next run as already-expired.
                    createdAt   = System.currentTimeMillis(),
                )
            )
            taskDao.clearSyncFailed(task.id)
        }
        android.util.Log.d("SyncRecovery", "Re-queued ${abandoned.size} abandoned task(s)")
        return abandoned.size
    }
}
