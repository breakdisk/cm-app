package io.logisticos.driver.feature.pickup.data

import io.logisticos.driver.core.database.dao.SyncQueueDao
import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import io.logisticos.driver.feature.delivery.domain.TaskStateMachine
import kotlinx.coroutines.flow.Flow
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import javax.inject.Inject

class PickupRepository @Inject constructor(
    private val taskDao: TaskDao,
    private val syncQueueDao: SyncQueueDao
) {
    fun observeTask(taskId: String): Flow<TaskEntity?> = taskDao.getByIdAsFlow(taskId)

    suspend fun confirmPickup(taskId: String, photoPath: String?) {
        val task = taskDao.getById(taskId) ?: return
        if (!TaskStateMachine.canTransition(task.status, TaskStatus.COMPLETED)) return
        taskDao.updateStatus(taskId, TaskStatus.COMPLETED)
        syncQueueDao.enqueue(
            SyncQueueEntity(
                action = SyncAction.TASK_STATUS_UPDATE,
                payloadJson = Json.encodeToString(
                    mapOf(
                        "taskId"    to taskId,
                        "status"    to TaskStatus.COMPLETED.name,
                        "photoPath" to (photoPath ?: "")
                    )
                ),
                createdAt = System.currentTimeMillis()
            )
        )
        if (photoPath != null) {
            syncQueueDao.enqueue(
                SyncQueueEntity(
                    action = SyncAction.POD_SUBMIT,
                    payloadJson = Json.encodeToString(mapOf("taskId" to taskId, "photoPath" to photoPath)),
                    createdAt = System.currentTimeMillis()
                )
            )
        }
    }

    suspend fun transitionToInProgress(taskId: String) {
        val task = taskDao.getById(taskId) ?: return
        if (TaskStateMachine.canTransition(task.status, TaskStatus.IN_PROGRESS)) {
            taskDao.updateStatus(taskId, TaskStatus.IN_PROGRESS)
        }
    }
}
