package io.logisticos.driver.core.database.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.TaskStatusRequest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.asRequestBody
import okhttp3.RequestBody.Companion.toRequestBody
import retrofit2.HttpException
import java.io.File
import java.util.concurrent.TimeUnit

@HiltWorker
class OutboundSyncWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted workerParams: WorkerParameters,
    private val syncQueueDao: SyncQueueDao,
    private val podDao: PodDao,
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
                driverOpsApi.updateTaskStatus(taskId, TaskStatusRequest(status = status))
            }
            SyncAction.POD_SUBMIT -> {
                val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val pod = podDao.getForTask(taskId) ?: return
                val photoBody = pod.photoPath?.let { path ->
                    val file = File(path)
                    if (file.exists()) {
                        MultipartBody.Part.createFormData(
                            "photo", file.name,
                            file.asRequestBody("image/jpeg".toMediaType())
                        )
                    } else null
                }
                val sigBody = pod.signaturePath?.let { path ->
                    val file = File(path)
                    if (file.exists()) {
                        MultipartBody.Part.createFormData(
                            "signature", file.name,
                            file.asRequestBody("image/png".toMediaType())
                        )
                    } else null
                }
                val response = podApi.submitPod(
                    taskId = taskId.toRequestBody("text/plain".toMediaType()),
                    photo = photoBody,
                    signature = sigBody,
                    otpToken = pod.otpToken?.toRequestBody("text/plain".toMediaType())
                )
                if (response.isSuccessful) {
                    podDao.markSynced(taskId)
                } else if (response.code() in 400..499 && response.code() != 429) {
                    // Unrecoverable client error — discard queue item permanently
                    syncQueueDao.remove(item.id)
                    return
                }
                // On 5xx or 429, throw to trigger retry with backoff
                if (!response.isSuccessful) throw HttpException(response)
            }
            else -> Unit
        }
    }

    companion object {
        const val WORK_NAME = "outbound_sync"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<OutboundSyncWorker>(15, TimeUnit.MINUTES)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request
            )
        }
    }
}
