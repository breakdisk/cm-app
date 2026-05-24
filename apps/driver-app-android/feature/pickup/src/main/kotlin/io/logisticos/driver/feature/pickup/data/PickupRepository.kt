package io.logisticos.driver.feature.pickup.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.common.ImageCompressor
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.GetUploadUrlRequest
import io.logisticos.driver.core.network.service.InitiatePopRequest
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SubmitPopRequest
import io.logisticos.driver.feature.delivery.domain.TaskStateMachine
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import javax.inject.Inject
import javax.inject.Named

class PickupRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val taskDao: TaskDao,
    private val syncQueueDao: SyncQueueDao,
    private val driverOpsApi: DriverOpsApiService,
    private val podApi: PodApiService,
    /** Dedicated client for R2 presigned PUT uploads — longer timeouts, no API interceptors. */
    @Named("r2_upload") private val r2HttpClient: OkHttpClient,
) {
    fun observeTask(taskId: String): Flow<TaskEntity?> = taskDao.getByIdAsFlow(taskId)

    /** Enqueue an item AND immediately kick a one-time worker so it ships
     *  within seconds of network return — not 15 min later on the next
     *  periodic tick. */
    private suspend fun enqueueAndKick(item: SyncQueueEntity) {
        syncQueueDao.enqueue(item)
        OutboundSyncWorker.kickOnce(context)
    }

    /**
     * Transitions task to IN_PROGRESS locally and on the backend.
     * Called when the pickup screen opens. Falls back to sync queue on network error.
     */
    suspend fun transitionToInProgress(taskId: String) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, TaskStatus.IN_PROGRESS)) return
        taskDao.updateStatus(taskId, TaskStatus.IN_PROGRESS)
        try {
            driverOpsApi.startTask(taskId)
        } catch (e: Exception) {
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.TASK_STATUS_UPDATE,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "status" to TaskStatus.IN_PROGRESS.name)
                    ),
                    createdAt = System.currentTimeMillis()
                )
            )
        }
    }

    /**
     * Completes a pickup task:
     *
     * 1. Marks task COMPLETED locally.
     * 2. Submits Proof of Pickup to the backend (POST /v1/pops → PUT /v1/pops/:id/submit).
     *    On failure, enqueues POP_SUBMIT so the worker retries on reconnect.
     * 3. Calls completeTask on the backend.
     *    On failure, enqueues TASK_STATUS_UPDATE so the task flips server-side
     *    as soon as connectivity returns.
     *
     * POP and task completion are separate queue items — POP failure does not
     * block the task from completing, and vice versa.
     *
     * @param scannedAwb   The barcode value actually scanned; if empty (auto-confirmed
     *                     UUID AWB), pass the task AWB string as the record value.
     * @param captureLat   Driver's latitude at moment of tap — use task lat as fallback.
     * @param captureLng   Driver's longitude at moment of tap — use task lng as fallback.
     * @param pickupLat    Pickup address latitude (task.lat) for geofence calculation.
     * @param pickupLng    Pickup address longitude (task.lng) for geofence calculation.
     * @param photoPath    Absolute path to the compressed parcel photo captured on the
     *                     pickup screen. Null when the driver skipped the photo step.
     *                     The file is compressed to ≤800 KB before POP submission.
     *                     Upload to the backend is enqueued in the sync queue so it
     *                     survives connectivity gaps.
     * @param deviceTimestamp ISO-8601 UTC string captured at the moment the driver taps
     *                     Confirm — used as the canonical chain-of-custody timestamp.
     */
    suspend fun confirmPickup(
        taskId: String,
        shipmentId: String,
        scannedAwb: String,
        captureLat: Double,
        captureLng: Double,
        pickupLat: Double,
        pickupLng: Double,
        photoPath: String?,
        deviceTimestamp: String,
    ) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, TaskStatus.COMPLETED)) return
        taskDao.updateStatus(taskId, TaskStatus.COMPLETED)

        // ── 0. Compress pickup photo if captured ────────────────────────────
        // Compress before POP submission so the file is always ≤800 KB on disk.
        // Non-fatal: a compress failure must not block the task from completing.
        val compressedPhotoPath: String? = photoPath?.let { path ->
            runCatching {
                val file = File(path).takeIf { it.exists() } ?: return@let null
                withContext(Dispatchers.IO) { ImageCompressor.compressToFile(file) }
                path
            }.getOrNull()
        }

        // ── 1. Proof of Pickup ──────────────────────────────────────────────
        // Try inline first; fall back to sync queue on any network error.
        // The server requires `scanned_barcode` — use the task AWB when no
        // real scan happened (auto-confirmed UUID AWBs).
        val effectiveBarcode = scannedAwb.ifBlank { task.awb }
        try {
            val popResp = podApi.initiatePop(
                InitiatePopRequest(
                    shipmentId      = shipmentId,
                    taskId          = taskId,
                    captureLat      = captureLat,
                    captureLng      = captureLng,
                    pickupLat       = pickupLat,
                    pickupLng       = pickupLng,
                    deviceTimestamp = deviceTimestamp,
                )
            )
            val popId = popResp.data.popId

            // ── Upload pickup photo to R2 if captured ───────────────────────
            // Uses the same presigned-URL flow as delivery POD photos.
            // Failure is non-fatal for the POP record (photo is optional) but
            // we log it so ops can investigate missing pickup evidence.
            var photoS3Key: String? = null
            var photoSizeBytes: Long? = null
            val photoFile = compressedPhotoPath?.let { File(it).takeIf { f -> f.exists() } }
            if (photoFile != null) {
                try {
                    val contentType = "image/jpeg"
                    val uploadResp = podApi.getPopUploadUrl(popId, GetUploadUrlRequest(contentType))
                    withContext(Dispatchers.IO) {
                        val photoBytes = photoFile.readBytes()
                        val reqBuilder = Request.Builder()
                            .url(uploadResp.data.uploadUrl)
                            .put(photoBytes.toRequestBody(contentType.toMediaType()))
                        uploadResp.data.uploadHeaders.forEach { (k, v) -> reqBuilder.addHeader(k, v) }
                        val putResponse = r2HttpClient.newCall(reqBuilder.build()).execute()
                        if (!putResponse.isSuccessful) {
                            val body = try { putResponse.body?.string() ?: "empty" } catch (e: Exception) { "unreadable" }
                            android.util.Log.e("PickupRepository", "R2 POP photo PUT ${putResponse.code}: $body")
                        } else {
                            putResponse.close()
                            photoS3Key = uploadResp.data.s3Key
                            photoSizeBytes = photoFile.length()
                            android.util.Log.d("PickupRepository", "POP photo uploaded: ${uploadResp.data.s3Key}")
                        }
                    }
                } catch (e: Exception) {
                    android.util.Log.w("PickupRepository", "POP photo upload failed — non-fatal: ${e.message}")
                }
            }

            podApi.submitPop(
                popId,
                SubmitPopRequest(
                    scannedBarcode  = effectiveBarcode,
                    deviceTimestamp = deviceTimestamp,
                    photoS3Key      = photoS3Key,
                    photoSizeBytes  = photoSizeBytes,
                )
            )
            android.util.Log.d(
                "PickupRepository",
                "POP submitted inline: pop_id=$popId " +
                "geofence=${popResp.data.geofenceVerified} " +
                "out_of_bounds=${popResp.data.outOfBoundsHandover} " +
                "has_photo=${photoS3Key != null}"
            )
        } catch (e: Exception) {
            android.util.Log.w("PickupRepository", "POP inline failed — queuing: ${e.message}")
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.POP_SUBMIT,
                    payloadJson = Json.encodeToString(
                        mapOf(
                            "taskId"          to taskId,
                            "shipmentId"      to shipmentId,
                            "scannedBarcode"  to effectiveBarcode,
                            "captureLat"      to captureLat.toString(),
                            "captureLng"      to captureLng.toString(),
                            "pickupLat"       to pickupLat.toString(),
                            "pickupLng"       to pickupLng.toString(),
                            "photoPath"       to (compressedPhotoPath ?: ""),
                            "deviceTimestamp" to deviceTimestamp,
                        )
                    ),
                    createdAt = System.currentTimeMillis(),
                )
            )
        }

        // ── 2. Task completion ──────────────────────────────────────────────
        try {
            driverOpsApi.completeTask(taskId, CompleteTaskRequest())
        } catch (e: Exception) {
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.TASK_STATUS_UPDATE,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "status" to TaskStatus.COMPLETED.name)
                    ),
                    createdAt = System.currentTimeMillis(),
                )
            )
        }
    }
}
