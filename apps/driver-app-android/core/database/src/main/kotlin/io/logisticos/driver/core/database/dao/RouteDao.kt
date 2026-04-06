package io.logisticos.driver.core.database.dao

import androidx.room.*
import io.logisticos.driver.core.database.entity.RouteEntity
import kotlinx.coroutines.flow.Flow

@Dao
interface RouteDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(route: RouteEntity)

    @Query("SELECT * FROM routes WHERE taskId = :taskId")
    suspend fun getForTask(taskId: String): RouteEntity?

    @Query("SELECT * FROM routes WHERE taskId = :taskId")
    fun getByTaskId(taskId: String): Flow<RouteEntity?>

    @Query("DELETE FROM routes WHERE taskId = :taskId")
    suspend fun deleteForTask(taskId: String)
}
