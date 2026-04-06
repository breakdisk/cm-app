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
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.RequestBody.Companion.asRequestBody
import okhttp3.RequestBody.Companion.toRequestBody
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
                val backoffMs = minOf(1000L * (1 shl item.retryCount), 300_000L)
                syncQueueDao.markFailed(item.id, e.message ?: "unknown", System.currentTimeMillis() + backoffMs)
            }
        }
        return Result.success()
    }

    private suspend fun processItem(item: SyncQueueEntity) {
        val payload = Json.parseToJsonElement(item.payloadJson).jsonObject
        when (item.action) {
            SyncAction.TASK_STATUS_UPDATE -> {
                val taskId = payload["taskId"]!!.jsonPrimitive.content
                val status = payload["status"]!!.jsonPrimitive.content
                driverOpsApi.updateTaskStatus(taskId, TaskStatusRequest(status = status))
            }
            SyncAction.POD_SUBMIT -> {
                val taskId = payload["taskId"]!!.jsonPrimitive.content
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
                podApi.submitPod(
                    taskId = taskId.toRequestBody("text/plain".toMediaType()),
                    photo = photoBody,
                    signature = sigBody,
                    otpToken = pod.otpToken?.toRequestBody("text/plain".toMediaType())
                )
                podDao.markSynced(taskId)
            }
            else -> Unit
        }
    }

    companion object {
        const val WORK_NAME = "outbound_sync"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<OutboundSyncWorker>(60, TimeUnit.SECONDS)
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
