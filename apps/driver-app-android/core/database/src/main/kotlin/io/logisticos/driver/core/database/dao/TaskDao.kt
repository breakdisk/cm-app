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

    @Query("UPDATE tasks SET failureReason = :reason WHERE id = :taskId")
    suspend fun updateFailureReason(taskId: String, reason: String)

    @Query("DELETE FROM tasks WHERE shiftId = :shiftId")
    suspend fun deleteForShift(shiftId: String)

    /**
     * Remove tasks that are no longer in the server's task list for this
     * shift. Called from ShiftRepository.syncShift after fetching the
     * authoritative payload — without this, completed/cancelled/reassigned
     * tasks linger locally forever and the route screen renders ghosts.
     */
    @Query("DELETE FROM tasks WHERE shiftId = :shiftId AND id NOT IN (:keepIds)")
    suspend fun pruneShiftTasks(shiftId: String, keepIds: List<String>)

    @Query("SELECT * FROM tasks WHERE shiftId = :shiftId")
    suspend fun getTasksForShiftOnce(shiftId: String): List<TaskEntity>

    @Query("SELECT * FROM tasks WHERE id = :taskId LIMIT 1")
    fun getByIdAsFlow(taskId: String): Flow<TaskEntity?>
}
