package io.logisticos.driver.feature.pickup.data

import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.core.network.service.CompleteTaskRequest
import io.logisticos.driver.core.network.service.DriverOpsApiService
import io.logisticos.driver.feature.delivery.domain.TaskStateMachine
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject

class PickupRepository @Inject constructor(
    private val taskDao: TaskDao,
    private val syncQueueDao: SyncQueueDao,
    private val driverOpsApi: DriverOpsApiService
) {
    fun observeTask(taskId: String): Flow<TaskEntity?> = taskDao.getByIdAsFlow(taskId)

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
            syncQueueDao.enqueue(
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
     * Completes the pickup: updates local DB then calls backend directly.
     * Pickup tasks don't require a POD — backend accepts empty body.
     * Photo is enqueued separately as POD_SUBMIT for offline resilience.
     */
    suspend fun confirmPickup(taskId: String, photoPath: String?) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, TaskStatus.COMPLETED)) return
        taskDao.updateStatus(taskId, TaskStatus.COMPLETED)

        try {
            driverOpsApi.completeTask(taskId, CompleteTaskRequest())
        } catch (e: Exception) {
            syncQueueDao.enqueue(
                SyncQueueEntity(
                    action = SyncAction.TASK_STATUS_UPDATE,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "status" to TaskStatus.COMPLETED.name)
                    ),
                    createdAt = System.currentTimeMillis()
                )
            )
        }

        if (photoPath != null) {
            syncQueueDao.enqueue(
                SyncQueueEntity(
                    action = SyncAction.POD_SUBMIT,
                    payloadJson = Json.encodeToString(
                        mapOf("taskId" to taskId, "photoPath" to photoPath)
                    ),
                    createdAt = System.currentTimeMillis()
                )
            )
        }
    }
}
