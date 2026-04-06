package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.TaskEntity
import io.logisticos.driver.core.database.entity.TaskStatus
import kotlinx.coroutines.flow.Flow

@Dao
interface TaskDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(task: TaskEntity)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAll(tasks: List<TaskEntity>)

    @Query("SELECT * FROM tasks WHERE id = :id")
    suspend fun getById(id: String): TaskEntity?

    @Query("SELECT * FROM tasks WHERE shiftId = :shiftId ORDER BY stopOrder ASC")
    fun getTasksForShift(shiftId: String): Flow<List<TaskEntity>>

    @Query("UPDATE tasks SET status = :status WHERE id = :taskId")
    suspend fun updateStatus(taskId: String, status: TaskStatus)

    @Query("UPDATE tasks SET stopOrder = :order WHERE id = :taskId")
    suspend fun updateStopOrder(taskId: String, order: Int)

    @Query("UPDATE tasks SET attemptCount = attemptCount + 1 WHERE id = :taskId")
    suspend fun incrementAttemptCount(taskId: String)

    @Query("DELETE FROM tasks WHERE shiftId = :shiftId")
    suspend fun deleteForShift(shiftId: String)
}
