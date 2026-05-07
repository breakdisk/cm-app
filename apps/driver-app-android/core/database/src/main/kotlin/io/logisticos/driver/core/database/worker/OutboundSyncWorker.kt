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
import io.logisticos.driver.core.network.service.AttachPhotoRequest
import io.logisticos.driver.core.network.service.AttachSignatureRequest
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.FailTaskRequest
import io.logisticos.driver.core.network.service.GetUploadUrlRequest
import io.logisticos.driver.core.network.service.InitiatePodRequest
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SubmitPodRequest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
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
    private val podApi: PodApiService,
    private val okHttpClient: OkHttpClient,
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
                    else -> syncQueueDao.remove(item.id)
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

                val photoFile = pod.photoPath?.let { File(it).takeIf { f -> f.exists() } }
                val sigFile   = pod.signaturePath?.let { File(it).takeIf { f -> f.exists() } }

                // 1. Initiate — requires_* flags reflect what evidence is actually on disk.
                //    Pickup tasks have no signature; photos are optional on both task types.
                //    Setting these correctly prevents "POD incomplete" on submit.
                val initiateResp = podApi.initiate(
                    InitiatePodRequest(
                        shipmentId        = task.shipmentId,
                        taskId            = taskId,
                        recipientName     = task.recipientName,
                        captureLat        = task.lat,
                        captureLng        = task.lng,
                        deliveryLat       = task.lat,
                        deliveryLng       = task.lng,
                        requiresPhoto     = photoFile != null,
                        requiresSignature = sigFile != null,
                    )
                )
                val podId = initiateResp.data.podId

                // 2. Upload photo via presigned R2 URL if the file is available.
                //    Runs on IO because OkHttp .execute() is blocking.
                if (photoFile != null) {
                    val contentType = "image/jpeg"
                    val uploadResp = podApi.getUploadUrl(podId, GetUploadUrlRequest(contentType))
                    val presignedUrl = uploadResp.data.uploadUrl
                    val s3Key = uploadResp.data.s3Key

                    withContext(Dispatchers.IO) {
                        val photoBytes = photoFile.readBytes()
                        val putRequest = Request.Builder()
                            .url(presignedUrl)
                            .put(photoBytes.toRequestBody(contentType.toMediaType()))
                            .build()
                        val putResponse = okHttpClient.newCall(putRequest).execute()
                        if (!putResponse.isSuccessful) {
                            val body = try { putResponse.body?.string() ?: "empty" } catch (e: Exception) { "unreadable" }
                            android.util.Log.e("OutboundSyncWorker", "R2 PUT ${putResponse.code}: $body")
                            error("R2 photo upload failed: ${putResponse.code}: $body")
                        }
                        putResponse.close()
                    }

                    podApi.attachPhoto(podId, AttachPhotoRequest(
                        s3Key       = s3Key,
                        contentType = contentType,
                        sizeBytes   = photoFile.length(),
                    ))
                    android.util.Log.d("OutboundSyncWorker", "Photo uploaded: $s3Key")
                }

                // 3. Attach signature if available
                if (sigFile != null) {
                    val base64 = Base64.encodeToString(sigFile.readBytes(), Base64.NO_WRAP)
                    podApi.attachSignature(podId, AttachSignatureRequest(base64))
                }

                // 4. Submit POD
                podApi.submit(podId, SubmitPodRequest(otpCode = pod.otpToken))

                // 4b. Mark POD as synced now that it's on the server.
                podDao.markSynced(taskId)

                // 5. Enqueue TASK_COMPLETE (step 7) separately so it has its own retry lifecycle.
                //    The task is already COMPLETED locally; this just confirms it with the backend.
                taskDao.updateStatusWithSync(
                    taskId,
                    io.logisticos.driver.core.database.entity.TaskStatus.COMPLETED,
                    isSynced = false,
                )
                syncQueueDao.enqueue(
                    io.logisticos.driver.core.database.entity.SyncQueueEntity(
                        action = io.logisticos.driver.core.database.entity.SyncAction.TASK_COMPLETE,
                        payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "podId" to podId)),
                        createdAt = System.currentTimeMillis(),
                    )
                )
                OutboundSyncWorker.kickOnce(applicationContext)
            }

            SyncAction.TASK_COMPLETE -> {
                val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val podId = payload["podId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }

                // After 7 days with no success, the backend may have auto-cancelled the task.
                // Mark locally as permanently failed so the driver knows to contact support.
                val sevenDaysMs = 7L * 24 * 60 * 60 * 1_000
                if (item.createdAt < System.currentTimeMillis() - sevenDaysMs) {
                    taskDao.markSyncFailed(taskId)
                    syncQueueDao.remove(item.id)
                    return
                }

                driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId))
                taskDao.markSynced(taskId)
                podDao.markSynced(taskId)
            }

            // Actions with no backend wiring. Log and drop deliberately so they
            // don't block the queue forever.
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
