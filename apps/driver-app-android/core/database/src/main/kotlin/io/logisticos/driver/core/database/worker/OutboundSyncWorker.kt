package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import android.util.Base64
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.network.service.AttachSignatureRequest
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.FailTaskRequest
import io.logisticos.driver.core.network.service.InitiatePodRequest
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SubmitPodRequest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.util.concurrent.TimeUnit

@HiltWorker
class OutboundSyncWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted workerParams: WorkerParameters,
    private val syncQueueDao: SyncQueueDao,
    private val podDao: PodDao,
    private val taskDao: TaskDao,
    private val driverOpsApi: DriverOpsApiService,
    private val podApi: PodApiService
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): Result {
        val pending = syncQueueDao.getPendingItems(System.currentTimeMillis())
        pending.forEach { item ->
            try {
                processItem(item)
                syncQueueDao.remove(item.id)
            } catch (e: Exception) {
                val backoffMs = minOf(1000L shl minOf(item.retryCount, 8), 300_000L)
                syncQueueDao.markFailed(item.id, e.message ?: "unknown", System.currentTimeMillis() + backoffMs)
            }
        }
        return Result.success()
    }

    private suspend fun processItem(item: SyncQueueEntity) {
        val payload = runCatching { Json.parseToJsonElement(item.payloadJson).jsonObject }.getOrNull()
        if (payload == null) {
            syncQueueDao.remove(item.id) // malformed JSON — discard permanently
            return
        }
        when (item.action) {
            SyncAction.TASK_STATUS_UPDATE -> {
                val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val status = payload["status"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val reason = payload["reason"]?.jsonPrimitive?.contentOrNull

                when (status.uppercase()) {
                    "IN_PROGRESS" -> driverOpsApi.startTask(taskId)
                    "COMPLETED"   -> {
                        val podId = payload["podId"]?.jsonPrimitive?.contentOrNull
                        driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId))
                    }
                    "FAILED"      -> {
                        driverOpsApi.failTask(taskId, FailTaskRequest(reason = reason ?: "unknown"))
                    }
                    else -> syncQueueDao.remove(item.id)   // unknown status — discard
                }
            }
            SyncAction.POD_SUBMIT -> {
                val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val pod = podDao.getForTask(taskId) ?: run {
                    syncQueueDao.remove(item.id); return
                }
                val task = taskDao.getById(taskId) ?: run {
                    syncQueueDao.remove(item.id); return
                }

                // 1. Initiate — use task's stored destination coords as best available
                val initiateResp = podApi.initiate(
                    InitiatePodRequest(
                        shipmentId = task.shipmentId,
                        taskId = taskId,
                        recipientName = task.recipientName,
                        captureLat = task.lat,
                        captureLng = task.lng,
                        deliveryLat = task.lat,
                        deliveryLng = task.lng
                    )
                )
                val podId = initiateResp.data.podId

                // 2. Attach signature if available
                if (pod.signaturePath != null) {
                    val sigFile = File(pod.signaturePath)
                    if (sigFile.exists()) {
                        val base64 = Base64.encodeToString(sigFile.readBytes(), Base64.NO_WRAP)
                        podApi.attachSignature(podId, AttachSignatureRequest(base64))
                    }
                }

                // 3. Submit POD
                podApi.submit(podId, SubmitPodRequest(otpCode = pod.otpToken))

                // 4. Complete the task
                driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId))

                podDao.markSynced(taskId)
            }
            // Actions with no backend wiring (SCAN_EVENT, SHIFT_START, SHIFT_END,
            // and historically COD_CONFIRM — whose value is actually delivered via
            // completeTask.codCollectedCents). Log and drop deliberately so they
            // don't block the queue forever — but keep this branch loud so a
            // future enum addition is caught in code review, not in production
            // silent data loss.
            else -> {
                android.util.Log.w(
                    "OutboundSyncWorker",
                    "no handler for ${item.action}; dropping queue id=${item.id}"
                )
            }
        }
    }

    companion object {
        const val WORK_NAME           = "outbound_sync"
        const val ONE_SHOT_WORK_NAME  = "outbound_sync_one_shot"

        private fun networkConstraints() = Constraints.Builder()
            .setRequiredNetworkType(NetworkType.CONNECTED)
            .build()

        /** Periodic safety net — fires every 15 min while online. Drains
         *  anything kickOnce missed (app killed mid-flight, doze deferral, etc). */
        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<OutboundSyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(networkConstraints())
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request
            )
        }

        /**
         * Immediate retry trigger — call after enqueueing into SyncQueueDao so
         * the item ships within seconds of network return rather than waiting
         * up to 15 min for the next periodic tick. WorkManager dedupes by name
         * (REPLACE), so multiple rapid enqueues collapse into one run.
         */
        fun kickOnce(context: Context) {
            val request = OneTimeWorkRequestBuilder<OutboundSyncWorker>()
                .setConstraints(networkConstraints())
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                .build()
            WorkManager.getInstance(context).enqueueUniqueWork(
                ONE_SHOT_WORK_NAME, ExistingWorkPolicy.REPLACE, request
            )
        }
    }
}
