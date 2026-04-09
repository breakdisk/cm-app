package io.logisticos.driver.feature.delivery.data

import io.logisticos.driver.core.database.dao.PodDao
import io.logisticos.driver.core.database.dao.ShiftDao
import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.PodEntity
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.domain.TaskStateMachine
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject

class DeliveryRepository @Inject constructor(
    private val taskDao: TaskDao,
    private val podDao: PodDao,
    private val shiftDao: ShiftDao,
    private val syncQueueDao: SyncQueueDao
) {
    fun observeTask(taskId: String): Flow<TaskEntity?> = taskDao.getByIdAsFlow(taskId)

    suspend fun transitionTask(taskId: String, newStatus: TaskStatus) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, newStatus)) return
        taskDao.updateStatus(taskId, newStatus)
        syncQueueDao.enqueue(
            SyncQueueEntity(
                action = SyncAction.TASK_STATUS_UPDATE,
                payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "status" to newStatus.name)),
                createdAt = System.currentTimeMillis()
            )
        )
        val shift = shiftDao.getActiveShiftOnce() ?: return
        when (newStatus) {
            TaskStatus.COMPLETED -> shiftDao.incrementCompleted(shift.id)
            TaskStatus.FAILED, TaskStatus.RETURNED -> shiftDao.incrementFailed(shift.id)
            else -> Unit
        }
    }

    suspend fun savePod(taskId: String, photoPath: String?, signaturePath: String?, otpToken: String?) {
        podDao.insert(
            PodEntity(
                taskId = taskId,
                photoPath = photoPath,
                signaturePath = signaturePath,
                otpToken = otpToken,
                capturedAt = System.currentTimeMillis()
            )
        )
        syncQueueDao.enqueue(
            SyncQueueEntity(
                action = SyncAction.POD_SUBMIT,
                payloadJson = Json.encodeToString(mapOf("taskId" to taskId)),
                createdAt = System.currentTimeMillis()
            )
        )
    }

    suspend fun getActiveShiftId(): String? = shiftDao.getActiveShiftOnce()?.id

    suspend fun saveFailureReason(taskId: String, reason: String) {
        taskDao.updateFailureReason(taskId, reason)
        taskDao.incrementAttemptCount(taskId)
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
