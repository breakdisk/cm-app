package io.logisticos.driver.feature.route.data

import io.logisticos.driver.core.database.dao.TaskDao
import io.logisticos.driver.core.database.entity.TaskEntity
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RouteRepository @Inject constructor(
    private val taskDao: TaskDao
) {
    fun observeTasks(shiftId: String): Flow<List<TaskEntity>> =
        taskDao.getTasksForShift(shiftId)

    suspend fun updateStopOrder(taskId: String, order: Int) =
        taskDao.updateStopOrder(taskId, order)
}
