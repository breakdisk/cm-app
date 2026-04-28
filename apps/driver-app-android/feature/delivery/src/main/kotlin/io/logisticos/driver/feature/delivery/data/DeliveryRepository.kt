package io.logisticos.driver.feature.delivery.data

import android.content.Context
import android.util.Base64
import dagger.hilt.android.qualifiers.ApplicationContext
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.PodEntity
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.database.worker.OutboundSyncWorker
import io.logisticos.driver.core.network.service.AttachSignatureRequest
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.core.network.service.InitiatePodRequest
import io.logisticos.driver.core.network.service.PodApiService
import io.logisticos.driver.core.network.service.SubmitPodRequest
import io.logisticos.driver.feature.delivery.domain.TaskStateMachine
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File
import javax.inject.Inject

class DeliveryRepository @Inject constructor(
    @ApplicationContext private val context: Context,
    private val taskDao: TaskDao,
    private val podDao: PodDao,
    private val shiftDao: ShiftDao,
    private val syncQueueDao: SyncQueueDao,
    private val driverOpsApi: DriverOpsApiService,
    private val podApi: PodApiService
) {
    /** Enqueue an item AND immediately kick a one-time worker so it ships
     *  within seconds of network return — not 15 min later on the next
     *  periodic tick. */
    private suspend fun enqueueAndKick(item: SyncQueueEntity) {
        syncQueueDao.enqueue(item)
        OutboundSyncWorker.kickOnce(context)
    }

    fun observeTask(taskId: String): Flow<TaskEntity?> = taskDao.getByIdAsFlow(taskId)

    /**
     * Transitions task to a new status locally and on the backend.
     * For IN_PROGRESS (arrival), calls PUT /v1/tasks/:id/start.
     * Falls back to sync queue on network error.
     */
    suspend fun transitionTask(taskId: String, newStatus: TaskStatus) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, newStatus)) return
        taskDao.updateStatus(taskId, newStatus)

        try {
            when (newStatus) {
                TaskStatus.IN_PROGRESS -> driverOpsApi.startTask(taskId)
                else -> Unit // COMPLETED handled by submitPod; FAILED by failTask
            }
        } catch (e: Exception) {
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.TASK_STATUS_UPDATE,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "status" to newStatus.name)
                    ),
                    createdAt = System.currentTimeMillis()
                )
            )
        }

        val shift = shiftDao.getActiveShiftOnce() ?: return
        when (newStatus) {
            TaskStatus.COMPLETED -> shiftDao.incrementCompleted(shift.id)
            TaskStatus.FAILED, TaskStatus.RETURNED -> shiftDao.incrementFailed(shift.id)
            else -> Unit
        }
    }

    /**
     * Full POD flow for delivery completion:
     * 1. POST /v1/pods — initiate, get pod_id
     * 2. PUT /v1/pods/:id/signature — attach signature if provided
     * 3. PUT /v1/pods/:id/submit — finalise with COD amount / OTP
     * 4. PUT /v1/tasks/:id/complete — mark task done with pod_id
     *
     * Persists locally first so data isn't lost if network fails mid-flow.
     * On error, enqueues POD_SUBMIT for retry via OutboundSyncWorker.
     *
     * @return pod_id on success, null on failure (error is enqueued for retry)
     */
    suspend fun submitPod(
        taskId: String,
        shipmentId: String,
        recipientName: String,
        captureLat: Double,
        captureLng: Double,
        photoPath: String?,
        signaturePath: String?,
        otpCode: String?,
        codCollectedCents: Long?,
        requiresPhoto: Boolean = true,
        requiresSignature: Boolean = true,
    ): String? {
        // Persist locally first
        podDao.insert(
            PodEntity(
                taskId = taskId,
                photoPath = photoPath,
                signaturePath = signaturePath,
                otpToken = otpCode,
                capturedAt = System.currentTimeMillis()
            )
        )

        // GPS unavailable (0,0) means the backend geofence check will fail. Surface
        // the error immediately rather than letting the server return 422.
        if (captureLat == 0.0 && captureLng == 0.0) {
            throw IllegalStateException("GPS location is unavailable. Move to an area with signal and try again.")
        }

        return try {
            // 1. Initiate
            val initiateResp = podApi.initiate(
                InitiatePodRequest(
                    shipmentId = shipmentId,
                    taskId = taskId,
                    recipientName = recipientName,
                    captureLat = captureLat,
                    captureLng = captureLng,
                    deliveryLat = captureLat,
                    deliveryLng = captureLng,
                    requiresPhoto = requiresPhoto,
                    requiresSignature = requiresSignature,
                )
            )
            val podId = initiateResp.data.podId

            // Geofence check: backend compares captureLat/Lng against the stored
            // delivery address. If the driver is too far away the submit step will
            // 422. Fail early with a clear message so the driver knows to move closer.
            if (!initiateResp.data.geofenceVerified) {
                throw IllegalStateException("You are not close enough to the delivery address. Move within 200 m and try again.")
            }

            // 2. Attach signature if provided (base64-encode from file)
            if (signaturePath != null) {
                val sigFile = File(signaturePath)
                if (sigFile.exists()) {
                    val base64 = Base64.encodeToString(sigFile.readBytes(), Base64.NO_WRAP)
                    podApi.attachSignature(podId, AttachSignatureRequest(signatureData = base64))
                }
            }

            // 3. Submit POD
            podApi.submit(podId, SubmitPodRequest(codCollectedCents = codCollectedCents, otpCode = otpCode))

            // 4. Complete the task
            driverOpsApi.completeTask(taskId, CompleteTaskRequest(podId = podId, codCollectedCents = codCollectedCents))

            // Mark local POD as synced
            podDao.markSynced(taskId)
            taskDao.updateStatus(taskId, TaskStatus.COMPLETED)
            val shift = shiftDao.getActiveShiftOnce()
            if (shift != null) shiftDao.incrementCompleted(shift.id)

            podId
        } catch (e: Exception) {
            // Surface the failure so UI/logcat show *why* the POD didn't sync,
            // instead of silently pretending it succeeded. The sync queue retry
            // still runs, but the user gets a real error now.
            android.util.Log.e("DeliveryRepository", "submitPod failed: ${e.javaClass.simpleName}: ${e.message}", e)
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.POD_SUBMIT,
                    payloadJson = Json.encodeToString(mapOf("taskId" to taskId)),
                    createdAt = System.currentTimeMillis()
                )
            )
            throw e
        }
    }

    suspend fun getActiveShiftId(): String? = shiftDao.getActiveShiftOnce()?.id

    suspend fun saveFailureReason(taskId: String, reason: String) {
        taskDao.updateFailureReason(taskId, reason)
        taskDao.incrementAttemptCount(taskId)
    }

    /**
     * Fail a delivery task — calls backend directly then enqueues for retry.
     */
    suspend fun failTask(taskId: String, reason: String) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, TaskStatus.FAILED)) return
        taskDao.updateStatus(taskId, TaskStatus.FAILED)
        taskDao.updateFailureReason(taskId, reason)
        taskDao.incrementAttemptCount(taskId)

        try {
            driverOpsApi.failTask(taskId, io.logisticos.driver.core.network.service.FailTaskRequest(reason = reason))
        } catch (e: Exception) {
            enqueueAndKick(
                SyncQueueEntity(
                    action = SyncAction.TASK_STATUS_UPDATE,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "status" to TaskStatus.FAILED.name, "reason" to reason)
                    ),
                    createdAt = System.currentTimeMillis()
                )
            )
        }

        val shift = shiftDao.getActiveShiftOnce()
        if (shift != null) shiftDao.incrementFailed(shift.id)
    }

    /**
     * Records that the driver collected COD locally so the home screen
     * running-total updates immediately. The actual COD value is sent to the
     * backend as `cod_collected_cents` on the completeTask call inside
     * submitPod — no separate sync action is needed (and the previous
     * COD_CONFIRM enqueue had no backend handler, so it was being silently
     * dropped by OutboundSyncWorker).
     */
    suspend fun confirmCod(shiftId: String, taskId: String, amount: Double) {
        shiftDao.addCodCollected(shiftId, amount)
    }
}
