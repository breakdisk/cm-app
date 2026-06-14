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
import io.logisticos.driver.core.common.ImageCompressor
import io.logisticos.driver.core.common.TaskSyncBus
import io.logisticos.driver.core.database.dao.LocationBreadcrumbDao
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import android.util.Base64
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.network.auth.SessionManager
import io.logisticos.driver.core.network.service.AttachPhotoRequest
import io.logisticos.driver.core.network.service.AttachSignatureRequest
import io.logisticos.driver.core.network.service.BulkLocationRequest
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.HubOpsApiService
import io.logisticos.driver.core.network.service.RecordScanRequest
import io.logisticos.driver.core.network.service.FailTaskRequest
import io.logisticos.driver.core.network.service.GetUploadUrlRequest
import io.logisticos.driver.core.network.service.InitiatePopRequest
import io.logisticos.driver.core.network.service.InitiatePodRequest
import io.logisticos.driver.core.network.service.LocationBreadcrumb
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SubmitPopRequest
import io.logisticos.driver.core.network.service.SubmitPodRequest
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
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
    private val locationDao: LocationBreadcrumbDao,
    private val driverOpsApi: DriverOpsApiService,
    private val podApi: PodApiService,
    private val hubOpsApi: HubOpsApiService,
    private val okHttpClient: OkHttpClient,
    private val sessionManager: SessionManager,
) : CoroutineWorker(context, workerParams) {

    override suspend fun doWork(): Result {
        // Guard: skip all network calls if the user is not logged in.
        // Without this, the worker floods the server with unauthenticated 401
        // requests after logout — saturating the OkHttp connection pool and
        // blocking the OTP send coroutine on the login screen from getting a
        // connection slot. Queued items remain in the DB and will be retried
        // automatically the next time the user logs in and the periodic
        // 15-minute worker fires (or the next kickOnce from a fresh enqueue).
        if (!sessionManager.isLoggedIn()) {
            android.util.Log.d("OutboundSyncWorker", "No active session — skipping sync, items remain queued")
            return Result.success()
        }
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
                        val popId = payload["popId"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
                        driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId, popId = popId))
                        TaskSyncBus.requestSync()  // backend now excludes this task — refresh Home
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
                //    Compress to ≤800 KB JPEG before upload to stay within Cloudflare
                //    R2 body limits and keep downstream OCR/display snappy.
                //    Runs on IO because ImageCompressor and OkHttp are both blocking.
                if (photoFile != null) {
                    val contentType = "image/jpeg"

                    val uploadResp = podApi.getUploadUrl(podId, GetUploadUrlRequest(contentType))
                    val presignedUrl = uploadResp.data.uploadUrl
                    val s3Key = uploadResp.data.s3Key
                    val requiredHeaders = uploadResp.data.uploadHeaders

                    withContext(Dispatchers.IO) {
                        // Compress in-place before reading bytes for upload.
                        // If the file is already ≤800 KB this is still a fast no-op
                        // (BitmapFactory decode + single-pass compress at quality 85).
                        ImageCompressor.compressToFile(photoFile)

                        val photoBytes = photoFile.readBytes()
                        android.util.Log.d(
                            "OutboundSyncWorker",
                            "Photo compressed: ${photoBytes.size / 1024} KB (limit ${ImageCompressor.MAX_SIZE_BYTES / 1024} KB)"
                        )

                        val reqBuilder = Request.Builder()
                            .url(presignedUrl)
                            .put(photoBytes.toRequestBody(contentType.toMediaType()))
                        // Add any headers the backend says must accompany the presigned PUT.
                        // For R2 presigned URLs the backend returns only headers that are
                        // genuinely signed (in X-Amz-SignedHeaders). An empty map is valid —
                        // the presigned URL itself carries the UNSIGNED-PAYLOAD indicator and
                        // R2 does not require x-amz-content-sha256 as a separate header when
                        // the URL is already signed correctly with the right 32-char R2 key.
                        requiredHeaders.forEach { (k, v) -> reqBuilder.addHeader(k, v) }
                        val putResponse = okHttpClient.newCall(reqBuilder.build()).execute()
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

                // 4c. Flush offline GPS breadcrumbs accumulated since last sync.
                //     Runs right after POD submit so chain-of-custody telemetry
                //     reaches the server before the task is marked complete.
                //     Non-fatal: a failed flush must never roll back the POD —
                //     the breadcrumbs remain in the local DB and will be retried
                //     on the next periodic worker tick.
                try {
                    val crumbs = locationDao.getUnsynced()
                    if (crumbs.isNotEmpty()) {
                        val bulk = BulkLocationRequest(crumbs.map { c ->
                            LocationBreadcrumb(
                                lat        = c.lat,
                                lng        = c.lng,
                                accuracyM  = c.accuracy,
                                speedKmh   = c.speedMps * 3.6f,
                                heading    = c.bearing,
                                recordedAt = DateTimeFormatter.ISO_INSTANT
                                    .format(Instant.ofEpochMilli(c.timestamp)),
                            )
                        })
                        driverOpsApi.bulkUpdateLocation(bulk)
                        locationDao.markSynced(crumbs.map { it.id })
                        android.util.Log.d(
                            "OutboundSyncWorker",
                            "GPS flush: ${crumbs.size} breadcrumbs sent"
                        )
                    }
                } catch (e: Exception) {
                    android.util.Log.w(
                        "OutboundSyncWorker",
                        "GPS breadcrumb flush failed — non-fatal: ${e.message}"
                    )
                }

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

            // Offline fallback for PickupRepository.confirmPickup() when the inline
            // POP API call failed (no connectivity at the moment of driver tap).
            // Calls initiate + optional photo upload + submit sequentially.
            // Task completion is handled by the separate TASK_STATUS_UPDATE entry.
            SyncAction.POP_SUBMIT -> {
                val taskId     = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val shipmentId = payload["shipmentId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val scannedBarcode = payload["scannedBarcode"]?.jsonPrimitive?.contentOrNull ?: ""
                val captureLat = payload["captureLat"]?.jsonPrimitive?.contentOrNull?.toDoubleOrNull() ?: 0.0
                val captureLng = payload["captureLng"]?.jsonPrimitive?.contentOrNull?.toDoubleOrNull() ?: 0.0
                val pickupLat  = payload["pickupLat"]?.jsonPrimitive?.contentOrNull?.toDoubleOrNull()  ?: captureLat
                val pickupLng  = payload["pickupLng"]?.jsonPrimitive?.contentOrNull?.toDoubleOrNull()  ?: captureLng
                val deviceTs   = payload["deviceTimestamp"]?.jsonPrimitive?.contentOrNull
                val photoPath  = payload["photoPath"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
                // AR dimensioning carried through the offline replay (blank → null).
                fun dimD(k: String) = payload[k]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }?.toDoubleOrNull()
                val verifiedLengthCm   = dimD("verifiedLengthCm")
                val verifiedWidthCm    = dimD("verifiedWidthCm")
                val verifiedHeightCm   = dimD("verifiedHeightCm")
                val verifiedCbm        = dimD("verifiedCbm")
                val volumetricWeightKg = dimD("volumetricWeightKg")
                val boxQuantity        = payload["boxQuantity"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }?.toIntOrNull()
                val dimensionIntegrity = payload["dimensionIntegrity"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }

                val popResp = podApi.initiatePop(
                    InitiatePopRequest(
                        shipmentId      = shipmentId,
                        taskId          = taskId,
                        captureLat      = captureLat,
                        captureLng      = captureLng,
                        pickupLat       = pickupLat,
                        pickupLng       = pickupLng,
                        deviceTimestamp = deviceTs,
                    )
                )
                val popId = popResp.data.popId

                // Upload pickup photo to R2 if one was captured offline.
                // Mirrors the POD_SUBMIT photo upload path; failure is non-fatal
                // (photo evidence is optional for POP) so we log and continue.
                var photoS3Key: String? = null
                var photoSizeBytes: Long? = null
                if (photoPath != null) {
                    val photoFile = java.io.File(photoPath).takeIf { it.exists() }
                    if (photoFile != null) {
                        try {
                            val contentType = "image/jpeg"
                            withContext(Dispatchers.IO) { ImageCompressor.compressToFile(photoFile) }
                            val uploadResp = podApi.getPopUploadUrl(
                                popId,
                                GetUploadUrlRequest(contentType)
                            )
                            val photoBytes = withContext(Dispatchers.IO) { photoFile.readBytes() }
                            android.util.Log.d(
                                "OutboundSyncWorker",
                                "POP photo: ${photoBytes.size / 1024} KB → R2"
                            )
                            val reqBuilder = Request.Builder()
                                .url(uploadResp.data.uploadUrl)
                                .put(photoBytes.toRequestBody(contentType.toMediaType()))
                            uploadResp.data.uploadHeaders.forEach { (k, v) -> reqBuilder.addHeader(k, v) }
                            val putResponse = okHttpClient.newCall(reqBuilder.build()).execute()
                            if (putResponse.isSuccessful) {
                                putResponse.close()
                                photoS3Key    = uploadResp.data.s3Key
                                photoSizeBytes = photoFile.length()
                                android.util.Log.d("OutboundSyncWorker", "POP photo uploaded: $photoS3Key")
                            } else {
                                val body = try { putResponse.body?.string() ?: "empty" } catch (e: Exception) { "unreadable" }
                                android.util.Log.e("OutboundSyncWorker", "R2 POP photo PUT ${putResponse.code}: $body")
                            }
                        } catch (e: Exception) {
                            android.util.Log.w("OutboundSyncWorker", "POP photo upload failed — non-fatal: ${e.message}")
                        }
                    }
                }

                podApi.submitPop(
                    popId,
                    SubmitPopRequest(
                        scannedBarcode     = scannedBarcode,
                        deviceTimestamp    = deviceTs,
                        photoS3Key         = photoS3Key,
                        photoSizeBytes     = photoSizeBytes,
                        verifiedLengthCm   = verifiedLengthCm,
                        verifiedWidthCm    = verifiedWidthCm,
                        verifiedHeightCm   = verifiedHeightCm,
                        verifiedCbm        = verifiedCbm,
                        volumetricWeightKg = volumetricWeightKg,
                        boxQuantity        = boxQuantity,
                        dimensionIntegrity = dimensionIntegrity,
                    )
                )
                android.util.Log.d(
                    "OutboundSyncWorker",
                    "POP submitted: pop_id=$popId task=$taskId has_photo=${photoS3Key != null}"
                )

                // Complete the task with the now-known pop_id. If the inline call fails,
                // enqueue TASK_COMPLETE so the pop_id is carried through on retry.
                // This mirrors the POD_SUBMIT → TASK_COMPLETE pattern for delivery tasks.
                try {
                    driverOpsApi.completeTask(taskId, CompleteTaskRequest(popId = popId))
                    taskDao.markSynced(taskId)
                    TaskSyncBus.requestSync()  // refresh Home once the pickup completes server-side
                } catch (e: Exception) {
                    android.util.Log.w("OutboundSyncWorker", "completeTask after POP_SUBMIT failed — queuing TASK_COMPLETE: ${e.message}")
                    syncQueueDao.enqueue(
                        SyncQueueEntity(
                            action      = SyncAction.TASK_COMPLETE,
                            payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "popId" to popId)),
                            createdAt   = System.currentTimeMillis(),
                        )
                    )
                    OutboundSyncWorker.kickOnce(applicationContext)
                }
            }

            SyncAction.TASK_COMPLETE -> {
                val taskId = payload["taskId"]?.jsonPrimitive?.contentOrNull
                    ?: run { syncQueueDao.remove(item.id); return }
                val podId = payload["podId"]?.jsonPrimitive?.contentOrNull
                val popId = payload["popId"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }

                // Either a pod_id (delivery) or pop_id (pickup) is required.
                if (podId == null && popId == null) {
                    syncQueueDao.remove(item.id)
                    return
                }

                // After 7 days with no success, the backend may have auto-cancelled the task.
                // Mark locally as permanently failed so the driver knows to contact support.
                val sevenDaysMs = 7L * 24 * 60 * 60 * 1_000
                if (item.createdAt < System.currentTimeMillis() - sevenDaysMs) {
                    taskDao.markSyncFailed(taskId)
                    syncQueueDao.remove(item.id)
                    return
                }

                driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId, popId = popId))
                taskDao.markSynced(taskId)
                TaskSyncBus.requestSync()  // offline completion confirmed — drop the Home card
                val now = System.currentTimeMillis()
                if (podId != null) {
                    taskDao.setPodId(taskId, podId, now)
                    podDao.markSynced(taskId)
                } else if (popId != null) {
                    taskDao.setPopId(taskId, popId, now)
                }
            }

            SyncAction.SHIFT_START -> {
                // Retry POST /v1/drivers/go-online that failed in HomeViewModel
                // (network unavailable at toggle time). The worker removes this item
                // on success, so the driver's availability flips server-side as soon
                // as connectivity returns — dispatch's proximity query can then find them.
                driverOpsApi.goOnline()
            }

            SyncAction.SHIFT_END -> {
                // Mirror of SHIFT_START — retry POST /v1/drivers/go-offline.
                driverOpsApi.goOffline()
            }

            SyncAction.HUB_SCAN -> {
                // Replay a hub chain-of-custody scan that was queued while offline.
                // device_timestamp is preserved from the original scan moment — not re-sampled.
                val hubId      = payload["hub_id"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val pieceAwb   = payload["piece_awb"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val masterAwb  = payload["master_awb"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val shipmentId = payload["shipment_id"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val scanType   = payload["scan_type"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val deviceTs   = payload["device_timestamp"]?.jsonPrimitive?.contentOrNull ?: run { syncQueueDao.remove(item.id); return }
                val palletId   = payload["pallet_id"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
                val containerId = payload["container_id"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
                val exception  = payload["exception"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }

                hubOpsApi.recordScan(
                    RecordScanRequest(
                        hubId           = hubId,
                        pieceAwb        = pieceAwb,
                        masterAwb       = masterAwb,
                        shipmentId      = shipmentId,
                        scanType        = scanType,
                        deviceTimestamp = deviceTs,
                        palletId        = palletId,
                        containerId     = containerId,
                        exception       = exception,
                    )
                )
                android.util.Log.d("OutboundSyncWorker", "HUB_SCAN replayed: type=$scanType awb=$pieceAwb")
            }

            // Any future SyncAction variants without a handler yet.
            // Log and drop deliberately so they don't block the queue forever.
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
