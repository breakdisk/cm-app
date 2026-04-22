package io.logisticos.driver.feature.delivery.data

import android.util.Base64
import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.PodEntity
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
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
    private val taskDao: TaskDao,
    private val podDao: PodDao,
    private val shiftDao: ShiftDao,
    private val syncQueueDao: SyncQueueDao,
    private val driverOpsApi: DriverOpsApiService,
    private val podApi: PodApiService
) {
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
            syncQueueDao.enqueue(
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
        codCollectedCents: Long?
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
                    deliveryLng = captureLng
                )
            )
            val podId = initiateResp.data.podId

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
            syncQueueDao.enqueue(
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
            syncQueueDao.enqueue(
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

    suspend fun confirmCod(shiftId: String, taskId: String, amount: Double) {
        shiftDao.addCodCollected(shiftId, amount)
        syncQueueDao.enqueue(
            SyncQueueEntity(
                action = SyncAction.COD_CONFIRM,
                payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "amount" to amount.toString())),
                createdAt = System.currentTimeMillis()
            )
        )
    }
}
